//! Adversarial panic-safety battery for the archive readers/extractors in `archive.rs` (CPE-1411, epic
//! CPE-1002 "File inspection & safety utilities").
//!
//! `parser_panic_safety.rs` (CPE-1169) and `binary_data_preview_panic_safety.rs` (CPE-1311) already
//! prove this contract for every OTHER structured-preview parser; `archive.rs`'s readers/extractors were
//! the one big gap — they parse OPENED ARCHIVE FILE CONTENTS (fully untrusted: a user can double-click
//! any file and get it previewed) with only a single happy-path zip fixture testing them, despite
//! shelling out to two comparatively young pure-Rust crates (`sevenz-rust` 0.6, `iso9660` 0.1) neither of
//! which is fuzz-hardened upstream. This file reuses the exact same adversarial battery + `catch_unwind`
//! harness from `tests/common/mod.rs` (`run_battery`/`assert_no_panic`) plus dedicated hand-crafted cases
//! for the "huge/negative/overflowing length & offset field" and "many/deeply-nested entries" classes the
//! generic magic+tail battery can't target precisely (those need control over specific header fields, not
//! just a fuzzed tail after a realistic prefix).
//!
//! Covers, for each of zip / tar (+ .tar.gz/.tgz) / gzip / 7z / ISO — both **listing**
//! (`read_archive_entries`, plus `zip_entries` directly) and, where the format supports it, **extraction**
//! (`extract_archive_entry`, `extract_archive_entry_any`, `extract_archive`). RAR is deliberately out of
//! scope (already covered by `binary_data_preview_panic_safety.rs`'s `rar_entries_never_panics`/
//! `rar_extract_entry_never_panics`, CPE-1354).
//!
//! Two real (upstream) bugs were found while building this battery, both documented in detail at their
//! test below:
//!
//! 1. **`archive.rs` bug, FIXED here** — `iso_entries`'s inner loop did `Err(_) => continue` on an
//!    unreadable directory record, but the `iso9660` crate's directory iterator does not advance its
//!    cursor on a parse error, so `continue` re-read the exact same failing bytes forever: an ISO with one
//!    unparseable root-directory record hung the whole listing thread. Fixed by `break`ing out of that
//!    directory's entry loop instead (still visits other queued directories) — see
//!    `iso_listing_does_not_hang_on_an_unparseable_directory_record` below.
//! 2. **Upstream `sevenz-rust` 0.6.1 bug, not fixable at the parser level — reported, not patched there.**
//!    `archive.rs` DOES now defensively contain it (CPE-1415): every `sevenz-rust` call site is wrapped in
//!    `catch_unwind` (`catch_sevenz_panic`), so a crafted `.7z` returns a clean `Err` instead of panicking
//!    through `archive.rs`'s public functions (this was already contained one level further out too — every
//!    call site runs inside a Tokio `spawn_blocking` task, whose boundary catches an uncaught panic). See
//!    `sevenz_rust_upstream_signature_header_offset_overflow_still_panics` (calls `sevenz_rust::Archive::read`
//!    directly, bypassing the wrapper, to keep pinning the raw upstream bug) and
//!    `sevenz_signature_header_offset_overflow_is_caught_and_returns_err` (proves the wrapped public path)
//!    below.

mod common;
use common::{assert_no_panic, run_battery};

use std::io::Write;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use cpe_server::archive::{
    extract_archive, extract_archive_entry, extract_archive_entry_any, read_archive_entries, zip_entries,
};

// ---------------------------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------------------------

/// Write `bytes` to a fresh uniquely-named temp file with the given extension (`read_archive_entries` /
/// `extract_archive*` dispatch by lowercase extension) — mirrors `binary_data_preview_panic_safety.rs`'s
/// `write_temp` helper (not shared across files, same reasoning as that file's own duplicated fixture
/// helpers: each integration-test file is its own crate).
fn write_temp(bytes: &[u8], suffix: &str) -> tempfile::NamedTempFile {
    let mut f = tempfile::Builder::new()
        .suffix(suffix)
        .tempfile()
        .expect("failed to create temp file for battery");
    f.write_all(bytes).expect("failed to write battery bytes to temp file");
    f.flush().expect("failed to flush temp file");
    f
}

/// A fresh, empty destination directory for one extraction case — `extract_archive` writes into it;
/// giving every case its own directory avoids cross-case interference/collisions.
fn fresh_dest_dir() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("cpe-archive-panic-dest-")
        .tempdir()
        .expect("failed to create a fresh destination dir")
}

/// CRC-32 (IEEE 802.3 / zlib polynomial) — used by both the hand-crafted zip fixture (matches the zip
/// format's own checksum) and the hand-crafted 7z signature header (`sevenz-rust` uses the same standard
/// CRC-32 for `StartHeaderCRC`). Duplicated from `archive.rs`'s own private `crc32` test helper since
/// integration tests can't reach a unit-test-only function.
fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

/// Run `f` on a freshly-spawned thread with an artificially small stack (256 KiB, per CPE-1411's
/// instruction) and wait up to `timeout` for it to finish, via a channel rather than `join()`.
///
/// This exists for two distinct hazards `catch_unwind` alone can't cover:
/// - **Deep recursion / stack overflow**: unlike a normal panic, a stack overflow is NOT catchable — Rust
///   turns it into an immediate process abort. A small stack makes a real overflow bug manifest at a much
///   shallower (cheaper, faster, more deterministic) recursion depth than the default ~8 MiB stack would,
///   and if it happens, the abort shows up as this whole test **binary** crashing — a loud, unambiguous
///   "process failure" signal rather than a clean assertion failure.
/// - **A genuine infinite loop** (no recursion, no growing stack — just a cursor that never advances, the
///   exact shape of the ISO bug found below): `catch_unwind` never returns because the closure never
///   returns. Polling via `recv_timeout` instead of `join()` means a hang is reported as a normal, bounded
///   test failure — not a stuck test process. The stuck probe thread is deliberately never joined/killed
///   (Rust has no safe way to kill a thread); it is simply abandoned. That's fine: the OS reclaims it when
///   the test binary's process exits, since process exit tears down every thread regardless of whether it
///   was joined.
fn run_with_timeout<F, R>(timeout: Duration, f: F) -> Option<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(move || {
            let _ = tx.send(f());
        })
        .expect("failed to spawn probe thread");
    rx.recv_timeout(timeout).ok()
}

// ---------------------------------------------------------------------------------------------
// ZIP — listing (`zip_entries`/`read_archive_entries`) + extraction (`extract_archive_entry`,
// `extract_archive_entry_any`, `extract_archive`)
// ---------------------------------------------------------------------------------------------

