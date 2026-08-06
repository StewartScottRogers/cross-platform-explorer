//! Malformed-input panic-safety coverage for the *path-based* structured preview parsers (CPE-1311,
//! epic CPE-1002 "File inspection & safety utilities").
//!
//! `parser_panic_safety.rs` (CPE-1169) already proved this contract — malformed input yields a graceful
//! `Err`/`None`/empty result, never a panic — for every `&[u8]`-taking parser entrypoint. Ten entrypoints
//! are out of scope there because they take a `path: &str` and read the file themselves:
//! `binary_preview::{pe_info, midi_info, wasm_info, torrent_info}` (goblin / midly / wasmprinter /
//! serde_bencode), `data_preview::{spreadsheet_info, sqlite_info}` (calamine / rusqlite),
//! `rar::rar_entries` and `camera_raw::read_raw_preview_data_url` (both hand-rolled binary-format
//! walkers, CPE-1354), and — behind the `dicom-thumb` feature — `dicom::{read_dicom_tags,
//! read_dicom_image_data_url}` (`dicom-object`/`dicom-pixeldata`, also CPE-1354). `goblin` and `midly` in
//! particular are historically panic-prone on crafted/adversarial input (out-of-bounds slicing on bogus
//! section/track counts, integer overflow on declared lengths); `rar` and `dicom` are hand-rolled or
//! third-party binary-format walkers over untrusted bytes, exactly the same risk class. All of it is the
//! kind of bug this harness exists to catch before a single malformed file can take down the whole
//! preview pane.
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
use cpe_server::camera_raw::read_raw_preview_data_url;
use cpe_server::data_preview::{spreadsheet_info, sqlite_info};
use cpe_server::rar::{rar_entries, rar_extract_entry};

#[cfg(feature = "dicom-thumb")]
use cpe_server::dicom::{read_dicom_image_data_url, read_dicom_tags};
#[cfg(feature = "dicom-thumb")]
use dicom_core::{DataElement, PrimitiveValue, VR, dicom_value};
#[cfg(feature = "dicom-thumb")]
use dicom_dictionary_std::{tags, uids};
#[cfg(feature = "dicom-thumb")]
use dicom_object::{FileMetaTableBuilder, InMemDicomObject};

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

// ---------------------------------------------------------------------------------------------
// rar.rs — hand-rolled RAR4/RAR5 header walker (CPE-1354)
// ---------------------------------------------------------------------------------------------

/// Write one RAR5 vint (little-endian base-128, continuation bit in the high bit of each byte) — a
/// small, self-contained duplicate of `rar.rs`'s own (private) `write_vint` test helper, kept here only
/// to build a realistic adversarial-battery fixture (not a new dependency, not exported from the crate).
fn rar5_write_vint(buf: &mut Vec<u8>, mut v: u64) {
    loop {
        let mut byte = (v & 0x7F) as u8;
        v >>= 7;
        if v != 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if v == 0 {
            break;
        }
    }
}

/// One RAR5 header envelope: `HeaderCRC32` (4 raw bytes, unchecked) + `HeaderSize` vint + body.
fn rar5_header(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&0u32.to_le_bytes());
    rar5_write_vint(&mut out, body.len() as u64);
    out.extend_from_slice(body);
    out
}

/// A minimal-but-real RAR5 file-header body: `HeaderType`=File(2), `HeaderFlags`=0 (no extra/data
/// area), `FileFlags`=0, `UnpackedSize`, `Attributes`=0, `CompressionInfo`=0, `HostOS`=0, then
/// `NameLength` + the name bytes — walks `parse_rar5` past the common envelope into its real
/// vint-by-vint file-header field reads (the historically panic-prone part on crafted length/flag
/// fields) instead of bailing at the 8-byte marker check.
fn rar5_file_header_body(name: &str, size: u64) -> Vec<u8> {
    let mut body = Vec::new();
    rar5_write_vint(&mut body, 2); // HeaderType = File
    rar5_write_vint(&mut body, 0); // HeaderFlags
    rar5_write_vint(&mut body, 0); // FileFlags
    rar5_write_vint(&mut body, size); // UnpackedSize
    rar5_write_vint(&mut body, 0); // Attributes
    rar5_write_vint(&mut body, 0); // CompressionInfo
    rar5_write_vint(&mut body, 0); // HostOS
    rar5_write_vint(&mut body, name.len() as u64); // NameLength
    body.extend_from_slice(name.as_bytes());
    body
}

