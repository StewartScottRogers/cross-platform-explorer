//! Malformed-input panic-safety coverage for the *path-based* structured preview parsers (CPE-1311,
//! epic CPE-1002 "File inspection & safety utilities").
//!
//! `parser_panic_safety.rs` (CPE-1169) already proved this contract — malformed input yields a graceful
//! `Err`/`None`/empty result, never a panic — for every `&[u8]`-taking parser entrypoint. Six entrypoints
//! were out of scope there because they take a `path: &str` and read the file themselves:
//! `binary_preview::{pe_info, midi_info, wasm_info, torrent_info}` (goblin / midly / wasmprinter /
//! serde_bencode) and `data_preview::{spreadsheet_info, sqlite_info}` (calamine / rusqlite). `goblin` and
//! `midly` in particular are historically panic-prone on crafted/adversarial input (out-of-bounds slicing
//! on bogus section/track counts, integer overflow on declared lengths), which is exactly the class of
//! bug this harness exists to catch before a single malformed file can take down the whole preview pane.
//!
//! This file reuses the *exact same* adversarial battery + `catch_unwind` harness from
//! `tests/common/mod.rs` (shared with `parser_panic_safety.rs`, not duplicated) and adds one seam: since
//! these entrypoints take a path rather than bytes, each battery case is written to a uniquely-named
//! temp file (via the `tempfile` dev-dependency already in `Cargo.toml`, so this needs no new
//! dependency) and the path is handed to the parser. `tempfile::NamedTempFile` deletes its file on drop,
//! including during an unwind, so nothing is left behind even when a case does panic.
//!
//! Each entrypoint's battery uses a *realistic* magic/header prefix (a minimal-but-structurally-real
//! file of that format) rather than just a magic byte or two, for the same reason
//! `parser_panic_safety.rs` does for `read_id3v2`/`read_wav`/`read_pdf`: a too-short prefix would make
//! every `magic_then_*`/`overflowing_length_field_*` case bail out at the same early gate check and never
//! actually reach the interesting parsing code (a "hollow" battery). Building a fully-valid file isn't
//! required, and for `pe_info` in particular isn't attempted beyond the DOS+COFF header — the point is to
//! walk far enough into each parser's real logic that a crafted tail can reach it, not to round-trip a
//! perfectly well-formed file.

mod common;
use common::run_battery;

use std::io::Write;

use cpe_server::binary_preview::{midi_info, pe_info, torrent_info, wasm_info};
use cpe_server::data_preview::{spreadsheet_info, sqlite_info};

/// Write `bytes` to a fresh uniquely-named temp file with the given extension (so
/// `calamine::open_workbook_auto`, which dispatches by extension, picks the right reader) and return the
/// handle — keep it alive for the duration of the call under test; it deletes the file on drop, even
/// during an unwind, so nothing accumulates across the hundreds of battery cases run here.
fn write_temp(bytes: &[u8], suffix: &str) -> tempfile::NamedTempFile {
    let mut f = tempfile::Builder::new()
        .suffix(suffix)
        .tempfile()
        .expect("failed to create temp file for panic-safety battery");
    f.write_all(bytes).expect("failed to write battery bytes to temp file");
    f.flush().expect("failed to flush temp file");
    f
}

// ---------------------------------------------------------------------------------------------
// binary_preview.rs — goblin (PE), midly (MIDI), wasmprinter (wasm), serde_bencode (torrent)
// ---------------------------------------------------------------------------------------------

#[test]
fn pe_info_never_panics() {
    // A minimal DOS header ("MZ" + e_lfanew pointing right after it) + a COFF header declaring 0
    // sections and a 0-byte optional header — enough to walk goblin past the DOS-stub gate and into the
    // COFF-header field reads, which is the interesting part of the parser (its own doc calls out
    // section/import table iteration as the historically panic-prone spot on crafted section counts).
    let mut magic = vec![0u8; 64];
    magic[0] = b'M';
    magic[1] = b'Z';
    magic[0x3C..0x40].copy_from_slice(&64u32.to_le_bytes()); // e_lfanew -> byte 64, right after the stub
    magic.extend_from_slice(b"PE\0\0");
    magic.extend_from_slice(&0x014Cu16.to_le_bytes()); // Machine = IMAGE_FILE_MACHINE_I386
    magic.extend_from_slice(&0u16.to_le_bytes()); // NumberOfSections
    magic.extend_from_slice(&0u32.to_le_bytes()); // TimeDateStamp
    magic.extend_from_slice(&0u32.to_le_bytes()); // PointerToSymbolTable
    magic.extend_from_slice(&0u32.to_le_bytes()); // NumberOfSymbols
    magic.extend_from_slice(&0u16.to_le_bytes()); // SizeOfOptionalHeader
    magic.extend_from_slice(&0x0102u16.to_le_bytes()); // Characteristics

    let header_len = magic.len();
    run_battery("binary_preview::pe_info", &magic, header_len, |b| {
        let f = write_temp(b, ".exe");
        let path = f.path().to_str().expect("temp path must be valid UTF-8");
        let r = pe_info(path);
        if b.is_empty() {
            assert!(r.is_err(), "pe_info(empty file) must be Err, not a panic");
        }
    });
}