fn build_valid_zip() -> Vec<u8> {
    let f = tempfile::Builder::new().suffix(".zip").tempfile().unwrap();
    {
        let file = std::fs::File::create(f.path()).unwrap();
        let mut w = zip::ZipWriter::new(file);
        let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default();
        w.start_file("a.txt", opts).unwrap();
        w.write_all(b"hello").unwrap();
        w.add_directory("sub/", opts).unwrap();
        w.finish().unwrap();
    }
    std::fs::read(f.path()).unwrap()
}

#[test]
fn zip_listing_never_panics() {
    let magic = build_valid_zip();
    let header_len = magic.len();
    run_battery("archive::read_archive_entries (zip)", &magic, header_len, |b| {
        let f = write_temp(b, ".zip");
        let path = f.path().to_str().unwrap();
        let r = read_archive_entries(path);
        if b.is_empty() {
            assert!(r.is_err(), "read_archive_entries(empty .zip) must be Err, not a panic");
        }
    });
    run_battery("archive::zip_entries", &magic, header_len, |b| {
        let f = write_temp(b, ".zip");
        let _ = zip_entries(f.path().to_str().unwrap());
    });
}

#[test]
fn zip_extraction_never_panics() {
    let magic = build_valid_zip();
    let header_len = magic.len();
    run_battery("archive::extract_archive_entry (zip)", &magic, header_len, |b| {
        let f = write_temp(b, ".zip");
        let path = f.path().to_str().unwrap();
        let _ = extract_archive_entry(path, "a.txt");
        let _ = extract_archive_entry(path, "does-not-exist.txt");
    });
    run_battery("archive::extract_archive_entry_any (zip)", &magic, header_len, |b| {
        let f = write_temp(b, ".zip");
        let _ = extract_archive_entry_any(f.path().to_str().unwrap(), "a.txt");
    });
    run_battery("archive::extract_archive (zip)", &magic, header_len, |b| {
        let f = write_temp(b, ".zip");
        let dest = fresh_dest_dir();
        let _ = extract_archive(f.path().to_str().unwrap(), dest.path().to_str().unwrap());
    });
}

/// A minimal single-entry STORED zip whose LOCAL + CENTRAL directory declared uncompressed/compressed
/// sizes are `declared_size` while the actual bytes present are just `data` (much shorter) — smuggled past
/// the zip *writer*'s own validation by building the raw structure directly, exactly like `archive.rs`'s
/// own `craft_zip_with_entry_name` unit-test helper (duplicated in spirit, not code, for the same reason).
fn craft_zip_with_declared_size(name: &str, data: &[u8], declared_size: u32) -> Vec<u8> {
    let name = name.as_bytes();
    let crc = crc32(data);
    let (nlen, dlen) = (name.len() as u16, data.len() as u32);
    let mut z = Vec::new();
    let u16le = |v: u16, z: &mut Vec<u8>| z.extend_from_slice(&v.to_le_bytes());
    let u32le = |v: u32, z: &mut Vec<u8>| z.extend_from_slice(&v.to_le_bytes());
    // Local file header.
    u32le(0x0403_4b50, &mut z);
    u16le(20, &mut z);
    u16le(0, &mut z);
    u16le(0, &mut z); // ver, flags, method(stored)
    u16le(0, &mut z);
    u16le(0, &mut z); // mod time/date
    u32le(crc, &mut z);
    u32le(dlen, &mut z);
    u32le(declared_size, &mut z); // crc, comp size (honest), uncomp size (LIE)
    u16le(nlen, &mut z);
    u16le(0, &mut z); // name len, extra len
    z.extend_from_slice(name);
    z.extend_from_slice(data);
    let cd_offset = z.len() as u32;
    // Central directory header.
    u32le(0x0201_4b50, &mut z);
    u16le(20, &mut z);
    u16le(20, &mut z);
    u16le(0, &mut z);
    u16le(0, &mut z); // made-by, needed, flags, method
    u16le(0, &mut z);
    u16le(0, &mut z); // mod time/date
    u32le(crc, &mut z);
    u32le(dlen, &mut z);
    u32le(declared_size, &mut z); // same lie, mirrored in the central directory
    u16le(nlen, &mut z);
    u16le(0, &mut z);
    u16le(0, &mut z); // name, extra, comment len
    u16le(0, &mut z);
    u16le(0, &mut z);
    u32le(0, &mut z); // disk start, internal attrs, external attrs
    u32le(0, &mut z); // local header offset
    z.extend_from_slice(name);
    let cd_size = z.len() as u32 - cd_offset;
    // End of central directory.
    u32le(0x0605_4b50, &mut z);
    u16le(0, &mut z);
    u16le(0, &mut z);
    u16le(1, &mut z);
    u16le(1, &mut z); // disks, entries
    u32le(cd_size, &mut z);
    u32le(cd_offset, &mut z);
    u16le(0, &mut z); // cd size/offset, comment len
    z
}

#[test]
fn zip_extraction_does_not_amplify_a_lying_declared_size() {
    // "Zip-bomb-ish", but bounded: a STORED entry declares an uncompressed size of ~1 GiB while only 8
    // real bytes are present. `extract_archive_entry`/`extract_archive` stream via `std::io::copy`
    // (fixed-size buffer, driven by what's actually there to read), so this must return promptly — an
    // error most likely, since the declared size won't match what's actually readable — rather than
    // pre-allocating anything close to 1 GiB. Deliberately NOT a multi-GB declared size: this proves the
    // "does it try to size a buffer off the untrusted field" question just as well without risking an
    // actual multi-GB allocation attempt in CI.
    let data = b"tiny!!!!";
    let bytes = craft_zip_with_declared_size("bomb.bin", data, 1_000_000_000);
    let f = write_temp(&bytes, ".zip");
    let path = f.path().to_str().unwrap().to_string();

    assert_no_panic("archive::extract_archive_entry (lying declared size)", "declared_1gb_actual_8b", || {
        let _ = extract_archive_entry(&path, "bomb.bin");
    });
    let dest = fresh_dest_dir();
    let dest_path = dest.path().to_str().unwrap().to_string();
    assert_no_panic("archive::extract_archive (lying declared size)", "declared_1gb_actual_8b", || {
        let _ = extract_archive(&path, &dest_path);
    });
}