#[test]
fn rar_entries_never_panics() {
    // RAR5 signature (`Rar!\x1a\x07\x01\x00`) followed by one real file-header block naming
    // "hello.txt" — realistic enough to walk `parse_rar5`'s actual field reads rather than just
    // exercising the marker-detection gate.
    const RAR5_MARKER: [u8; 8] = [0x52, 0x61, 0x72, 0x21, 0x1A, 0x07, 0x01, 0x00];
    let mut magic = RAR5_MARKER.to_vec();
    magic.extend(rar5_header(&rar5_file_header_body("hello.txt", 8)));

    let header_len = magic.len();
    run_battery("rar::rar_entries", &magic, header_len, |b| {
        let f = write_temp(b, ".rar");
        let path = f.path().to_str().expect("temp path must be valid UTF-8");
        let r = rar_entries(path);
        if b.is_empty() {
            assert!(r.is_err(), "rar_entries(empty file) must be Err, not a panic");
        }
    });
}

#[test]
fn rar_extract_entry_never_panics() {
    // Extraction (CPE-1360) is the second hand-rolled RAR path: it re-walks the header block AND then
    // slices the data area (`data[data_start..next_pos]`) for a STORED entry — untrusted offsets/sizes, so
    // a malformed/truncated header must Err (or return bytes), never panic. Same realistic RAR5 magic as
    // `rar_entries_never_panics` (a file header naming "hello.txt"), plus a few STORED data bytes after it
    // so an un-mutated case actually reaches the data-copy path rather than bailing at the header.
    const RAR5_MARKER: [u8; 8] = [0x52, 0x61, 0x72, 0x21, 0x1A, 0x07, 0x01, 0x00];
    let mut magic = RAR5_MARKER.to_vec();
    magic.extend(rar5_header(&rar5_file_header_body("hello.txt", 8)));
    magic.extend_from_slice(b"ABCDEFGH"); // 8 STORED data bytes for the extractor to copy

    let header_len = magic.len();
    run_battery("rar::rar_extract_entry", &magic, header_len, |b| {
        let f = write_temp(b, ".rar");
        let path = f.path().to_str().expect("temp path must be valid UTF-8");
        // Ask for the entry the magic names, plus a name that won't match — both must be panic-free on
        // every mutation (the miss exercises the walk-to-end-without-finding path).
        let r = rar_extract_entry(path, "hello.txt");
        let _ = rar_extract_entry(path, "nope.bin");
        if b.is_empty() {
            assert!(r.is_err(), "rar_extract_entry(empty file) must be Err, not a panic");
        }
    });
}

// ---------------------------------------------------------------------------------------------
// camera_raw.rs — hand-rolled TIFF/IFD walker for embedded-JPEG raw previews (CPE-1354)
// ---------------------------------------------------------------------------------------------

#[test]
fn read_raw_preview_data_url_never_panics() {
    // A minimal-but-real little-endian TIFF: header(8) -> IFD0 (one JPEGInterchangeFormat/Length
    // pointer pair) -> a JPEG-SOI-shaped stub — realistic enough that the walk reaches the real
    // IFD-entry / embedded-JPEG-candidate extraction logic instead of bailing at the II/MM + magic-42
    // gate (mirrors `binary_preview::wasm_info`'s "use the whole minimal-valid file as magic" approach).
    const TAG_JPEG_OFFSET: u16 = 0x0201;
    const TAG_JPEG_LENGTH: u16 = 0x0202;
    const TYPE_LONG: u16 = 4;

    fn tiff_entry(tag: u16, typ: u16, count: u32, value: u32) -> Vec<u8> {
        let mut e = Vec::with_capacity(12);
        e.extend_from_slice(&tag.to_le_bytes());
        e.extend_from_slice(&typ.to_le_bytes());
        e.extend_from_slice(&count.to_le_bytes());
        e.extend_from_slice(&value.to_le_bytes());
        e
    }

    let jpeg: Vec<u8> = vec![0xFF, 0xD8, 0x00, 0x00, 0xFF, 0xD9]; // SOI ... EOI stub
    let ifd0_offset: u32 = 8;
    // header(8) -> IFD0 (2-byte entry count + 2 entries*12 bytes + 4-byte NextIFD) -> jpeg bytes.
    let jpeg_offset: u32 = ifd0_offset + 2 + 24 + 4;

    let mut ifd0 = Vec::new();
    ifd0.extend_from_slice(&2u16.to_le_bytes()); // entry count
    ifd0.extend_from_slice(&tiff_entry(TAG_JPEG_OFFSET, TYPE_LONG, 1, jpeg_offset));
    ifd0.extend_from_slice(&tiff_entry(TAG_JPEG_LENGTH, TYPE_LONG, 1, jpeg.len() as u32));
    ifd0.extend_from_slice(&0u32.to_le_bytes()); // NextIFD = 0

    let mut magic = Vec::new();
    magic.extend_from_slice(b"II");
    magic.extend_from_slice(&42u16.to_le_bytes());
    magic.extend_from_slice(&ifd0_offset.to_le_bytes());
    magic.extend_from_slice(&ifd0);
    magic.extend_from_slice(&jpeg);

    let header_len = magic.len();
    run_battery("camera_raw::read_raw_preview_data_url", &magic, header_len, |b| {
        let f = write_temp(b, ".cr2");
        let path = f.path().to_str().expect("temp path must be valid UTF-8");
        let r = read_raw_preview_data_url(path);
        if b.is_empty() {
            assert!(r.is_err(), "read_raw_preview_data_url(empty file) must be Err, not a panic");
        }
    });
}

