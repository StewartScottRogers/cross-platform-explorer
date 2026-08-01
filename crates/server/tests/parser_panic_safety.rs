//! Parser panic-safety property harness (CPE-1169, epic CPE-1002 "File inspection & safety utilities").
//!
//! Every byte-parser entrypoint in `cpe-server` already documents (and, module-by-module, unit-tests)
//! that malformed/truncated/adversarial input yields a graceful empty/`None`/`Err` result rather than a
//! panic — that's *the* guardrail behind `CLAUDE.md`'s "filesystem commands skip entries they can't read
//! rather than failing the whole listing": a single panicking parser would take the whole directory
//! listing (or column/preview pass) down with it. This file is the **cross-cutting** proof: ONE
//! table-driven harness that feeds *every* entrypoint the same adversarial battery — empty, 1-byte,
//! truncated at every header boundary, all-zeros, all-`0xFF`, seeded pseudo-random, valid-magic-then-
//! garbage, and overflowing length fields — through [`std::panic::catch_unwind`], so a regression names
//! the exact entrypoint + input class that broke rather than just "some test failed".
//!
//! Deterministic only: [`lcg_bytes`] is a tiny inline linear-congruential generator, not the `rand` crate
//! (no new dependency). Each `#[test]` below covers one entrypoint (or, for the [`extract_column`]
//! dispatcher, one [`MetaColumn`] family) so a failure's test name is already half the diagnosis; the
//! panic message from [`assert_no_panic`] gives the rest (entrypoint + adversarial input class).
//!
//! This harness only checks the *empty*-input case against each function's documented graceful sentinel
//! (safe and unambiguous for every entrypoint here — confirmed by reading each one's doc comment and
//! existing unit tests) plus "never panics" for the rest of the battery. It deliberately does NOT assert
//! a graceful sentinel for every adversarial class against every entrypoint: several entrypoints have
//! legitimate magic-byte collisions with parts of the battery (e.g. `file_type::detect_type` reads an
//! all-`0xFF` two-byte prefix as a valid MP3 frame sync, per its own documented ordering) where asserting
//! "must be empty" would be a wrong assertion, not a real bug. Panic-safety — this ticket's actual goal —
//! is still fully asserted across the whole battery for every entrypoint.

use std::panic::{self, AssertUnwindSafe};

use cpe_server::archive_format::{detect_format, ArchiveFormat};
use cpe_server::column_extract::{extract_column, read_audio_tags, MetaColumn};
use cpe_server::doc_column::doc_pages_cell;
use cpe_server::file_type::{detect_type, mismatch};
use cpe_server::image_column::image_dimensions_cell;
use cpe_server::inspect::inspect_bytes;
use cpe_server::media_column::AudioColumn;
use cpe_server::media_meta::{read_all, write_back};
use cpe_server::media_meta_edit::MetaEdit;
use cpe_server::media_meta_read::{parse_vorbis_comment, read_exif, read_flac, read_id3v2, read_ogg, read_pdf};
use cpe_server::media_meta_write::{write_flac, write_id3v2};
use cpe_server::metadata_column::CellValue;
use cpe_server::perceptual::phash;
use cpe_server::text_encoding::{detect_encoding, EncodingGuess};
use cpe_server::thumb_orient::read_exif_orientation;
use cpe_server::video_column::video_cell;
use cpe_server::video_meta_read::read_mp4;

// ---------------------------------------------------------------------------------------------
// Deterministic pseudo-random bytes (no `rand` dependency)
// ---------------------------------------------------------------------------------------------