#[test]
fn zip_listing_and_extraction_handle_many_entries_and_deep_paths() {
    // Scale/shape cases the magic+tail battery can't reach: thousands of real entries, and one entry
    // whose name is a few thousand path segments deep. Neither should panic, hang, or blow the stack —
    // zip listing/extraction here is purely iterative (no recursion in `archive.rs` or the `zip` crate's
    // own reader), so this is a scale sanity check more than a targeted attack, but it's cheap and it's
    // exactly the shape CPE-1411 asked for.
    let d = tempfile::Builder::new().prefix("cpe-archive-panic-many-").tempdir().unwrap();
    let many_zip = d.path().join("many.zip");
    {
        let file = std::fs::File::create(&many_zip).unwrap();
        let mut w = zip::ZipWriter::new(file);
        let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default();
        for i in 0..1500 {
            w.start_file(format!("f{i}.txt"), opts).unwrap();
            w.write_all(b"x").unwrap();
        }
        // One deeply-nested path: 500 "a/" segments then a leaf file.
        let deep_name = format!("{}leaf.txt", "a/".repeat(500)); // ~2x a typical 260-char MAX_PATH
        w.start_file(&deep_name, opts).unwrap();
        w.write_all(b"deep").unwrap();
        w.finish().unwrap();
    }
    let path = many_zip.to_str().unwrap();

    assert_no_panic("archive::read_archive_entries (zip, many+deep)", "1500_entries_plus_deep_path", || {
        let entries = read_archive_entries(path).expect("a validly-built zip must still list");
        assert_eq!(entries.len(), 1501, "1500 flat entries + 1 deep-path entry");
    });
    let dest = fresh_dest_dir();
    assert_no_panic("archive::extract_archive (zip, many+deep)", "1500_entries_plus_deep_path", || {
        let _ = extract_archive(path, dest.path().to_str().unwrap());
    });
}

// ---------------------------------------------------------------------------------------------
// TAR (+ .tar.gz/.tgz) — listing via `read_archive_entries` + extraction via `extract_archive_entry_any`
// and `extract_archive`
// ---------------------------------------------------------------------------------------------

fn build_valid_tar() -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut b = tar::Builder::new(&mut buf);
        let data: &[u8] = b"hello";
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_entry_type(tar::EntryType::Regular);
        header.set_cksum();
        b.append_data(&mut header, "a.txt", data).unwrap();
        b.finish().unwrap();
    }
    buf
}

fn build_valid_targz() -> Vec<u8> {
    let tar_bytes = build_valid_tar();
    let mut out = Vec::new();
    {
        let mut enc = flate2::write::GzEncoder::new(&mut out, flate2::Compression::default());
        enc.write_all(&tar_bytes).unwrap();
        enc.finish().unwrap();
    }
    out
}

#[test]
fn tar_listing_never_panics() {
    let magic = build_valid_tar();
    let header_len = magic.len();
    run_battery("archive::read_archive_entries (.tar)", &magic, header_len, |b| {
        let f = write_temp(b, ".tar");
        let r = read_archive_entries(f.path().to_str().unwrap());
        if b.is_empty() {
            // Unlike zip/7z/ISO (which all need a real trailer/header to open at all), the `tar` crate
            // treats a 0-byte stream as "no more entries" rather than an error — an empty archive is
            // gracefully `Ok(vec![])`, not `Err`. (Verified empirically; mirrors the same
            // format-specific "empty is a legitimate degenerate case, not malformed" shape
            // `binary_data_preview_panic_safety.rs`'s `sqlite_info_never_panics` documents for SQLite.)
            assert!(r.is_ok() && r.unwrap().is_empty(), "read_archive_entries(empty .tar) must gracefully report zero entries");
        }
    });
}

#[test]
fn targz_listing_never_panics() {
    let magic = build_valid_targz();
    let header_len = magic.len();
    run_battery("archive::read_archive_entries (.tar.gz)", &magic, header_len, |b| {
        let f = write_temp(b, ".tar.gz");
        let _ = read_archive_entries(f.path().to_str().unwrap());
    });
    // Same gzip-tar bytes, `.tgz` extension — a distinct dispatch branch in `read_archive_entries`.
    run_battery("archive::read_archive_entries (.tgz)", &magic, header_len, |b| {
        let f = write_temp(b, ".tgz");
        let _ = read_archive_entries(f.path().to_str().unwrap());
    });
}

#[test]
fn tar_and_targz_extraction_never_panics() {
    for (magic, ext) in [(build_valid_tar(), ".tar"), (build_valid_targz(), ".tar.gz")] {
        let header_len = magic.len();
        run_battery(&format!("archive::extract_archive_entry_any ({ext})"), &magic, header_len, |b| {
            let f = write_temp(b, ext);
            let _ = extract_archive_entry_any(f.path().to_str().unwrap(), "a.txt");
        });
        run_battery(&format!("archive::extract_archive ({ext})"), &magic, header_len, |b| {
            let f = write_temp(b, ext);
            let dest = fresh_dest_dir();
            let _ = extract_archive(f.path().to_str().unwrap(), dest.path().to_str().unwrap());
        });
    }
}

/// One raw ustar header block (512 bytes) whose 12-byte `size` field uses the GNU base-256 extension
/// (leading byte's high bit set) to claim `huge_size` — while the archive that follows carries far less
/// actual data. Standard POSIX ustar field layout; the checksum is computed for real so the header passes
/// the tar crate's own validity check and the interesting field (`size`) is what's actually adversarial.
fn craft_tar_header_with_huge_size(name: &str, huge_size: u64) -> Vec<u8> {
    let mut h = vec![0u8; 512];
    let nb = name.as_bytes();
    let n = nb.len().min(100);
    h[0..n].copy_from_slice(&nb[..n]);
    h[100..108].copy_from_slice(b"0000644\0"); // mode
    h[108..116].copy_from_slice(b"0000000\0"); // uid
    h[116..124].copy_from_slice(b"0000000\0"); // gid
    h[124] = 0x80; // GNU base-256 size marker (top bit of the 12-byte size field)
    h[128..136].copy_from_slice(&huge_size.to_be_bytes()); // magnitude, right-justified in the field
    h[136..148].copy_from_slice(b"00000000000\0"); // mtime (octal)
    for b in &mut h[148..156] {
        *b = b' '; // checksum field: spaces while summing, per the ustar spec
    }
    h[156] = b'0'; // typeflag: regular file
    h[257..263].copy_from_slice(b"ustar\0");
    h[263] = b'0';
    h[264] = b'0';
    let checksum: u32 = h.iter().map(|&b| b as u32).sum();
    let cksum = format!("{checksum:06o}\0 ");
    h[148..156].copy_from_slice(cksum.as_bytes());
    h
}

