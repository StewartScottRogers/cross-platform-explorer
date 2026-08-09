//! File split/join (CPE-1491): chunk a large file into fixed-size numbered parts (`<name>.001`,
//! `<name>.002`, …) plus a small JSON manifest, and rejoin the parts back into the original — the
//! classic orthodox-commander utility for FAT32/USB size limits, chunked uploads, and email-attachment
//! splits. Both directions stream through a **bounded** chunk buffer (never a whole-file or whole-part
//! read), and the whole-file SHA-256 is computed in the same pass as the I/O (split hashes while it
//! writes; join hashes while it reconstructs) — reusing the streaming approach of
//! [`crate::fsutil::sha256_file`] (CPE-412/737) rather than a second pass or a new hashing dep.
//! Pure and Tauri-free (CPE-815); the Tauri commands are thin `spawn_blocking` dispatchers.
//!
//! Overwrite policy (decided here, since the ticket calls it out): **split** refuses if the manifest or
//! any target part file already exists in `out_dir`; **join** refuses if `out_path` already exists. Both
//! fail loudly rather than silently clobbering something — callers that want to replace prior output
//! delete it first (or the future GUI dialog offers that as an explicit choice, CPE-1509).

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Bounded chunk buffer for both split's source reader and join's part reader — a multi-GB file never
/// loads into memory. Matches the streaming convention (docs/design/STREAMING.md).
const CHUNK_SIZE: usize = 1024 * 1024; // 1 MiB

/// Hard cap on the number of parts a split will create, or a join (incl. a hostile manifest) will
/// accept. Guards against `part_size` being pathologically small relative to the source (split) and
/// against a corrupt/hostile manifest claiming an absurd part count (join). 100k parts is already far
/// beyond any real use of this feature.
const MAX_PARTS: u64 = 100_000;

/// A manifest is JSON and always tiny in practice; cap the read so a hostile stand-in file (someone
/// drops a multi-GB file where the manifest should be) can't be read into memory whole.
const MAX_MANIFEST_BYTES: u64 = 64 * 1024;

/// Suffix of the manifest file written alongside a split's parts: `<original_name>.split-manifest.json`.
const MANIFEST_SUFFIX: &str = ".split-manifest.json";

/// The manifest written by [`split_file`] and consumed by [`join_files`]. Deliberately small — just
/// enough to relocate the ordered parts and verify the reconstruction; the part width (zero-padding) is
/// re-derived from `part_count` rather than stored, and part sizes are derived from `total_size`/
/// `part_size`/`part_count` rather than stored per-part.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct SplitManifest {
    /// The original file's name (not path) — parts are named `<original_name>.NNN` and join's default
    /// output uses this name.
    pub original_name: String,
    /// Total size of the original file, in bytes.
    pub total_size: u64,
    /// Number of parts written. A 0-byte source has `part_count == 0` (no part files at all) — see
    /// [`split_file`].
    pub part_count: u64,
    /// The requested part size in bytes. Every part is exactly this size except the last, which holds
    /// the remainder.
    pub part_size: u64,
    /// SHA-256 of the whole original file, lowercase hex — recomputed from the concatenated parts on
    /// join and compared against this.
    pub sha256: String,
}

fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Zero-padded width for part sequence numbers: 3 digits (`.001`) unless `part_count` itself needs more.
fn part_width(part_count: u64) -> usize {
    part_count.max(1).to_string().len().max(3)
}

fn part_path(out_dir: &Path, original_name: &str, index: u64, width: usize) -> PathBuf {
    out_dir.join(format!("{original_name}.{index:0width$}"))
}