/// A tiny seeded linear-congruential generator (Numerical Recipes' constants) — NOT cryptographic, just
/// a reproducible byte stream so "seeded pseudo-random" input is deterministic across runs/machines
/// without pulling in the `rand` crate (the ticket forbids new dependencies).
fn lcg_bytes(seed: u64, len: usize) -> Vec<u8> {
    let mut state = seed ^ 0x9E37_79B9_7F4A_7C15;
    (0..len)
        .map(|_| {
            state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            (state >> 33) as u8
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// The shared adversarial battery
// ---------------------------------------------------------------------------------------------

/// One adversarial input, named for the panic message when it breaks something.
struct Case {
    class: String,
    bytes: Vec<u8>,
}

fn case(class: impl Into<String>, bytes: Vec<u8>) -> Case {
    Case { class: class.into(), bytes }
}

/// Build the shared adversarial battery for one entrypoint: `magic` is its leading signature (empty if
/// it has none), `header_len` is roughly the smallest structurally-meaningful header the parser reads
/// before it can report anything (used to aim the truncation/overflow boundaries at the interesting
/// spot rather than an arbitrary one).
fn battery(magic: &[u8], header_len: usize) -> Vec<Case> {
    let mut cases = Vec::new();
    cases.push(case("empty", Vec::new()));
    cases.push(case("one_byte_0x00", vec![0x00]));
    cases.push(case("one_byte_0xff", vec![0xFF]));

    for &n in &[2usize, 4, 8, 16, 32, 64, 128, 256, 512, 1024] {
        cases.push(case(format!("all_zeros_{n}"), vec![0u8; n]));
        cases.push(case(format!("all_0xff_{n}"), vec![0xFFu8; n]));
        cases.push(case(format!("seeded_random_{n}"), lcg_bytes(0x00C0_FFEE ^ n as u64, n)));
    }

    // Truncated at every prefix length of the magic itself (the classic "cut mid-signature").
    for cut in 0..=magic.len() {
        cases.push(case(format!("truncated_magic_at_{cut}"), magic[..cut].to_vec()));
    }
    // Truncated right around the declared header boundary (magic present, body cut short/long).
    let boundary = header_len.max(magic.len());
    for cut in boundary.saturating_sub(2)..=boundary + 2 {
        let mut buf = magic.to_vec();
        buf.resize(cut, 0);
        cases.push(case(format!("truncated_at_header_boundary_{cut}"), buf));
    }

    // Valid-magic-then-garbage: the real signature followed by an adversarial tail.
    for &n in &[8usize, 64, 256] {
        cases.push(case(format!("magic_then_zeros_{n}"), [magic, &vec![0u8; n][..]].concat()));
        cases.push(case(format!("magic_then_0xff_{n}"), [magic, &vec![0xFFu8; n][..]].concat()));
        cases.push(case(
            format!("magic_then_random_{n}"),
            [magic, &lcg_bytes(0xBAD_F00D ^ n as u64, n)[..]].concat(),
        ));
    }

    // Overflowing length fields: a declared size/length of all-1 bits right after the magic, in both
    // 32-bit and 64-bit, big- and little-endian — covers the common "size prefix right after the
    // signature" convention (ID3 syncsafe size, FLAC/MP4 box length, OGG page fields, PDF window
    // lengths, …) without needing per-format offset knowledge.
    let overflow_fields: [(&str, Vec<u8>); 4] = [
        ("u32_be_max", 0xFFFF_FFFFu32.to_be_bytes().to_vec()),
        ("u32_le_max", 0xFFFF_FFFFu32.to_le_bytes().to_vec()),
        ("u64_be_max", 0xFFFF_FFFF_FFFF_FFFFu64.to_be_bytes().to_vec()),
        ("u64_le_max", 0xFFFF_FFFF_FFFF_FFFFu64.to_le_bytes().to_vec()),
    ];
    for (label, overflow) in overflow_fields {
        let mut buf = magic.to_vec();
        buf.extend_from_slice(&overflow);
        buf.extend_from_slice(&lcg_bytes(0xF00D, 32));
        cases.push(case(format!("overflowing_length_field_{label}"), buf));
    }

    cases
}

// ---------------------------------------------------------------------------------------------
// catch_unwind harness
// ---------------------------------------------------------------------------------------------

/// Run `f` under `catch_unwind`. Any panic — the parser's own, or a graceful-contract assertion failing
/// inside `f` — is re-raised as a single, clearly-attributed failure naming the entrypoint and the
/// adversarial input class that triggered it, which is the whole point of this harness (CPE-1169: "a
/// failure names the entrypoint + input class").
fn assert_no_panic(entrypoint: &str, input_class: &str, f: impl FnOnce()) {
    if let Err(payload) = panic::catch_unwind(AssertUnwindSafe(f)) {
        let msg = payload
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "<non-string panic payload>".to_string());
        panic!(
            "parser panic-safety violation: entrypoint `{entrypoint}` on adversarial input class \
             `{input_class}` did not return gracefully: {msg}"
        );
    }
}

/// Drive one entrypoint's shared battery through `check`, which calls the real function and may assert
/// its documented graceful-return contract (most usefully on the unambiguous `bytes.is_empty()` case —
/// see the module doc comment for why the harness doesn't over-assert on every other class).
fn run_battery(name: &str, magic: &[u8], header_len: usize, check: impl Fn(&[u8])) {
    for c in battery(magic, header_len) {
        assert_no_panic(name, &c.class, || check(&c.bytes));
    }
}

// ---------------------------------------------------------------------------------------------
// Entrypoints: magic-byte / true-type detection
// ---------------------------------------------------------------------------------------------

#[test]
fn detect_type_never_panics() {
    run_battery("file_type::detect_type", &[], 0, |b| {
        let r = detect_type(b);
        if b.is_empty() {
            assert!(r.is_none(), "detect_type(empty) must be None");
        }
    });
}

#[test]
fn mismatch_never_panics() {
    run_battery("file_type::mismatch", &[], 0, |b| {
        let r = mismatch(b, "exe");
        if b.is_empty() {
            assert!(r.is_none(), "mismatch(empty, _) must be None (nothing detected to compare)");
        }
    });
}

#[test]
fn archive_detect_format_never_panics() {
    run_battery("archive_format::detect_format", &[], 0, |b| {
        let r = detect_format(b, "unnamed.bin");
        if b.is_empty() {
            assert_eq!(r, ArchiveFormat::Unknown, "detect_format(empty, no-hint-name) must be Unknown");
        }
    });
}

// ---------------------------------------------------------------------------------------------
// Entrypoints: audio metadata read codecs
// ---------------------------------------------------------------------------------------------

#[test]
fn read_id3v2_never_panics() {
    run_battery("media_meta_read::read_id3v2", b"ID3", 10, |b| {
        let r = read_id3v2(b);
        if b.is_empty() {
            assert!(r.is_empty(), "read_id3v2(empty) must be empty");
        }
    });
}

#[test]
fn read_flac_never_panics() {
    run_battery("media_meta_read::read_flac", b"fLaC", 4, |b| {
        let r = read_flac(b);
        if b.is_empty() {
            assert!(r.is_empty(), "read_flac(empty) must be empty");
        }
    });
}

#[test]
fn read_ogg_never_panics() {
    run_battery("media_meta_read::read_ogg", b"OggS", 27, |b| {
        let r = read_ogg(b);
        if b.is_empty() {
            assert!(r.is_empty(), "read_ogg(empty) must be empty");
        }
    });
}

#[test]
fn parse_vorbis_comment_never_panics() {
    run_battery("media_meta_read::parse_vorbis_comment", &[], 4, |b| {
        let r = parse_vorbis_comment(b);
        if b.is_empty() {
            assert!(r.is_empty(), "parse_vorbis_comment(empty) must be empty");
        }
    });
}

// ---------------------------------------------------------------------------------------------
// Entrypoints: image metadata (EXIF, header dimensions, perceptual hash, orientation)
// ---------------------------------------------------------------------------------------------

#[test]
fn read_exif_never_panics() {
    // A JPEG SOI is one realistic container magic `exif::Reader` auto-detects; the reader also handles
    // TIFF/PNG/WebP/HEIF, but one representative magic is enough for the truncation/overflow classes.
    run_battery("media_meta_read::read_exif", &[0xFF, 0xD8, 0xFF], 4, |b| {
        let r = read_exif(b);
        if b.is_empty() {
            assert!(r.is_empty(), "read_exif(empty) must be empty");
        }
    });
}

#[test]
fn image_dimensions_cell_never_panics() {
    run_battery("image_column::image_dimensions_cell", &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A], 8, |b| {
        let r = image_dimensions_cell(b);
        if b.is_empty() {
            assert_eq!(r, CellValue::Empty, "image_dimensions_cell(empty) must be Empty");
        }
    });
}

#[test]
fn phash_never_panics() {
    run_battery("perceptual::phash", &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A], 8, |b| {
        let r = phash(b);
        if b.is_empty() {
            assert!(r.is_none(), "phash(empty) must be None");
        }
    });
}