#[test]
fn tar_never_panics_on_a_lying_huge_size_field() {
    // The tar header's declared size (8 TB, GNU base-256-encoded) vastly exceeds the ~512 real data bytes
    // that follow it. `tar::Entry` is a bounded reader over the underlying stream driven by that
    // declared size, not a pre-allocation, so listing/extraction must error or short-read, never hang or
    // try to allocate 8 TB.
    let mut bytes = craft_tar_header_with_huge_size("huge.bin", 8_000_000_000_000);
    bytes.extend_from_slice(&[b'X'; 512]); // some real (but tiny relative to the claim) data
    let path_variants = [(".tar", bytes.clone()), (".tar.gz", {
        let mut out = Vec::new();
        let mut enc = flate2::write::GzEncoder::new(&mut out, flate2::Compression::default());
        enc.write_all(&bytes).unwrap();
        enc.finish().unwrap();
        out
    })];

    for (ext, data) in path_variants {
        let f = write_temp(&data, ext);
        let path = f.path().to_str().unwrap().to_string();
        assert_no_panic("archive::read_archive_entries (tar huge size)", &format!("gnu_base256_8tb{ext}"), || {
            let _ = read_archive_entries(&path);
        });
        let dest = fresh_dest_dir();
        let dest_path = dest.path().to_str().unwrap().to_string();
        assert_no_panic("archive::extract_archive (tar huge size)", &format!("gnu_base256_8tb{ext}"), || {
            let _ = extract_archive(&path, &dest_path);
        });
        assert_no_panic("archive::extract_archive_entry_any (tar huge size)", &format!("gnu_base256_8tb{ext}"), || {
            let _ = extract_archive_entry_any(&path, "huge.bin");
        });
    }
}

#[test]
fn tar_listing_and_extraction_handle_many_entries_and_a_deep_path() {
    let mut buf = Vec::new();
    {
        let mut b = tar::Builder::new(&mut buf);
        for i in 0..1000 {
            let mut header = tar::Header::new_gnu();
            header.set_size(1);
            header.set_entry_type(tar::EntryType::Regular);
            header.set_cksum();
            b.append_data(&mut header, format!("f{i}.txt"), &b"x"[..]).unwrap();
        }
        // GNU long-name extension: a path far past the 100-byte classic name field.
        let deep_name = format!("{}leaf.txt", "a/".repeat(500)); // ~2x a typical 260-char MAX_PATH
        let mut header = tar::Header::new_gnu();
        header.set_size(4);
        header.set_entry_type(tar::EntryType::Regular);
        header.set_cksum();
        b.append_data(&mut header, &deep_name, &b"deep"[..]).unwrap();
        b.finish().unwrap();
    }
    let f = write_temp(&buf, ".tar");
    let path = f.path().to_str().unwrap();

    assert_no_panic("archive::read_archive_entries (tar, many+deep)", "1000_entries_plus_deep_path", || {
        let entries = read_archive_entries(path).expect("a validly-built tar must still list");
        assert_eq!(entries.len(), 1001);
    });
    let dest = fresh_dest_dir();
    assert_no_panic("archive::extract_archive (tar, many+deep)", "1000_entries_plus_deep_path", || {
        let _ = extract_archive(path, dest.path().to_str().unwrap());
    });
}

// ---------------------------------------------------------------------------------------------
// GZIP (single-file `.gz`) — listing via `gzip_single_entry` (reads only the 4-byte ISIZE trailer, never
// decompresses) and extraction via `extract_archive` (does decompress, streamed)
// ---------------------------------------------------------------------------------------------

fn build_valid_gz(content: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = flate2::write::GzEncoder::new(&mut out, flate2::Compression::default());
    enc.write_all(content).unwrap();
    enc.finish().unwrap();
    out
}

#[test]
fn gzip_listing_never_panics() {
    let magic = build_valid_gz(b"hello world");
    let header_len = magic.len();
    run_battery("archive::read_archive_entries (.gz)", &magic, header_len, |b| {
        let f = write_temp(b, ".gz");
        let r = read_archive_entries(f.path().to_str().unwrap());
        // A `.gz` shorter than 4 bytes still yields a single entry with size 0 (documented fallback in
        // `gzip_single_entry`), so — unlike every other format — even the empty-file case is `Ok`, not
        // `Err`. Assert that documented graceful behavior rather than the usual empty-is-Err sentinel.
        if b.is_empty() {
            assert!(r.is_ok(), "read_archive_entries(empty .gz) must gracefully report a size-0 entry: {}", r.err().unwrap_or_default());
        }
    });
}

#[test]
fn gzip_extraction_never_panics() {
    let magic = build_valid_gz(b"hello world");
    let header_len = magic.len();
    run_battery("archive::extract_archive (.gz)", &magic, header_len, |b| {
        let f = write_temp(b, ".gz");
        let dest = fresh_dest_dir();
        let _ = extract_archive(f.path().to_str().unwrap(), dest.path().to_str().unwrap());
    });
}

#[test]
fn gzip_extraction_streams_a_highly_compressible_payload_without_ballooning() {
    // "Gzip-bomb-ish", kept modest on purpose: 4 MiB of zeros compresses to a few KiB. `extract_archive`'s
    // `.gz` branch is `std::io::copy(&mut GzDecoder::new(file), &mut out)` — a fixed-size-buffer streaming
    // copy straight through to a file, not a Vec sized off the trailer's declared length — so this proves
    // that streaming shape end-to-end with a real (if deliberately small) high compression ratio, rather
    // than actually trying to reproduce a multi-GB bomb in CI.
    let payload = vec![0u8; 4 * 1024 * 1024];
    let bytes = build_valid_gz(&payload);
    assert!(bytes.len() < payload.len() / 100, "fixture should compress by at least 100x to be a real bomb-ish case");
    let f = write_temp(&bytes, ".gz");
    let path = f.path().to_str().unwrap().to_string();
    let dest = fresh_dest_dir();
    let dest_path = dest.path().to_str().unwrap().to_string();
    assert_no_panic("archive::extract_archive (.gz, compressible payload)", "4mib_of_zeros", || {
        let out = extract_archive(&path, &dest_path).expect("a validly-built .gz must still extract");
        let extracted = std::fs::read(std::path::Path::new(&out).join("note")).ok().or_else(|| {
            // The extracted filename is the archive's own stem; discover it instead of assuming.
            std::fs::read_dir(&out).ok()?.next()?.ok().map(|e| std::fs::read(e.path()).unwrap())
        });
        assert_eq!(extracted, Some(payload), "decompressed content must round-trip byte-exact");
    });
}

// ---------------------------------------------------------------------------------------------
// 7-ZIP — listing (`sevenz_entries` via `read_archive_entries`) + extraction (`extract_archive_entry_any`,
// `extract_archive`). Small-stack-thread wrapped per CPE-1411 (a young crate parsing a graph of
// folders/coders — a plausible spot for hidden recursion even though nothing here confirmed one).
// ---------------------------------------------------------------------------------------------