// ---------------------------------------------------------------------------------------------
// dicom.rs — dicom-object/dicom-pixeldata backed tag + pixel-data reader (CPE-1354). Gated behind the
// `dicom-thumb` feature (off by default, mirroring the module itself); these cases only compile/run
// with it enabled, e.g. `cargo test --features dicom-thumb`.
// ---------------------------------------------------------------------------------------------

/// A minimal-but-real, uncompressed (Explicit VR Little Endian) DICOM Part-10 file — 128-byte preamble,
/// `DICM` magic, file-meta group, and a tiny 2x2 8-bit MONOCHROME2 pixel array — built via
/// `dicom-object`'s own round-trip API (mirrors `dicom.rs`'s own `write_minimal_dicom` unit-test
/// helper) and read back as bytes, so the battery's "magic" is realistic enough to walk both
/// `read_dicom_tags` and `read_dicom_image_data_url` into their real file-meta/data-set/pixel-data
/// parsing rather than bailing at the `DICM` gate.
#[cfg(feature = "dicom-thumb")]
fn build_minimal_dicom_bytes() -> Vec<u8> {
    let mut obj = InMemDicomObject::new_empty();
    obj.put(DataElement::new(tags::PATIENT_NAME, VR::PN, dicom_value!(Strs, ["Doe^Jane"])));
    obj.put(DataElement::new(tags::ROWS, VR::US, dicom_value!(U16, [2])));
    obj.put(DataElement::new(tags::COLUMNS, VR::US, dicom_value!(U16, [2])));
    obj.put(DataElement::new(tags::SAMPLES_PER_PIXEL, VR::US, dicom_value!(U16, [1])));
    obj.put(DataElement::new(tags::PHOTOMETRIC_INTERPRETATION, VR::CS, dicom_value!(Strs, ["MONOCHROME2"])));
    obj.put(DataElement::new(tags::BITS_ALLOCATED, VR::US, dicom_value!(U16, [8])));
    obj.put(DataElement::new(tags::BITS_STORED, VR::US, dicom_value!(U16, [8])));
    obj.put(DataElement::new(tags::HIGH_BIT, VR::US, dicom_value!(U16, [7])));
    obj.put(DataElement::new(tags::PIXEL_REPRESENTATION, VR::US, dicom_value!(U16, [0])));
    let pixels: Vec<u8> = vec![10, 20, 30, 40];
    obj.put(DataElement::new(tags::PIXEL_DATA, VR::OB, PrimitiveValue::U8(pixels.into())));

    let file_obj = obj
        .with_meta(
            FileMetaTableBuilder::new()
                .transfer_syntax(uids::EXPLICIT_VR_LITTLE_ENDIAN)
                .media_storage_sop_class_uid("1.2.840.10008.5.1.4.1.1.7"),
        )
        .expect("building the file-meta table must succeed");

    let tmp = tempfile::NamedTempFile::new().expect("failed to create temp file for DICOM fixture");
    file_obj.write_to_file(tmp.path()).expect("failed to write minimal DICOM fixture");
    std::fs::read(tmp.path()).expect("failed to read back minimal DICOM fixture")
}

#[cfg(feature = "dicom-thumb")]
#[test]
fn read_dicom_tags_never_panics() {
    let magic = build_minimal_dicom_bytes();
    let header_len = magic.len();
    run_battery("dicom::read_dicom_tags", &magic, header_len, |b| {
        let f = write_temp(b, ".dcm");
        let path = f.path().to_str().expect("temp path must be valid UTF-8");
        let r = read_dicom_tags(path);
        if b.is_empty() {
            assert!(r.is_err(), "read_dicom_tags(empty file) must be Err, not a panic");
        }
    });
}

#[cfg(feature = "dicom-thumb")]
#[test]
fn read_dicom_image_data_url_never_panics() {
    let magic = build_minimal_dicom_bytes();
    let header_len = magic.len();
    run_battery("dicom::read_dicom_image_data_url", &magic, header_len, |b| {
        let f = write_temp(b, ".dcm");
        let path = f.path().to_str().expect("temp path must be valid UTF-8");
        let r = read_dicom_image_data_url(path);
        if b.is_empty() {
            assert!(r.is_err(), "read_dicom_image_data_url(empty file) must be Err, not a panic");
        }
    });
}