#[test]
fn read_exif_orientation_never_panics() {
    run_battery("thumb_orient::read_exif_orientation", &[0xFF, 0xD8, 0xFF], 4, |b| {
        let r = read_exif_orientation(b);
        if b.is_empty() {
            assert!(r.is_none(), "read_exif_orientation(empty) must be None");
        }
    });
}

// ---------------------------------------------------------------------------------------------
// Entrypoints: document (PDF) read codecs
// ---------------------------------------------------------------------------------------------

#[test]
fn read_pdf_never_panics() {
    run_battery("media_meta_read::read_pdf", b"%PDF", 5, |b| {
        let r = read_pdf(b);
        if b.is_empty() {
            assert!(r.is_empty(), "read_pdf(empty) must be empty");
        }
    });
}

#[test]
fn doc_pages_cell_never_panics() {
    run_battery("doc_column::doc_pages_cell", b"%PDF", 4, |b| {
        let r = doc_pages_cell(b);
        if b.is_empty() {
            assert_eq!(r, CellValue::Empty, "doc_pages_cell(empty) must be Empty");
        }
    });
}

// ---------------------------------------------------------------------------------------------
// Entrypoints: video (ISO-BMFF / MP4) read codecs
// ---------------------------------------------------------------------------------------------

#[test]
fn read_mp4_never_panics() {
    // No single fixed leading magic (an ISO-BMFF `moov` box can appear after other boxes); the battery
    // still exercises truncation/overflow/garbage without one, and the module's own unit tests already
    // cover realistic box-tree fixtures exhaustively (see `video_meta_read.rs`).
    run_battery("video_meta_read::read_mp4", &[], 8, |b| {
        let r = read_mp4(b);
        if b.is_empty() {
            assert!(r.is_empty(), "read_mp4(empty) must be empty");
        }
    });
}