fn build_valid_7z() -> Vec<u8> {
    let f = tempfile::Builder::new().suffix(".7z").tempfile().unwrap();
    {
        let mut w = sevenz_rust::SevenZWriter::create(f.path()).unwrap();
        let mut entry = sevenz_rust::SevenZArchiveEntry::new();
        entry.name = "a.txt".to_string();
        w.push_archive_entry(entry, Some(std::io::Cursor::new(b"hello".to_vec()))).unwrap();
        w.finish().unwrap();
    }
    std::fs::read(f.path()).unwrap()
}

/// Run `check` on `bytes` inside a 256 KiB-stack thread (see [`run_with_timeout`]'s doc comment) with a
/// generous timeout, so a hidden-recursion bug in the young `sevenz-rust`/`iso9660` crates would surface
/// as a fast, deterministic process abort (stack overflow) rather than needing to actually exhaust an
/// 8 MiB default stack.
fn run_battery_small_stack(name: &str, magic: &[u8], header_len: usize, check: impl Fn(&[u8]) + Send + Sync + 'static) {
    let check = std::sync::Arc::new(check);
    for c in common::battery(magic, header_len) {
        let check = check.clone();
        let bytes = c.bytes.clone();
        let result = run_with_timeout(Duration::from_secs(20), move || {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| check(&bytes)));
            outcome.is_ok()
        });
        match result {
            Some(true) => {}
            Some(false) => panic!(
                "parser panic-safety violation: entrypoint `{name}` on adversarial input class `{}` did \
                 not return gracefully",
                c.class
            ),
            None => panic!(
                "entrypoint `{name}` on adversarial input class `{}` did not complete within the timeout \
                 (possible hang)",
                c.class
            ),
        }
    }
}

#[test]
fn sevenz_listing_never_panics() {
    let magic = build_valid_7z();
    let header_len = magic.len();
    run_battery_small_stack("archive::read_archive_entries (.7z)", &magic, header_len, |b| {
        let f = write_temp(b, ".7z");
        let r = read_archive_entries(f.path().to_str().unwrap());
        if b.is_empty() {
            assert!(r.is_err(), "read_archive_entries(empty .7z) must be Err, not a panic");
        }
    });
}

#[test]
fn sevenz_extraction_never_panics() {
    let magic = build_valid_7z();
    let header_len = magic.len();
    run_battery_small_stack("archive::extract_archive_entry_any (.7z)", &magic, header_len, |b| {
        let f = write_temp(b, ".7z");
        let _ = extract_archive_entry_any(f.path().to_str().unwrap(), "a.txt");
    });
    run_battery_small_stack("archive::extract_archive (.7z)", &magic, header_len, |b| {
        let f = write_temp(b, ".7z");
        let dest = fresh_dest_dir();
        let _ = extract_archive(f.path().to_str().unwrap(), dest.path().to_str().unwrap());
    });
}

#[test]
fn sevenz_listing_and_extraction_handle_many_entries() {
    // Kept to 500 (rather than the zip/tar batteries' thousands) — each entry goes through a real LZMA
    // encode via `SevenZWriter`, so this is already meaningfully slower per-entry; 500 is enough to prove
    // "many entries" doesn't panic/hang without making the suite slow.
    let f = tempfile::Builder::new().suffix(".7z").tempfile().unwrap();
    {
        let mut w = sevenz_rust::SevenZWriter::create(f.path()).unwrap();
        for i in 0..500 {
            let mut entry = sevenz_rust::SevenZArchiveEntry::new();
            entry.name = format!("f{i}.txt");
            w.push_archive_entry(entry, Some(std::io::Cursor::new(b"x".to_vec()))).unwrap();
        }
        w.finish().unwrap();
    }
    let path = f.path().to_str().unwrap();

    assert_no_panic("archive::read_archive_entries (7z, many entries)", "500_entries", || {
        let entries = read_archive_entries(path).expect("a validly-built 7z must still list");
        assert_eq!(entries.len(), 500);
    });
    let dest = fresh_dest_dir();
    assert_no_panic("archive::extract_archive (7z, many entries)", "500_entries", || {
        let _ = extract_archive(path, dest.path().to_str().unwrap());
    });
}

/// The 32-byte 7z "signature header": 6-byte magic + 2-byte version + 4-byte `StartHeaderCRC` (a real
/// CRC-32 of the next 20 bytes, computed here, so the header passes `sevenz-rust`'s own validity check and
/// the walk actually reaches the interesting arithmetic) + 8-byte `NextHeaderOffset` (LE) + 8-byte
/// `NextHeaderSize` (LE) + 4-byte `NextHeaderCRC` (unchecked before the bug below is reached).
fn craft_7z_signature_header(next_header_offset: u64, next_header_size: u64) -> Vec<u8> {
    let mut rest20 = Vec::with_capacity(20);
    rest20.extend_from_slice(&next_header_offset.to_le_bytes());
    rest20.extend_from_slice(&next_header_size.to_le_bytes());
    rest20.extend_from_slice(&0u32.to_le_bytes()); // NextHeaderCRC
    let start_header_crc = crc32(&rest20);

    let mut sig = Vec::with_capacity(32);
    sig.extend_from_slice(&[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]); // "7z\xBC\xAF\x27\x1C"
    sig.push(0); // major version — must be 0 or sevenz-rust rejects immediately
    sig.push(4); // minor version — unchecked
    sig.extend_from_slice(&start_header_crc.to_le_bytes());
    sig.extend_from_slice(&rest20);
    sig
}

// ---------------------------------------------------------------------------------------------
// Upstream-tracking pins (CPE-1415): `archive.rs` now wraps every `sevenz-rust` call in
// `catch_sevenz_panic` (`std::panic::catch_unwind`), so the two crafted-signature-header cases below no
// longer panic THROUGH `archive.rs`'s public functions — they return a clean `Err` instead. That's the
// defensive mitigation this ticket asked for: it turns the panic into a normal handled error right at the
// `sevenz-rust` call site instead of relying on the outer Tokio `spawn_blocking` task boundary that
// already contained it (per CPE-1411's finding, reproduced below).
//
// The two tests immediately below call `sevenz_rust::Archive::read` DIRECTLY — bypassing `archive.rs` and
// its `catch_sevenz_panic` wrapper entirely — specifically so the "does upstream still have this bug"
// signal isn't lost now that `archive.rs` catches it. If a future `sevenz-rust` upgrade fixes either
// overflow, THAT test (not the `_is_caught_and_returns_err` ones further down) is the one that flips red —
// the correct signal to come back, delete the `#[should_panic]`, and consider whether `catch_sevenz_panic`
// is still worth keeping (harmless either way, so no urgency to remove it).
// ---------------------------------------------------------------------------------------------