/// Split the file at `path` into fixed-`part_size` parts under `out_dir`, plus a manifest. Streams the
/// source through a bounded 1 MiB buffer, hashing the whole file in the same pass. Refuses `part_size ==
/// 0`, a directory `path`, or a `part_size` so small relative to the source that it would blow the
/// [`MAX_PARTS`] cap. Refuses to overwrite a pre-existing manifest or part file in `out_dir`.
pub fn split_file(path: &Path, part_size: u64, out_dir: &Path) -> Result<SplitManifest, String> {
    if part_size == 0 {
        return Err("part_size must be greater than 0".to_string());
    }
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if meta.is_dir() {
        return Err(format!("{}: is a folder", path.display()));
    }
    let total_size = meta.len();
    let original_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("{}: not a valid file name", path.display()))?
        .to_string();

    // A 0-byte source needs no parts at all — see the struct doc on `part_count`.
    let part_count = if total_size == 0 { 0 } else { total_size.div_ceil(part_size) };
    if part_count > MAX_PARTS {
        return Err(format!(
            "part_size {part_size} would split {total_size} bytes into {part_count} parts, over the \
             {MAX_PARTS}-part cap — choose a larger part size"
        ));
    }

    std::fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;

    let manifest_path = out_dir.join(format!("{original_name}{MANIFEST_SUFFIX}"));
    if manifest_path.exists() {
        return Err(format!("{}: already exists — remove it before re-splitting", manifest_path.display()));
    }
    let width = part_width(part_count);
    let mut part_paths = Vec::with_capacity(part_count as usize);
    for i in 1..=part_count {
        let p = part_path(out_dir, &original_name, i, width);
        if p.exists() {
            return Err(format!("{}: already exists — remove it before re-splitting", p.display()));
        }
        part_paths.push(p);
    }

    let mut src = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; CHUNK_SIZE];

    let mut part_idx: usize = 0;
    let mut part_file: Option<File> = None;
    let mut bytes_in_part: u64 = 0;

    loop {
        let n = src.read(&mut buf).map_err(|e| format!("{}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        let mut off = 0usize;
        while off < n {
            if part_file.is_none() {
                let p = part_paths
                    .get(part_idx)
                    .ok_or_else(|| "internal error: ran out of parts mid-split".to_string())?;
                part_file = Some(File::create(p).map_err(|e| format!("{}: {e}", p.display()))?);
                bytes_in_part = 0;
            }
            let remaining_in_part = part_size - bytes_in_part;
            let take = remaining_in_part.min((n - off) as u64) as usize;
            let f = part_file.as_mut().expect("just ensured Some above");
            f.write_all(&buf[off..off + take]).map_err(|e| e.to_string())?;
            bytes_in_part += take as u64;
            off += take;
            if bytes_in_part == part_size {
                f.flush().map_err(|e| e.to_string())?;
                part_file = None;
                part_idx += 1;
            }
        }
    }
    if let Some(mut f) = part_file.take() {
        f.flush().map_err(|e| e.to_string())?;
    }

    let sha256 = to_hex(&hasher.finalize());
    let manifest = SplitManifest { original_name, total_size, part_count, part_size, sha256 };
    let json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    std::fs::write(&manifest_path, json).map_err(|e| format!("{}: {e}", manifest_path.display()))?;
    Ok(manifest)
}

/// Resolve `first_part_or_manifest` (either the manifest itself, or any one numbered part — conventionally
/// `.001`) to the manifest's path, which is also how the ordered parts are located: `dir.join(original_name
/// + ".NNN")` for each `1..=part_count`.
fn resolve_manifest_path(first_part_or_manifest: &Path) -> Result<PathBuf, String> {
    let file_name = first_part_or_manifest
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("{}: not a valid file name", first_part_or_manifest.display()))?;

    if let Some(stem) = file_name.strip_suffix(MANIFEST_SUFFIX) {
        if stem.is_empty() {
            return Err(format!("{file_name}: not a valid split manifest name"));
        }
        if !first_part_or_manifest.is_file() {
            return Err(format!("{}: manifest not found", first_part_or_manifest.display()));
        }
        return Ok(first_part_or_manifest.to_path_buf());
    }

    // Otherwise treat it as a numbered part: `<stem>.NNN` — strip the trailing `.NNN` to recover the
    // original name the manifest was written under.
    let dot = file_name.rfind('.').ok_or_else(|| {
        format!(
            "{file_name}: not a split manifest or a numbered part (expected \
             '<name>{MANIFEST_SUFFIX}' or '<name>.NNN')"
        )
    })?;
    let (stem, seq_with_dot) = file_name.split_at(dot);
    let seq = &seq_with_dot[1..];
    if stem.is_empty() || seq.is_empty() || !seq.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!(
            "{file_name}: not a split manifest or a numbered part (expected \
             '<name>{MANIFEST_SUFFIX}' or '<name>.NNN')"
        ));
    }
    let dir = first_part_or_manifest.parent().unwrap_or_else(|| Path::new("."));
    let manifest_path = dir.join(format!("{stem}{MANIFEST_SUFFIX}"));
    if !manifest_path.is_file() {
        return Err(format!("{}: manifest not found for part {file_name}", manifest_path.display()));
    }
    Ok(manifest_path)
}