#[test]
fn video_cell_never_panics() {
    run_battery("video_column::video_cell", &[], 8, |b| {
        let r = video_cell(b);
        if b.is_empty() {
            assert_eq!(r, CellValue::Empty, "video_cell(empty) must be Empty");
        }
    });
}

// ---------------------------------------------------------------------------------------------
// Entrypoints: write-back codecs
// ---------------------------------------------------------------------------------------------

#[test]
fn write_id3v2_never_panics() {
    run_battery("media_meta_write::write_id3v2", b"ID3", 10, |b| {
        // write_id3v2 always succeeds (it builds a fresh tag regardless of `orig`'s shape); the only
        // universal graceful contract is "never panics", which `run_battery`/`assert_no_panic` already
        // enforces around this call.
        let _ = write_id3v2(b, &[]);
    });
}

#[test]
fn write_flac_never_panics() {
    run_battery("media_meta_write::write_flac", b"fLaC", 4, |b| {
        let r = write_flac(b, &[]);
        if b.is_empty() {
            assert_eq!(r, b.to_vec(), "write_flac(empty, _) must return the input unchanged");
        }
    });
}

// ---------------------------------------------------------------------------------------------
// Entrypoints: the dispatcher layer (media_meta / column_extract) — same codecs, reached the way the
// app actually calls them (by extension), so a dispatch-layer regression is covered too.
// ---------------------------------------------------------------------------------------------