#[test]
#[should_panic(expected = "attempt to add with overflow")]
fn sevenz_rust_upstream_signature_header_offset_overflow_still_panics() {
    // ============================================================================================
    // REAL BUG FOUND (CPE-1411), upstream in `sevenz-rust` 0.6.1, NOT fixable from `archive.rs`:
    //
    // `Archive::init_archive` (sevenz-rust-0.6.1/src/reader.rs:341-345) does:
    //     reader.seek(SeekFrom::Start(SIGNATURE_HEADER_SIZE + start_header.next_header_offset))
    // — a bare, unchecked `u64 + u64` addition. `next_header_offset` is 8 raw bytes read straight out of
    // the signature header via `read_u64le` (reader.rs:239) with NO range check before this add. Any
    // `.7z` file (real or crafted) whose header claims a `next_header_offset` near `u64::MAX` overflows
    // this addition. In a debug build — the profile `cargo test` uses by default, and the one this repo's
    // `crates/server/Cargo.toml` has no `[profile]` override to change — Rust's checked arithmetic makes
    // that overflow a hard panic ("attempt to add with overflow"), reachable from
    // `archive::read_archive_entries`/`extract_archive_entry_any`/`extract_archive` on any `.7z` path.
    // (There is a second, related issue one line later: the `next_header_size > usize::MAX as u64` guard
    // at reader.rs:332 is a no-op on any 64-bit target, since `usize::MAX as u64 == u64::MAX` there — so
    // an attacker-controlled `next_header_size` can reach `vec![0; next_header_size as usize]` at
    // reader.rs:347 completely unchecked. A `next_header_size` beyond `isize::MAX` at least fails safely,
    // as Rust's own allocation-capacity check ("capacity overflow"), which is a catchable panic — that's
    // the `size_u64_max` case exercised below. A `next_header_size` BETWEEN `isize::MAX` and whatever RAM
    // is actually available would instead attempt a real, and possibly succeeding-just-slowly-then-OOM,
    // multi-exabyte allocation; that variant is deliberately NOT exercised here, since an actual failed
    // allocation of that shape aborts the process (uncatchable — would kill this whole test binary, not
    // just fail one assertion), which is not a safe thing to trigger in-process in a shared CI run.)
    //
    // This is squarely a panic reachable from fully attacker-controlled bytes (a malicious .7z a user
    // opens in the preview pane), but the fix belongs in `sevenz-rust` itself (`checked_add`/`saturating_add`
    // on the seek offset, and a real bound — against the actual reader length, not just `usize::MAX` — on
    // `next_header_size` before allocating). There is no small, correct guard `archive.rs` can add at its
    // call site: doing so would mean re-implementing `sevenz-rust`'s own signature-header parsing just to
    // pre-validate it, which is not "small." Per CPE-1411's instructions this was reported, not patched at
    // the parser level — CPE-1415 instead added a `catch_unwind` net around every `archive.rs` call site
    // (see the `_is_caught_and_returns_err` tests below), but the raw upstream bug pinned HERE is
    // unchanged: this test calls `sevenz_rust::Archive::read` directly, with no `catch_unwind` in between.
    // ============================================================================================
    let sig = craft_7z_signature_header(u64::MAX - 5, 64);
    let f = write_temp(&sig, ".7z");
    let mut file = std::fs::File::open(f.path()).expect("failed to open crafted 7z fixture");
    let len = file.metadata().expect("failed to stat crafted 7z fixture").len();
    let _ = sevenz_rust::Archive::read(&mut file, len, &[]); // panics (see above)
}

// The other half of the finding above (asserted separately since `#[should_panic]` only pins ONE
// expected-panic call per test): a `next_header_size` past `isize::MAX` with a SAFE offset (0), isolating
// the allocation-capacity path from the seek-overflow path. Rust's own `Vec` capacity-overflow guard
// catches this before any real allocation is attempted, so — unlike the case above — this one is a
// legitimate "never panics... no wait, it panics but it's OUR call, and it's a *safe*, catchable panic"
// situation. It's still `#[should_panic]`, for the same "loudly tell us if upstream changes" reason, and
// — like the test above — calls `sevenz_rust::Archive::read` directly so it keeps pinning the raw upstream
// behavior regardless of `archive.rs`'s `catch_sevenz_panic` wrapper.
#[test]
#[should_panic(expected = "capacity overflow")]
fn sevenz_rust_upstream_signature_header_size_overflow_still_panics() {
    let sig = craft_7z_signature_header(0, u64::MAX);
    let f = write_temp(&sig, ".7z");
    let mut file = std::fs::File::open(f.path()).expect("failed to open crafted 7z fixture");
    let len = file.metadata().expect("failed to stat crafted 7z fixture").len();
    let _ = sevenz_rust::Archive::read(&mut file, len, &[]); // panics (see above)
}

// ---------------------------------------------------------------------------------------------
// CPE-1415 deliverable: the SAME crafted bytes, now proven to return a clean `Err` — not panic, not hang —
// through `archive.rs`'s public listing AND extraction paths, thanks to `catch_sevenz_panic`. Run on the
// small-stack timeout-guarded thread like the rest of this file's 7z batteries, so a regression that turns
// the catch into a hang (rather than a clean return) still fails loudly instead of wedging the test binary.
// ---------------------------------------------------------------------------------------------

#[test]
fn sevenz_signature_header_offset_overflow_is_caught_and_returns_err() {
    let sig = craft_7z_signature_header(u64::MAX - 5, 64);
    let f = write_temp(&sig, ".7z");
    let path = f.path().to_str().unwrap().to_string();

    let listing = run_with_timeout(Duration::from_secs(10), {
        let path = path.clone();
        move || read_archive_entries(&path)
    });
    match listing {
        // `ArchiveEntry` has no `Debug` impl (it's a plain `Serialize` DTO), so report the entry count on
        // an unexpected `Ok` rather than trying to `{:?}`-format the whole `Result`.
        Some(Ok(entries)) => panic!(
            "expected a clean Err (panic caught by catch_sevenz_panic), got Ok with {} entries",
            entries.len()
        ),
        Some(Err(_)) => {}
        None => panic!("read_archive_entries hung instead of returning a clean Err (CPE-1415 regression)"),
    }

    let extraction = run_with_timeout(Duration::from_secs(10), {
        let path = path.clone();
        move || extract_archive_entry_any(&path, "a.txt")
    });
    match extraction {
        Some(r) => assert!(r.is_err(), "expected a clean Err (panic caught by catch_sevenz_panic), got {r:?}"),
        None => panic!("extract_archive_entry_any hung instead of returning a clean Err (CPE-1415 regression)"),
    }

    let dest = fresh_dest_dir();
    let dest_path = dest.path().to_str().unwrap().to_string();
    let full_extract = run_with_timeout(Duration::from_secs(10), move || extract_archive(&path, &dest_path));
    match full_extract {
        Some(r) => assert!(r.is_err(), "expected a clean Err (panic caught by catch_sevenz_panic), got {r:?}"),
        None => panic!("extract_archive hung instead of returning a clean Err (CPE-1415 regression)"),
    }
}