fn load_manifest(path: &Path) -> Result<SplitManifest, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if meta.len() > MAX_MANIFEST_BYTES {
        return Err(format!("{}: manifest is implausibly large ({} bytes)", path.display(), meta.len()));
    }
    let content = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&content).map_err(|e| format!("{}: invalid manifest JSON: {e}", path.display()))
}

/// Sanity-check a manifest loaded off disk before trusting it to drive a join — guards against both
/// accidental corruption and a hand-crafted hostile manifest (e.g. a claimed `part_count` designed to
/// exhaust resources, or an `original_name` that could escape `out_dir` — see [`crate::backup::safe_join`]
/// for the analogous check on plan-relative paths).
fn validate_manifest(m: &SplitManifest) -> Result<(), String> {
    if m.part_size == 0 {
        return Err("manifest is corrupt: part_size is 0".to_string());
    }
    if m.part_count > MAX_PARTS {
        return Err(format!("manifest is corrupt: part_count {} exceeds the {MAX_PARTS}-part cap", m.part_count));
    }
    let mut components = Path::new(&m.original_name).components();
    let name_is_plain =
        matches!(components.next(), Some(std::path::Component::Normal(_))) && components.next().is_none();
    if !name_is_plain {
        return Err("manifest is corrupt: original_name is not a plain file name".to_string());
    }
    if m.sha256.len() != 64 || !m.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("manifest is corrupt: sha256 is not a 64-character hex digest".to_string());
    }
    if m.part_count == 0 {
        if m.total_size != 0 {
            return Err("manifest is corrupt: part_count is 0 but total_size is not".to_string());
        }
    } else {
        let min = m.part_size.saturating_mul(m.part_count - 1) + 1;
        let max = m.part_size.saturating_mul(m.part_count);
        if m.total_size < min || m.total_size > max {
            return Err("manifest is corrupt: total_size is inconsistent with part_size/part_count".to_string());
        }
    }
    Ok(())
}

/// Rejoin the parts referenced by `first_part_or_manifest` (the manifest itself, or any one numbered
/// part) into `out_path`, streamed through a bounded 1 MiB buffer with the reconstructed SHA-256 computed
/// in the same pass. Errors — never panics — on a missing part, a part whose size doesn't match the
/// manifest, or a checksum mismatch after reconstruction (in which case the partial `out_path` is removed
/// rather than left behind looking like a good file). Refuses to overwrite a pre-existing `out_path`.
pub fn join_files(first_part_or_manifest: &Path, out_path: &Path) -> Result<(), String> {
    let manifest_path = resolve_manifest_path(first_part_or_manifest)?;
    let manifest = load_manifest(&manifest_path)?;
    validate_manifest(&manifest)?;

    if out_path.exists() {
        return Err(format!("{}: already exists — refusing to overwrite", out_path.display()));
    }

    let dir = manifest_path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
    // Any failure past this point (missing/short part, I/O error, checksum mismatch) removes the
    // partial `out_path` — a caller must never mistake a truncated or corrupt reconstruction for a
    // finished one just because a file exists at the target path.
    match join_into(&manifest, &dir, out_path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(out_path);
            Err(e)
        }
    }
}