#[test]
fn media_meta_read_all_never_panics() {
    run_battery("media_meta::read_all(mp3)", b"ID3", 10, |b| {
        let r = read_all("mp3", b);
        if b.is_empty() {
            assert!(r.is_empty(), "read_all(\"mp3\", empty) must be empty");
        }
    });
}

#[test]
fn media_meta_write_back_never_panics() {
    let edits: Vec<MetaEdit> = Vec::new();
    run_battery("media_meta::write_back(mp3)", b"ID3", 10, |b| {
        // mp3 always has a writer (write_id3v2 never errors), so this is provably `Ok` for every input in
        // the battery, not just the empty case.
        let r = write_back("mp3", b, &edits);
        assert!(r.is_ok(), "write_back(\"mp3\", _, []) must always succeed (mp3 always has a writer)");
    });
}

#[test]
fn column_extract_read_audio_tags_never_panics() {
    run_battery("column_extract::read_audio_tags(mp3)", b"ID3", 10, |b| {
        let r = read_audio_tags("mp3", b);
        if b.is_empty() {
            assert!(r.is_empty(), "read_audio_tags(\"mp3\", empty) must be empty");
        }
    });
}

#[test]
fn column_extract_extract_column_audio_never_panics() {
    run_battery("column_extract::extract_column(Audio(Track), mp3)", b"ID3", 10, |b| {
        let r = extract_column("mp3", b, MetaColumn::Audio(AudioColumn::Track));
        if b.is_empty() {
            assert_eq!(r, CellValue::Empty);
        }
    });
}

#[test]
fn column_extract_extract_column_image_never_panics() {
    run_battery(
        "column_extract::extract_column(ImageDimensions, png)",
        &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A],
        8,
        |b| {
            let r = extract_column("png", b, MetaColumn::ImageDimensions);
            if b.is_empty() {
                assert_eq!(r, CellValue::Empty);
            }
        },
    );
}

#[test]
fn column_extract_extract_column_doc_never_panics() {
    run_battery("column_extract::extract_column(DocPages, pdf)", b"%PDF", 5, |b| {
        let r = extract_column("pdf", b, MetaColumn::DocPages);
        if b.is_empty() {
            assert_eq!(r, CellValue::Empty);
        }
    });
}

#[test]
fn column_extract_extract_column_video_never_panics() {
    run_battery("column_extract::extract_column(VideoDuration, mp4)", &[], 8, |b| {
        let r = extract_column("mp4", b, MetaColumn::VideoDuration);
        if b.is_empty() {
            assert_eq!(r, CellValue::Empty);
        }
    });
}

#[test]
fn column_extract_extract_column_applies_to_all_never_panics() {
    // The magic-byte-detector columns (CPE-1166) are file-agnostic — no extension gate — so they're
    // exercised directly against the raw battery with no extension at all.
    for col in [MetaColumn::TrueType, MetaColumn::TypeMismatch, MetaColumn::TextEncoding, MetaColumn::LineEndings] {
        run_battery(&format!("column_extract::extract_column({col:?}, no-ext)"), &[], 0, |b| {
            let _ = extract_column("", b, col);
        });
    }
}

// ---------------------------------------------------------------------------------------------
// Entrypoints: text encoding + composed file inspection
// ---------------------------------------------------------------------------------------------

#[test]
fn detect_encoding_never_panics() {
    run_battery("text_encoding::detect_encoding", &[], 0, |b| {
        let r = detect_encoding(b);
        if b.is_empty() {
            assert_eq!(r, EncodingGuess::Empty, "detect_encoding(empty) must be Empty");
        }
    });
}

#[test]
fn inspect_bytes_never_panics() {
    run_battery("inspect::inspect_bytes", &[], 0, |b| {
        let r = inspect_bytes("adversarial.bin", b);
        if b.is_empty() {
            assert_eq!(r.encoding, EncodingGuess::Empty.label());
            assert!(r.line_endings.is_none());
            assert!(r.file_type.is_none());
            assert!(r.type_mismatch.is_none());
        }
    });
}