#[test]
fn sevenz_signature_header_size_overflow_is_caught_and_returns_err() {
    let sig = craft_7z_signature_header(0, u64::MAX);
    let f = write_temp(&sig, ".7z");
    let path = f.path().to_str().unwrap().to_string();

    let listing = run_with_timeout(Duration::from_secs(10), move || read_archive_entries(&path));
    match listing {
        Some(Ok(entries)) => panic!(
            "expected a clean Err (panic caught by catch_sevenz_panic), got Ok with {} entries",
            entries.len()
        ),
        Some(Err(_)) => {}
        None => panic!("read_archive_entries hung instead of returning a clean Err (CPE-1415 regression)"),
    }
}

// ---------------------------------------------------------------------------------------------
// ISO 9660 — listing only (`iso_entries` via `read_archive_entries`; there is no `.iso` branch in any
// `extract_*` function, so extraction is out of scope here — confirmed by reading `archive.rs`).
//
// `iso9660` 0.1.1 has no writer, so — like `archive.rs`'s own `write_7z_fixture`/`craft_zip_with_entry_name`
// test helpers — this hand-assembles the on-disk structures `ISO9660::new`/`iso_entries` actually touch:
// the System Area, one Primary Volume Descriptor, a Volume Descriptor Set Terminator, and one directory
// extent. Field offsets are taken directly from the crate's own parser
// (`iso9660-0.1.1/src/parse/volume_descriptor.rs` + `directory_entry.rs`), not the ISO 9660 spec, so the
// fixture is guaranteed to walk the exact code path under test.
// ---------------------------------------------------------------------------------------------

const SECTOR: usize = 2048;

fn push_both_endian16(buf: &mut Vec<u8>, v: u16) {
    buf.extend_from_slice(&v.to_le_bytes());
    buf.extend_from_slice(&v.to_be_bytes()); // read by the crate's `take(2)` but never interpreted
}

fn push_both_endian32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_le_bytes());
    buf.extend_from_slice(&v.to_be_bytes()); // ditto, `take(4)`
}

/// One ISO 9660 directory record (`directory_entry()` in the crate's parser): a self-contained,
/// length-prefixed block naming a file or subdirectory. `identifier` is the raw identifier bytes — pass
/// `&[0x00]`/`&[0x01]` for the special self (".") / parent ("..") entries.
fn iso_dir_record(extent_loc: u32, extent_length: u32, is_dir: bool, identifier: &[u8]) -> Vec<u8> {
    let mut rec = Vec::new();
    rec.push(0); // length — patched below once the real size is known
    rec.push(0); // extended_attribute_record_length
    push_both_endian32(&mut rec, extent_loc);
    push_both_endian32(&mut rec, extent_length);
    rec.extend_from_slice(&[0u8; 7]); // date_time: all-zero parses gracefully (year 1900, epoch fallback)
    rec.push(if is_dir { 0x02 } else { 0x00 }); // file_flags: DIRECTORY bit
    rec.push(0); // file_unit_size
    rec.push(0); // interleave_gap_size
    push_both_endian16(&mut rec, 1); // volume_sequence_number
    rec.push(identifier.len() as u8);
    rec.extend_from_slice(identifier);
    let len = rec.len() as u8;
    rec[0] = len;
    rec
}

/// The 2048-byte Primary Volume Descriptor sector: the fixed header + every field `primary_descriptor()`
/// reads, in order, sized exactly as that function consumes them (verified byte-for-byte against
/// `volume_descriptor.rs` while building this fixture).
fn iso_primary_volume_descriptor(root_extent_loc: u32, root_extent_length: u32) -> Vec<u8> {
    let mut d = Vec::with_capacity(SECTOR);
    d.push(1); // type_code = Primary
    d.extend_from_slice(b"CD001");
    d.push(1); // version — folded into the crate's own `tag("CD001\x01")` match
    d.push(0); // padding ("unused" in the real spec)
    d.extend_from_slice(&[0u8; 32]); // system_identifier
    d.extend_from_slice(&[0u8; 32]); // volume_identifier
    d.extend_from_slice(&[0u8; 8]); // padding
    push_both_endian32(&mut d, 19); // volume_space_size — cosmetic, unchecked by `ISO9660::new`
    d.extend_from_slice(&[0u8; 32]); // padding
    push_both_endian16(&mut d, 1); // volume_set_size
    push_both_endian16(&mut d, 1); // volume_sequence_number
    push_both_endian16(&mut d, 2048); // logical_block_size — the ONE field `ISO9660::new` actually checks
    push_both_endian32(&mut d, 0); // path_table_size
    d.extend_from_slice(&0u32.to_le_bytes()); // path_table_loc
    d.extend_from_slice(&0u32.to_le_bytes()); // optional_path_table_loc
    d.extend_from_slice(&[0u8; 4]); // path_table_loc (BE, unread)
    d.extend_from_slice(&[0u8; 4]); // optional_path_table_loc (BE, unread)
    d.extend_from_slice(&iso_dir_record(root_extent_loc, root_extent_length, true, &[0x00])); // root dir record
    d.extend_from_slice(&[0u8; 128]); // volume_set_identifier
    d.extend_from_slice(&[0u8; 128]); // publisher_identifier
    d.extend_from_slice(&[0u8; 128]); // data_preparer_identifier
    d.extend_from_slice(&[0u8; 128]); // application_identifier
    d.extend_from_slice(&[0u8; 38]); // copyright_file_identifier
    d.extend_from_slice(&[0u8; 36]); // abstract_file_identifier
    d.extend_from_slice(&[0u8; 37]); // bibliographic_file_identifier
    let ascii_dt = b"0000000000000000\0"; // 16 ASCII-digit chars (parsed as ints) + 1 raw gmt_offset byte
    for _ in 0..4 {
        d.extend_from_slice(ascii_dt); // creation / modification / expiration / effective time
    }
    d.push(1); // file_structure_version
    d.resize(SECTOR, 0);
    d
}

fn iso_volume_descriptor_set_terminator() -> Vec<u8> {
    let mut d = Vec::with_capacity(SECTOR);
    d.push(255);
    d.extend_from_slice(b"CD001");
    d.push(1);
    d.resize(SECTOR, 0);
    d
}

