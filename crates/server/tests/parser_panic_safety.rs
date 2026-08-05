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
//! Deterministic only: the battery's `lcg_bytes` is a tiny inline linear-congruential generator, not the
//! `rand` crate (no new dependency). Each `#[test]` below covers one entrypoint (or, for the
//! [`extract_column`] dispatcher, one [`MetaColumn`] family) so a failure's test name is already half the
//! diagnosis; the panic message from `assert_no_panic` gives the rest (entrypoint + adversarial input
//! class).
//!
//! This harness only checks the *empty*-input case against each function's documented graceful sentinel
//! (safe and unambiguous for every entrypoint here — confirmed by reading each one's doc comment and
//! existing unit tests) plus "never panics" for the rest of the battery. It deliberately does NOT assert
//! a graceful sentinel for every adversarial class against every entrypoint: several entrypoints have
//! legitimate magic-byte collisions with parts of the battery (e.g. `file_type::detect_type` reads an
//! all-`0xFF` two-byte prefix as a valid MP3 frame sync, per its own documented ordering) where asserting
//! "must be empty" would be a wrong assertion, not a real bug. Panic-safety — this ticket's actual goal —
//! is still fully asserted across the whole battery for every entrypoint.
//!
//! The battery generator + `catch_unwind` harness itself lives in `tests/common/mod.rs` (CPE-1311
//! extracted it there) so `binary_data_preview_panic_safety.rs` — which drives the same battery against
//! *path-based* parsers by wrapping each case's bytes into a temp file — reuses it instead of duplicating
//! it.

mod common;
use common::{assert_no_panic, run_battery};

use cpe_server::archive_format::{detect_format, ArchiveFormat};
use cpe_server::column_extract::{extract_column, read_audio_tags, MetaColumn};
use cpe_server::doc_column::doc_pages_cell;
use cpe_server::file_type::{detect_type, mismatch};
use cpe_server::image_column::image_dimensions_cell;
use cpe_server::inspect::inspect_bytes;
use cpe_server::media_column::AudioColumn;
use cpe_server::media_meta::{read_all, write_back};
use cpe_server::media_meta_edit::{MetaEdit, MetaField};
use cpe_server::media_meta_read::{
    parse_vorbis_comment, read_exif, read_flac, read_id3v2, read_iptc, read_ogg, read_pdf, read_wav, read_xmp,
};
use cpe_server::media_meta_write::{
    write_exif, write_flac, write_id3v2, write_iptc, write_ogg, write_pdf, write_vorbis_comment, write_wav, write_xmp,
};
use cpe_server::metadata_column::CellValue;
use cpe_server::model_3d::read_model_info;
use cpe_server::perceptual::phash;
use cpe_server::text_encoding::{detect_encoding, EncodingGuess};
use cpe_server::thumb_orient::read_exif_orientation;
use cpe_server::video_column::video_cell;
use cpe_server::video_meta_read::read_mp4;
use cpe_server::video_meta_write::write_mp4;

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
    // Reach the FRAME LOOP, not just the header gate. `read_id3v2` returns early unless the 10-byte
    // header is present with major ∈ 2..=4, and it clamps frame parsing to `end = (10 + tag_size)`.
    // So a bare `b"ID3"` (or a valid header with a zero syncsafe size) leaves `end == 10` and the loop
    // never runs — hollow. Use a valid header whose syncsafe tag-size is maxed (`0x7F7F7F7F`) so `end`
    // clamps to the actual buffer length and the appended garbage in the `magic_then_*` /
    // `overflowing_length_field` cases is genuinely walked as frames. One header per major version
    // since the frame arithmetic differs (id/size widths, be_u24 vs be_u32 vs syncsafe28).
    for major in [2u8, 3, 4] {
        let header = [b'I', b'D', b'3', major, 0x00, 0x00, 0x7F, 0x7F, 0x7F, 0x7F];
        run_battery("media_meta_read::read_id3v2", &header, 10, |b| {
            let r = read_id3v2(b);
            if b.is_empty() {
                assert!(r.is_empty(), "read_id3v2(empty) must be empty");
            }
        });
    }
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