/// The streamed concatenate-and-verify body of [`join_files`], factored out so every failure path shares
/// one cleanup point (see caller).
fn join_into(manifest: &SplitManifest, dir: &Path, out_path: &Path) -> Result<(), String> {
    let width = part_width(manifest.part_count);

    let mut out_file = File::create(out_path).map_err(|e| format!("{}: {e}", out_path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; CHUNK_SIZE];

    for i in 1..=manifest.part_count {
        let p = part_path(dir, &manifest.original_name, i, width);
        let expected_len = if i == manifest.part_count {
            manifest.total_size - manifest.part_size * (manifest.part_count - 1)
        } else {
            manifest.part_size
        };
        let part_meta = std::fs::metadata(&p).map_err(|_| format!("part {i} missing: {}", p.display()))?;
        if part_meta.len() != expected_len {
            return Err(format!(
                "part {i} is the wrong size: expected {expected_len} bytes, found {} ({})",
                part_meta.len(),
                p.display()
            ));
        }
        let mut part_file = File::open(&p).map_err(|e| format!("part {i} ({}): {e}", p.display()))?;
        loop {
            let n = part_file.read(&mut buf).map_err(|e| format!("part {i} ({}): {e}", p.display()))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            out_file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        }
    }
    out_file.flush().map_err(|e| e.to_string())?;
    drop(out_file);

    let digest = to_hex(&hasher.finalize());
    if digest != manifest.sha256 {
        return Err(format!(
            "checksum mismatch: reconstructed file does not match the manifest (expected {}, got {digest})",
            manifest.sha256
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn scratch(tag: &str) -> PathBuf {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-splitjoin-{}-{}-{}", tag, std::process::id(), n));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A deterministic, non-repeating-enough byte stream so a byte-flip or truncation is detectable
    /// (not accidentally still a valid prefix/suffix of itself).
    fn pattern(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    fn round_trip_exact_multiple_of_part_size() {
        let d = scratch("exact");
        let src = d.join("data.bin");
        let bytes = pattern(3 * 1024); // exactly 3 parts of 1024 bytes, no ragged final part
        std::fs::write(&src, &bytes).unwrap();

        let manifest = split_file(&src, 1024, &d).unwrap();
        assert_eq!(manifest.part_count, 3);
        assert_eq!(manifest.total_size, bytes.len() as u64);
        for i in 1..=3u64 {
            let p = part_path(&d, "data.bin", i, 3);
            assert_eq!(std::fs::metadata(&p).unwrap().len(), 1024, "part {i} should be exactly part_size");
        }

        let out = d.join("rejoined.bin");
        join_files(&d.join("data.bin.001"), &out).unwrap();
        let joined = std::fs::read(&out).unwrap();
        assert_eq!(joined, bytes, "round-tripped bytes must match the original exactly");
        assert_eq!(to_hex(&Sha256::digest(&joined)), manifest.sha256);

        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn round_trip_ragged_final_part() {
        let d = scratch("ragged");
        let src = d.join("photo.raw");
        let bytes = pattern(3 * 1024 + 777); // 3 full parts + a short 777-byte final part
        std::fs::write(&src, &bytes).unwrap();

        let manifest = split_file(&src, 1024, &d).unwrap();
        assert_eq!(manifest.part_count, 4);
        let last = part_path(&d, "photo.raw", 4, 3);
        assert_eq!(std::fs::metadata(&last).unwrap().len(), 777);

        let out = d.join("rejoined.raw");
        join_files(&d.join("photo.raw.split-manifest.json"), &out).unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), bytes);

        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn part_size_zero_is_err() {
        let d = scratch("zeropart");
        let src = d.join("x.bin");
        std::fs::write(&src, b"hello").unwrap();
        let err = split_file(&src, 0, &d).unwrap_err();
        assert!(err.contains("part_size"), "error should name the bad parameter: {err}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn missing_part_on_join_is_err_not_panic() {
        let d = scratch("missing");
        let src = d.join("a.bin");
        std::fs::write(&src, pattern(2500)).unwrap();
        split_file(&src, 1000, &d).unwrap(); // 3 parts: .001 .002 .003

        std::fs::remove_file(part_path(&d, "a.bin", 2, 3)).unwrap();

        let out = d.join("out.bin");
        let err = join_files(&d.join("a.bin.001"), &out).unwrap_err();
        assert!(err.contains("missing"), "should name the missing part: {err}");
        assert!(!out.exists(), "no output should be left behind on failure");

        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn corrupted_part_byte_flip_is_checksum_mismatch_err() {
        let d = scratch("corrupt");
        let src = d.join("b.bin");
        std::fs::write(&src, pattern(2500)).unwrap();
        split_file(&src, 1000, &d).unwrap();

        // Flip one byte in the middle part — same length, so the size check passes and only the
        // checksum catches it.
        let p2 = part_path(&d, "b.bin", 2, 3);
        let mut bytes = std::fs::read(&p2).unwrap();
        bytes[10] ^= 0xFF;
        std::fs::write(&p2, &bytes).unwrap();

        let out = d.join("out.bin");
        let err = join_files(&d.join("b.bin.001"), &out).unwrap_err();
        assert!(err.contains("checksum mismatch"), "should be a checksum-mismatch error: {err}");
        assert!(!out.exists(), "the bad reconstruction must be removed, not left behind");

        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn zero_byte_source_round_trips_with_no_part_files() {
        let d = scratch("zerosrc");
        let src = d.join("empty.dat");
        std::fs::write(&src, b"").unwrap();

        let manifest = split_file(&src, 1024, &d).unwrap();
        assert_eq!(manifest.part_count, 0, "a 0-byte source needs no parts — documented in split_file");
        assert_eq!(manifest.total_size, 0);
        assert!(!part_path(&d, "empty.dat", 1, 3).exists());

        let out = d.join("rejoined.dat");
        join_files(&d.join("empty.dat.split-manifest.json"), &out).unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), Vec::<u8>::new());

        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn large_synthetic_input_round_trips_through_the_bounded_chunk_buffer() {
        // Several MiB, sized so it does NOT divide evenly by CHUNK_SIZE or part_size — exercises the
        // multi-chunk-per-part *and* multi-part-per-chunk boundary logic. This is a behavioral proxy for
        // "never buffers the whole file": CHUNK_SIZE stays a fixed 1 MiB read buffer regardless of the
        // ~6 MiB source, which a full-file-read implementation would not need at all.
        let d = scratch("large");
        let src = d.join("big.bin");
        let len = 6 * 1024 * 1024 + 12_345;
        let bytes = pattern(len);
        std::fs::write(&src, &bytes).unwrap();

        let manifest = split_file(&src, 700_000, &d).unwrap(); // part_size not aligned to CHUNK_SIZE
        assert_eq!(manifest.part_count, (len as u64).div_ceil(700_000));

        let out = d.join("big_out.bin");
        join_files(&d.join("big.bin.001"), &out).unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), bytes);

        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn split_refuses_to_overwrite_an_existing_manifest_or_part() {
        let d = scratch("noclobber-split");
        let src = d.join("c.bin");
        std::fs::write(&src, pattern(2000)).unwrap();
        split_file(&src, 1000, &d).unwrap();

        // Re-splitting into the same out_dir must refuse rather than silently overwrite.
        let err = split_file(&src, 1000, &d).unwrap_err();
        assert!(err.contains("already exists"), "should refuse to clobber: {err}");

        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn join_refuses_to_overwrite_an_existing_out_path() {
        let d = scratch("noclobber-join");
        let src = d.join("d.bin");
        std::fs::write(&src, pattern(1500)).unwrap();
        split_file(&src, 1000, &d).unwrap();

        let out = d.join("already-here.bin");
        std::fs::write(&out, b"pre-existing content").unwrap();
        let err = join_files(&d.join("d.bin.001"), &out).unwrap_err();
        assert!(err.contains("already exists"), "should refuse to clobber: {err}");
        assert_eq!(std::fs::read(&out).unwrap(), b"pre-existing content", "must not touch the existing file");

        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn hostile_part_size_over_the_part_cap_is_err() {
        let d = scratch("cap");
        let src = d.join("e.bin");
        // Small file, but part_size=1 would need > MAX_PARTS parts — rejected on the arithmetic alone,
        // no actual writing attempted.
        std::fs::write(&src, pattern((MAX_PARTS as usize) + 1)).unwrap();
        let err = split_file(&src, 1, &d).unwrap_err();
        assert!(err.contains("cap"), "should name the cap: {err}");
        assert!(std::fs::read_dir(&d).unwrap().next().is_some(), "src file itself still exists");
        // No part files should have been written.
        let part_files: Vec<_> = std::fs::read_dir(&d)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".00"))
            .collect();
        assert!(part_files.is_empty(), "must fail fast before writing any parts");

        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn hostile_manifest_part_count_over_cap_is_rejected_on_join() {
        let d = scratch("hostile-manifest");
        let manifest = SplitManifest {
            original_name: "f.bin".to_string(),
            total_size: 1,
            part_count: MAX_PARTS + 1,
            part_size: 1,
            sha256: "a".repeat(64),
        };
        std::fs::write(d.join("f.bin.split-manifest.json"), serde_json::to_string(&manifest).unwrap()).unwrap();

        let out = d.join("out.bin");
        let err = join_files(&d.join("f.bin.split-manifest.json"), &out).unwrap_err();
        assert!(err.contains("cap"), "should reject a hostile part_count before touching any parts: {err}");
        assert!(!out.exists());

        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn join_from_a_part_path_and_from_the_manifest_path_are_equivalent() {
        let d = scratch("either-entry");
        let src = d.join("g.bin");
        std::fs::write(&src, pattern(2200)).unwrap();
        split_file(&src, 1000, &d).unwrap();

        let out_a = d.join("via-part.bin");
        join_files(&d.join("g.bin.002"), &out_a).unwrap();
        let out_b = d.join("via-manifest.bin");
        join_files(&d.join("g.bin.split-manifest.json"), &out_b).unwrap();

        assert_eq!(std::fs::read(&out_a).unwrap(), std::fs::read(&out_b).unwrap());
        let _ = std::fs::remove_dir_all(&d);
    }
}