#[test]
fn midi_info_never_panics() {
    // A minimal-but-complete standard MIDI file: an "MThd" header chunk (format 0, 1 track, division 96)
    // followed by one "MTrk" chunk holding a single End-of-Track meta event — real enough that midly
    // walks the track-event loop (the historically panic-prone part on crafted event/length bytes)
    // instead of bailing at the header gate.
    let mut magic = Vec::new();
    magic.extend_from_slice(b"MThd");
    magic.extend_from_slice(&6u32.to_be_bytes()); // header chunk length, always 6
    magic.extend_from_slice(&0u16.to_be_bytes()); // format 0
    magic.extend_from_slice(&1u16.to_be_bytes()); // ntrks = 1
    magic.extend_from_slice(&96u16.to_be_bytes()); // division
    magic.extend_from_slice(b"MTrk");
    let track_events: [u8; 4] = [0x00, 0xFF, 0x2F, 0x00]; // delta 0, End-of-Track meta event
    magic.extend_from_slice(&(track_events.len() as u32).to_be_bytes());
    magic.extend_from_slice(&track_events);

    let header_len = magic.len();
    run_battery("binary_preview::midi_info", &magic, header_len, |b| {
        let f = write_temp(b, ".mid");
        let path = f.path().to_str().expect("temp path must be valid UTF-8");
        let r = midi_info(path);
        if b.is_empty() {
            assert!(r.is_err(), "midi_info(empty file) must be Err, not a panic");
        }
    });
}

#[test]
fn wasm_info_never_panics() {
    // The 8-byte minimal-valid WebAssembly module: magic "\0asm" + version 1, no sections.
    let magic: [u8; 8] = [0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
    run_battery("binary_preview::wasm_info", &magic, magic.len(), |b| {
        let f = write_temp(b, ".wasm");
        let path = f.path().to_str().expect("temp path must be valid UTF-8");
        let r = wasm_info(path, 64 * 1024);
        if b.is_empty() {
            assert!(r.is_err(), "wasm_info(empty file) must be Err, not a panic");
        }
    });
}

#[test]
fn torrent_info_never_panics() {
    // Bencode has no fixed leading magic, so use a small-but-fully-valid dict — `d4:infod4:name1:aee` =
    // `{"info": {"name": "a"}}` — as the battery's "magic": it walks torrent_info's real dict/nested-dict
    // extraction instead of the empty-magic battery just producing runs of zero bytes with no structure
    // at all.
    let magic = b"d4:infod4:name1:aee".to_vec();
    let header_len = magic.len();
    run_battery("binary_preview::torrent_info", &magic, header_len, |b| {
        let f = write_temp(b, ".torrent");
        let path = f.path().to_str().expect("temp path must be valid UTF-8");
        let r = torrent_info(path);
        if b.is_empty() {
            assert!(r.is_err(), "torrent_info(empty file) must be Err, not a panic");
        }
    });
}

// ---------------------------------------------------------------------------------------------
// data_preview.rs — calamine (spreadsheet), rusqlite (SQLite)
// ---------------------------------------------------------------------------------------------

#[test]
fn spreadsheet_info_never_panics() {
    // `calamine::open_workbook_auto` dispatches on the file extension (`.xlsx` here, via `write_temp`),
    // then an xlsx is itself a zip archive, so the local-file-header signature is the realistic magic to
    // battery against — it walks calamine into the zip-central-directory read rather than bailing
    // instantly on an unrecognized container.
    let magic: [u8; 4] = [0x50, 0x4B, 0x03, 0x04]; // "PK\x03\x04"
    run_battery("data_preview::spreadsheet_info", &magic, magic.len(), |b| {
        let f = write_temp(b, ".xlsx");
        let path = f.path().to_str().expect("temp path must be valid UTF-8");
        let r = spreadsheet_info(path);
        if b.is_empty() {
            assert!(r.is_err(), "spreadsheet_info(empty file) must be Err, not a panic");
        }
    });
}

#[test]
fn sqlite_info_never_panics() {
    // The 16-byte SQLite file-format magic; `header_len` mirrors the real 100-byte database header even
    // though the battery doesn't fill in every field, matching how the shared battery aims truncation at
    // a format's real header boundary elsewhere (see `read_wav`/`read_id3v2` in `parser_panic_safety.rs`).
    let magic = b"SQLite format 3\0".to_vec();
    run_battery("data_preview::sqlite_info", &magic, 100, |b| {
        let f = write_temp(b, ".db");
        let path = f.path().to_str().expect("temp path must be valid UTF-8");
        let r = sqlite_info(path);
        if b.is_empty() {
            // A zero-byte file is a well-formed *empty* SQLite database by the format's own convention
            // (not a malformed one), so `sqlite_info` legitimately succeeds here — assert the graceful
            // "no tables" result rather than an error.
            assert!(r.is_ok(), "sqlite_info(empty file) must gracefully report an empty database: {r:?}");
            assert!(r.unwrap().contains("0 table"), "empty database must report 0 tables/views");
        }
    });
}