#[test]
fn read_wav_never_panics() {
    // `read_wav` gates on a 12-byte "RIFF" + size(4, unused by the parser) + "WAVE" header before it ever
    // walks the chunk tree, so a bare `b"RIFF"` (or `b"RIFF"` followed by garbage that doesn't happen to
    // spell "WAVE" at byte 8) returns early — hollow, same trap `read_id3v2_never_panics` and
    // `read_pdf_never_panics` avoid above. Use a full valid 12-byte header so every adversarial class
    // (crucially the `overflowing_length_field_*` ones) actually reaches the chunk-walk loop and lands on
    // a real chunk id/size pair — exactly the 4-byte little-endian chunk-size exposure this test guards.
    let header = *b"RIFF\x00\x00\x00\x00WAVE";
    run_battery("media_meta_read::read_wav", &header, 12, |b| {
        let r = read_wav(b);
        if b.is_empty() {
            assert!(r.is_empty(), "read_wav(empty) must be empty");
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

#[test]
fn read_xmp_never_panics() {
    // `read_xmp` has two entry paths (CPE-1291): a standalone `.xmp` sidecar (no fixed leading magic —
    // battery it with an empty magic, same as `read_mp4_never_panics`) and a JPEG APP1-embedded packet
    // (its own magic: SOI + the APP1 marker). Battery both so a regression in either path is caught.
    run_battery("media_meta_read::read_xmp(sidecar)", &[], 9, |b| {
        let r = read_xmp(b);
        if b.is_empty() {
            assert!(r.is_empty(), "read_xmp(empty) must be empty");
        }
    });
    run_battery("media_meta_read::read_xmp(jpeg-app1)", &[0xFF, 0xD8, 0xFF, 0xE1], 4, |b| {
        let r = read_xmp(b);
        if b.is_empty() {
            assert!(r.is_empty(), "read_xmp(empty) must be empty");
        }
    });
}

// ---------------------------------------------------------------------------------------------
// Entrypoints: document (PDF) read codecs
// ---------------------------------------------------------------------------------------------

#[test]
fn read_pdf_never_panics() {
    // `read_pdf` gates on `has_pdf_header`, which requires the full `%PDF-` signature (WITH the dash).
    // A bare `b"%PDF"` fails that check so the byte-scanning body (find_last_info_ref /
    // resolve_indirect_dict / extract_pdf_fields) never runs — hollow. Use `b"%PDF-"` so the
    // `magic_then_*` / `overflowing_length_field` cases carry an adversarial body into the real parser.
    run_battery("media_meta_read::read_pdf", b"%PDF-", 5, |b| {
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
// Entrypoints: 3D model geometry read
// ---------------------------------------------------------------------------------------------

#[test]
fn read_model_info_never_panics() {
    // `read_model_info` chains five format parsers (binary STL, ASCII STL, OBJ, GLB, glTF JSON) with no
    // single fixed leading magic shared across all of them (binary STL's 80-byte header is arbitrary
    // bytes, OBJ/ASCII-STL are plain text) — same "no fixed magic" situation as `read_mp4_never_panics`
    // above, so battery it with no magic. `header_len` is aimed at binary STL's 84-byte header (80-byte
    // header + the little-endian `u32` triangle count at offset 80..84 that drives `triangle_count * 50`
    // — the read's own overflowing-length-field boundary), since binary STL is checked first and is the
    // most structurally strict (and thus most panic-risky) of the five parsers.
    run_battery("model_3d::read_model_info", &[], 84, |b| {
        let r = read_model_info(b);
        if b.is_empty() {
            assert!(r.is_none(), "read_model_info(empty) must be None");
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

#[test]
fn write_wav_never_panics() {
    // `write_wav` takes (orig: &[u8], fields: &[MetaField]) and rejects non-WAV or truncated/corrupt
    // chunk trees gracefully (via `Err`, never a panic) — same 12-byte RIFF/WAVE header gate as
    // `read_wav_never_panics`. Feed a small fixed field set and fuzz the RIFF bytes. CPE-1314.
    let fields = vec![MetaField {
        group: "wav".to_string(),
        key: "Title".to_string(),
        value: "test title".to_string(),
        editable: true,
    }];
    let header = *b"RIFF\x00\x00\x00\x00WAVE";
    run_battery("media_meta_write::write_wav", &header, 12, |b| {
        let r = write_wav(b, &fields);
        if b.is_empty() {
            assert!(r.is_err(), "write_wav(empty, _) must return Err (not a WAV file)");
        }
    });
}

#[test]
fn write_pdf_never_panics() {
    // `write_pdf` takes (orig: &[u8], fields: &[MetaField]) and rejects a missing `%PDF-` header, a
    // missing/non-classic xref table, or a missing `/Root`/trailer gracefully (via `Err`, never a panic).
    // Same `%PDF-` magic as `read_pdf_never_panics`. Feed a small fixed field set and fuzz the PDF bytes.
    // CPE-1314.
    let fields = vec![MetaField {
        group: "pdf".to_string(),
        key: "Title".to_string(),
        value: "test title".to_string(),
        editable: true,
    }];
    run_battery("media_meta_write::write_pdf", b"%PDF-", 5, |b| {
        let r = write_pdf(b, &fields);
        if b.is_empty() {
            assert!(r.is_err(), "write_pdf(empty, _) must return Err (not a PDF)");
        }
    });
}

#[test]
fn write_iptc_never_panics() {
    // `write_iptc` takes (orig: &[u8], fields: &[MetaField]) and rejects non-JPEG/truncated input
    // gracefully (via `Err`, never a panic) — same JPEG SOI + APP1-ish magic as `read_iptc_never_panics`.
    // Feed a small fixed field set and fuzz the JPEG bytes. CPE-1314.
    let fields = vec![MetaField {
        group: "iptc".to_string(),
        key: "Headline".to_string(),
        value: "test headline".to_string(),
        editable: true,
    }];
    run_battery("media_meta_write::write_iptc", &[0xFF, 0xD8, 0xFF, 0xE1], 4, |b| {
        let r = write_iptc(b, &fields);
        if b.is_empty() {
            assert!(r.is_err(), "write_iptc(empty, _) must return Err (not a JPEG)");
        }
    });
}

#[test]
fn write_xmp_never_panics() {
    // `write_xmp` takes (orig: &[u8], fields: &[MetaField]) and rejects non-JPEG/truncated input
    // gracefully (via `Err`, never a panic) — unlike `read_xmp`, the write side is JPEG-only (no sidecar
    // path). Same JPEG magic as `read_xmp_never_panics`'s jpeg-app1 case. Feed a small fixed field set and
    // fuzz the JPEG bytes. CPE-1314.
    let fields = vec![MetaField {
        group: "xmp".to_string(),
        key: "Title".to_string(),
        value: "test title".to_string(),
        editable: true,
    }];
    run_battery("media_meta_write::write_xmp", &[0xFF, 0xD8, 0xFF, 0xE1], 4, |b| {
        let r = write_xmp(b, &fields);
        if b.is_empty() {
            assert!(r.is_err(), "write_xmp(empty, _) must return Err (not a JPEG)");
        }
    });
}

#[test]
fn read_iptc_never_panics() {
    // `read_iptc` expects a JPEG SOI marker (0xFF 0xD8) at the start, followed by APP13 segment
    // with Photoshop/IPTC data; gracefully returns empty if not present or malformed.
    run_battery("media_meta_read::read_iptc", &[0xFF, 0xD8, 0xFF, 0xE1], 4, |b| {
        let r = read_iptc(b);
        if b.is_empty() {
            assert!(r.is_empty(), "read_iptc(empty) must be empty");
        }
    });
}

#[test]
fn write_exif_never_panics() {
    // `write_exif` takes (orig: &[u8], fields: &[MetaField]) and rejects non-JPEG or truncated input
    // gracefully (via `Err`, never a panic). Feed a small fixed field set and fuzz the JPEG bytes.
    let fields = vec![MetaField {
        group: "exif".to_string(),
        key: "ImageDescription".to_string(),
        value: "test description".to_string(),
        editable: true,
    }];
    run_battery("media_meta_write::write_exif", &[0xFF, 0xD8, 0xFF, 0xE1], 4, |b| {
        let r = write_exif(b, &fields);
        if b.is_empty() {
            assert!(r.is_err(), "write_exif(empty, _) must return Err (not a valid JPEG)");
        }
    });
}

#[test]
fn write_ogg_never_panics() {
    // `write_ogg` takes (orig: &[u8], fields: &[MetaField]) and rejects non-Ogg or malformed input
    // gracefully (via `Err`, never a panic). Feed a small fixed field set and fuzz the OGG bytes.
    let fields = vec![MetaField {
        group: "vorbis".to_string(),
        key: "Title".to_string(),
        value: "test title".to_string(),
        editable: true,
    }];
    run_battery("media_meta_write::write_ogg", &[0x4F, 0x67, 0x67, 0x53], 27, |b| {
        let r = write_ogg(b, &fields);
        if b.is_empty() {
            assert!(r.is_err(), "write_ogg(empty, _) must return Err (not a valid OggS stream)");
        }
    });
}

#[test]
fn write_mp4_never_panics() {
    // `write_mp4` takes (orig: &[u8], fields: &[MetaField]) and rejects non-BMFF/fragmented/open-ended or
    // truncated input gracefully (via `Err`, never a panic). Feed a small fixed field set and fuzz the
    // ISO-BMFF bytes (seeded with a plausible `moov` box magic so the battery reaches the walk, not just
    // the header-length reject). CPE-1309.
    let fields = vec![MetaField {
        group: "video".to_string(),
        key: "Title".to_string(),
        value: "test title".to_string(),
        editable: true,
    }];
    run_battery("video_meta_write::write_mp4", &[0x00, 0x00, 0x00, 0x10, b'm', b'o', b'o', b'v'], 8, |b| {
        let r = write_mp4(b, &fields);
        if b.is_empty() {
            assert!(r.is_err(), "write_mp4(empty, _) must return Err (no moov box)");
        }
    });
}

#[test]
fn write_vorbis_comment_never_panics() {
    // `write_vorbis_comment` takes only &[MetaField] (no orig bytes), so fuzz the field values
    // themselves rather than raw bytes: long strings, empty strings, non-ASCII unicode. It always
    // returns Vec<u8> (never panics, never errors). Test multiple field configurations.
    let test_cases = vec![
        ("empty_fields", vec![]),
        ("simple_title", vec![MetaField {
            group: "vorbis".to_string(),
            key: "Title".to_string(),
            value: "simple".to_string(),
            editable: true,
        }]),
        ("empty_value", vec![MetaField {
            group: "vorbis".to_string(),
            key: "Comment".to_string(),
            value: "".to_string(),
            editable: true,
        }]),
        ("long_artist", vec![MetaField {
            group: "vorbis".to_string(),
            key: "Artist".to_string(),
            value: "a".repeat(1000),
            editable: true,
        }]),
        ("unicode_album", vec![MetaField {
            group: "vorbis".to_string(),
            key: "Album".to_string(),
            value: "unicode: μ π ω".to_string(),
            editable: true,
        }]),
    ];
    for (class_name, fields) in test_cases {
        assert_no_panic("media_meta_write::write_vorbis_comment", class_name, || {
            let _r = write_vorbis_comment(&fields);
            // Function always succeeds; just assert it didn't panic
        });
    }
}

// ---------------------------------------------------------------------------------------------
// Entrypoints: the dispatcher layer (media_meta / column_extract) — same codecs, reached the way the
// app actually calls them (by extension), so a dispatch-layer regression is covered too.
// ---------------------------------------------------------------------------------------------

#[test]
fn media_meta_read_all_never_panics() {
    // One battery per extension `read_all` special-cases its own codec for (CPE-1291 added "wav"/"xmp" to
    // this list), so a dispatch-layer regression on any of them is caught here too — not just via each
    // codec's own direct entrypoint test above. Magic/header_len per extension mirror the same
    // non-hollow-header reasoning used above (maxed ID3 syncsafe size; full RIFF/WAVE header; no fixed
    // magic for the xmp-sidecar path).
    let cases: [(&str, &[u8], usize); 3] =
        [("mp3", &b"ID3\x03\x00\x00\x7F\x7F\x7F\x7F"[..], 10), ("wav", &b"RIFF\x00\x00\x00\x00WAVE"[..], 12), ("xmp", &[], 9)];
    for (ext, magic, header_len) in cases {
        run_battery(&format!("media_meta::read_all({ext})"), magic, header_len, |b| {
            let r = read_all(ext, b);
            if b.is_empty() {
                assert!(r.is_empty(), "read_all(\"{ext}\", empty) must be empty");
            }
        });
    }
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
    // CPE-1291 added "wav" to AUDIO_EXTS / read_audio_tags — battery it alongside mp3.
    let cases: [(&str, &[u8], usize); 2] =
        [("mp3", &b"ID3"[..], 10), ("wav", &b"RIFF\x00\x00\x00\x00WAVE"[..], 12)];
    for (ext, magic, header_len) in cases {
        run_battery(&format!("column_extract::read_audio_tags({ext})"), magic, header_len, |b| {
            let r = read_audio_tags(ext, b);
            if b.is_empty() {
                assert!(r.is_empty(), "read_audio_tags(\"{ext}\", empty) must be empty");
            }
        });
    }
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