/// A root directory extent listing "." / ".." (both self-referencing, single-level fixture) and one file.
fn build_root_directory_extent() -> Vec<u8> {
    let mut dir = Vec::new();
    dir.extend_from_slice(&iso_dir_record(18, SECTOR as u32, true, &[0x00])); // "."
    dir.extend_from_slice(&iso_dir_record(18, SECTOR as u32, true, &[0x01])); // ".."
    dir.extend_from_slice(&iso_dir_record(19, 8, false, b"HELLO.TXT;1")); // one file, declared size 8
    dir.resize(SECTOR, 0);
    dir
}

/// A minimal-but-real ISO image: 16-sector system area (LBA 0..16), one Primary Volume Descriptor (LBA
/// 16, root directory at LBA 18), the Volume Descriptor Set Terminator (LBA 17), and the root directory
/// extent (LBA 18) — everything `iso_entries` (via `read_archive_entries`) actually walks for listing. No
/// file-content sector: `iso_entries` only reads directory metadata, never a file's own extent.
fn build_valid_iso() -> Vec<u8> {
    let mut img = vec![0u8; 16 * SECTOR]; // system area
    img.extend_from_slice(&iso_primary_volume_descriptor(18, SECTOR as u32)); // LBA 16
    img.extend_from_slice(&iso_volume_descriptor_set_terminator()); // LBA 17
    img.extend_from_slice(&build_root_directory_extent()); // LBA 18
    img
}

#[test]
fn iso_listing_lists_the_valid_fixture_and_never_panics_on_truncation_or_garbage() {
    let valid = build_valid_iso();
    assert_no_panic("archive::read_archive_entries (.iso)", "valid_fixture", || {
        let f = write_temp(&valid, ".iso");
        let entries = read_archive_entries(f.path().to_str().unwrap()).expect("a validly-built ISO must still list");
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        // `ISOFile::new` splits the `;N` version suffix off the identifier (it's tracked separately as
        // `version`, not exposed by `archive::ArchiveEntry`), so the listed name is "HELLO.TXT", not
        // "HELLO.TXT;1".
        assert!(entries.iter().any(|e| e.name == "HELLO.TXT" && e.size == 8), "got {names:?}");
    });

    // Custom (not the generic magic+tail battery, which assumes a short signature at byte 0 — ISO's
    // mandatory 32 KiB all-zero System Area makes that generator's per-byte truncation loop needlessly
    // huge for no extra coverage): truncate at every structurally interesting boundary, and replace the
    // whole image with all-zeros/all-0xFF/seeded-random of the same length.
    let boundaries = [0, 1, 100, 32768, 32768 + 1, 32768 + 50, 34816, 34816 + 1, 34816 + 50, 36864, 36864 + 50, valid.len() - 1];
    for &cut in &boundaries {
        let case = format!("truncated_at_{cut}");
        assert_no_panic("archive::read_archive_entries (.iso)", &case, || {
            let f = write_temp(&valid[..cut.min(valid.len())], ".iso");
            let _ = read_archive_entries(f.path().to_str().unwrap());
        });
    }
    for (label, bytes) in [
        ("all_zeros", vec![0u8; valid.len()]),
        ("all_0xff", vec![0xFFu8; valid.len()]),
        ("seeded_random", (0..valid.len()).map(|i| ((i * 2654435761u64.wrapping_add(i as u64) as usize) % 256) as u8).collect()),
    ] {
        assert_no_panic("archive::read_archive_entries (.iso)", label, || {
            let f = write_temp(&bytes, ".iso");
            let _ = read_archive_entries(f.path().to_str().unwrap());
        });
    }
}

/// Same fixture as [`build_valid_iso`], but the root directory extent (LBA 18) is 2048 bytes of `0xFF`
/// instead of real directory records — see the finding documented on the test below.
fn build_iso_with_unparseable_root_dir() -> Vec<u8> {
    let mut img = vec![0u8; 16 * SECTOR];
    img.extend_from_slice(&iso_primary_volume_descriptor(18, SECTOR as u32));
    img.extend_from_slice(&iso_volume_descriptor_set_terminator());
    img.extend_from_slice(&vec![0xFFu8; SECTOR]); // LBA 18: garbage — every directory-record parse fails
    img
}

#[test]
fn iso_listing_does_not_hang_on_an_unparseable_directory_record() {
    // ============================================================================================
    // REAL BUG FOUND (CPE-1411) — in `archive.rs` itself. FIXED as part of this ticket.
    //
    // `iso9660` 0.1.1's `ISODirectoryIterator::next()` (src/directory_entry/isodirectory.rs) does NOT
    // advance its `next_offset` cursor when `read_entry_at` returns `Err` (a directory-record parse
    // failure, e.g. a length-prefixed identifier whose bytes aren't valid UTF-8 — exactly what a root
    // directory extent full of `0xFF` produces on the very first record). Calling `.next()` again after
    // that `Err` re-reads the SAME bytes and gets the SAME `Err` — forever. `iso_entries`'s inner loop
    // used to be:
    //     for entry in dir.contents() {
    //         let entry = match entry { Ok(e) => e, Err(_) => continue };
    //         ...
    //     }
    // `continue` calls `.next()` again immediately — combined with the upstream iterator's non-advancing
    // cursor, that is an unconditional infinite loop: ANY `.iso` whose root (or any visited) directory
    // contains one unparseable record hangs the whole listing thread forever, no timeout, no error, just
    // a stuck preview pane. This is a real, fully attacker-reachable (a user opens a malformed `.iso`)
    // denial-of-service, arguably worse than a panic since a panic at least gets caught and reported.
    //
    // FIX (small, bounded, applied in this PR): change that `Err(_) => continue` to `Err(_) => break` —
    // stop reading the REST of *this one* directory's entries (the same "skip what we can't read" spirit
    // CLAUDE.md's filesystem guardrail already requires everywhere else), while the outer
    // `while let Some(..) = stack.pop()` still moves on to any other directories already queued. This is
    // strictly a partial-listing improvement over the old code (which never reached later siblings
    // either — it just hung instead of skipping them), so it changes no currently-passing behavior.
    //
    // This test proves the fix: it must return within the timeout, not hang. (Verified manually against
    // the pre-fix code that it does reproduce the hang this describes before the `continue`→`break` fix.)
    // ============================================================================================
    let bytes = build_iso_with_unparseable_root_dir();
    let f = write_temp(&bytes, ".iso");
    let path = f.path().to_str().unwrap().to_string();

    let result = run_with_timeout(Duration::from_secs(10), move || read_archive_entries(&path));
    assert!(result.is_some(), "read_archive_entries hung on an unparseable ISO directory record (CPE-1411)");
    // Whichever it is — `Ok(partial)` or `Err` — is fine; termination is the property under test, not a
    // particular result shape.
}
