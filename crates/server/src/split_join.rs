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
        manifest_must_be_a_file(first_part_or_manifest, None)?;
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
    manifest_must_be_a_file(&manifest_path, Some(file_name))?;
    Ok(manifest_path)
}

/// The manifest equivalent of [`part_stat_error`], and it exists for the same reason.
///
/// These two call sites used `!Path::is_file()`, which is `metadata().map(..).unwrap_or(false)` — it folds
/// **every** stat failure into `false`, so a manifest that is sitting in the folder but cannot be stat'ed
/// (permission denied, a dead mount, a link that will not resolve) was reported as *"manifest not found"*.
/// Exactly the bug this ticket is named after, one call earlier: `resolve_manifest_path` runs before any
/// part is touched, so `join_files` could answer "not found" about a file the user can see before it ever
/// reached the fixed line.
///
/// Found by the PR #869 reviewer, who also established the sharper point — a `map_err(|_| ..)` sweep cannot
/// find this, and neither can a search for the *word* "missing", because the spelling here is a negated
/// `is_file()` producing "not found". `Path::try_exists()` is the std API that returns `io::Result` instead
/// of collapsing; `metadata()` is used here because the type matters too (a directory named like a manifest
/// is not a manifest, and saying so is more useful than "not found").
fn manifest_must_be_a_file(path: &Path, for_part: Option<&str>) -> Result<(), String> {
    match std::fs::metadata(path) {
        Ok(m) if m.is_file() => Ok(()),
        Ok(_) => Err(manifest_stat_error(path, for_part, None)),
        Err(e) => Err(manifest_stat_error(path, for_part, Some(&e))),
    }
}

/// The pure classification [`manifest_must_be_a_file`] delegates to — split out for the same reason
/// [`part_stat_error`] is: the taxonomy is then testable on every OS and account without depending on
/// permission bits, which are privilege- and filesystem-dependent.
///
/// `None` means the `stat` succeeded and the entry simply is not a file.
fn manifest_stat_error(path: &Path, for_part: Option<&str>, e: Option<&std::io::Error>) -> String {
    let suffix = for_part.map(|f| format!(" for part {f}")).unwrap_or_default();
    match e {
        None => format!("{}: not a file, so not a split manifest{suffix}", path.display()),
        Some(e) if e.kind() == std::io::ErrorKind::NotFound => {
            format!("{}: manifest not found{suffix}", path.display())
        }
        // Not provably absent, and we could not stat it. Say so, in the same shape `part_stat_error`
        // uses, so a manifest and a part failing the same way read the same way to a user.
        Some(e) => format!("manifest ({}): {e}{suffix}", path.display()),
    }
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
        // Checked (not saturating) arithmetic: a hostile manifest (e.g. part_size = u64::MAX, part_count = 2)
        // must be rejected as corrupt, never panic on overflow (debug) or silently wrap and defeat this
        // consistency check (release). A checked_mul success here also guarantees the same raw multiplication
        // when computing the last part's expected length below can't overflow.
        let overflow = || "manifest is corrupt: part_size/part_count overflow".to_string();
        let min = m
            .part_size
            .checked_mul(m.part_count - 1)
            .and_then(|v| v.checked_add(1))
            .ok_or_else(overflow)?;
        let max = m.part_size.checked_mul(m.part_count).ok_or_else(overflow)?;
        if m.total_size < min || m.total_size > max {
            return Err("manifest is corrupt: total_size is inconsistent with part_size/part_count".to_string());
        }
    }
    Ok(())
}

/// Rejoin the parts referenced by `first_part_or_manifest` (the manifest itself, or any one numbered
/// part) into `out_path`, streamed through a bounded 1 MiB buffer with the reconstructed SHA-256 computed
/// in the same pass. Errors — never panics — on a missing part, a part that is present but cannot be
/// stat'ed (the OS's own cause is reported, *not* "missing" — CPE-1687, see `part_stat_error`), a part
/// whose size doesn't match the manifest, or a checksum mismatch after reconstruction (in which case the
/// partial `out_path` is removed rather than left behind looking like a good file). Refuses to overwrite
/// a pre-existing `out_path`.
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

/// Turn a failed `stat` of part `i` into the message the user reads (CPE-1687).
///
/// "The part is not there" and "the part is there and I could not stat it" are **different answers**.
/// Only [`std::io::ErrorKind::NotFound`] is genuine absence; permission denied, a dead network mount, a
/// transient I/O error and a link that will not resolve all mean *we do not know*, and answering any of
/// them with "missing" sends the user hunting for a file that is sitting in the folder in front of them.
/// The non-absent case therefore reports the OS's own words in exactly the shape the `File::open` and
/// `read` calls further down [`join_into`] already use, so one part failing at `stat` and the same part
/// failing at `open` read the same way.
///
/// Pure, and taking the `io::Error` rather than doing the `stat` itself, so the taxonomy is testable on
/// every OS and CI account without depending on permission bits — the same reason
/// `dispatch::classify_path_error` is split out from its caller. (The end-to-end test in this module
/// still constructs a real unstattable part; this makes the *classification* deterministic.)
fn part_stat_error(i: u64, p: &Path, e: &std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::NotFound {
        format!("part {i} missing: {}", p.display())
    } else {
        format!("part {i} ({}): {e}", p.display())
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
        let part_meta = std::fs::metadata(&p).map_err(|e| part_stat_error(i, &p, &e))?;
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
        // CPE-1687 made "missing" conditional on `ErrorKind::NotFound`; a part that really was deleted
        // must still get it, and still name *which* part. This is the half of the taxonomy that was
        // already right, pinned so the fix can't be "achieved" by dropping the word altogether.
        assert!(err.contains("part 2 missing"), "should name the missing part: {err}");
        assert!(!out.exists(), "no output should be left behind on failure");

        let _ = std::fs::remove_dir_all(&d);
    }

    /// Try to make `p` a directory entry that exists but cannot be `stat`ed, and report which mechanism
    /// worked (`None` = this machine can't produce the condition).
    ///
    /// Two mechanisms, tried in order, because the obvious one does not actually work:
    ///
    /// 1. **Permission denial** (`icacls /deny` on Windows, `chmod` on Unix) — the mechanism CPE-1687's
    ///    acceptance criteria name, and the one that fixed the sibling ticket CPE-1678. It cannot work
    ///    here, and the reason is worth writing down rather than rediscovering: on Unix, `stat()` on a
    ///    *file* needs no permission on the file at all (only `+x` on the parent directories), so
    ///    `chmod 000` leaves `fs::metadata` succeeding; and on Windows `fs::metadata` opens with a
    ///    desired-access mask of 0, which a per-file deny ACE does not refuse. Denying the *parent*
    ///    directory would work on both, but the manifest lives in that same directory and `join_files`
    ///    reads it before it ever reaches a part — the run would fail earlier, above the code under test.
    ///    Attempted anyway (and probed, never assumed) so the claim stays true if a future OS changes it.
    /// 2. **A symlink loop** — `a.bin.002 -> a.bin.002`. The entry is listed in the folder, so this is
    ///    exactly the user-visible complaint ("it is right there"), `symlink_metadata` sees it, and
    ///    `fs::metadata` fails resolving it with `FilesystemLoop`/ELOOP (Unix) or ERROR_CANT_RESOLVE_
    ///    FILENAME (Windows) — a non-`NotFound` stat failure, which is the whole point. Needs no
    ///    privilege on Unix; needs Developer Mode or elevation on Windows.
    ///
    /// The result is *probed*, never assumed: the caller runs only if the entry is genuinely present
    /// (`symlink_metadata` Ok) **and** `stat` genuinely fails for a reason that is not absence. Anything
    /// else is a machine that cannot host this test, which is not evidence of a bug — so the caller
    /// skips, loudly.
    fn make_unstattable(p: &Path) -> Option<&'static str> {
        fn denied_and_present(p: &Path) -> bool {
            std::fs::symlink_metadata(p).is_ok()
                && std::fs::metadata(p).is_err_and(|e| e.kind() != std::io::ErrorKind::NotFound)
        }

        // 1. Permission denial, on the real file the split just produced.
        #[cfg(windows)]
        {
            if let Ok(user) = std::env::var("USERNAME") {
                if !user.is_empty() {
                    let _ = std::process::Command::new("icacls")
                        .arg(p)
                        .arg("/deny")
                        .arg(format!("{user}:(RA,RD)"))
                        .output();
                }
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o000));
        }
        if denied_and_present(p) {
            return Some("permission denial");
        }

        // 2. Symlink loop, replacing the part file with a link to itself.
        undo_unstattable(p);
        if std::fs::remove_file(p).is_err() {
            return None;
        }
        #[cfg(windows)]
        let linked = std::os::windows::fs::symlink_file(p, p).is_ok();
        #[cfg(unix)]
        let linked = std::os::unix::fs::symlink(p, p).is_ok();
        if linked && denied_and_present(p) {
            return Some("symlink loop");
        }
        None
    }

    /// Undo whatever [`make_unstattable`] managed to do, so the scratch dir can be removed. Safe to call
    /// when nothing was done.
    fn undo_unstattable(p: &Path) {
        #[cfg(windows)]
        {
            if let Ok(user) = std::env::var("USERNAME") {
                let _ =
                    std::process::Command::new("icacls").arg(p).arg("/remove:d").arg(&user).output();
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o600));
        }
    }

    /// The same bug, one call earlier, found by the PR #869 reviewer: `resolve_manifest_path` used
    /// `!Path::is_file()`, which collapses every stat failure to `false`, so a manifest sitting in the
    /// folder but unstattable came back "manifest not found". It runs *before* any part is touched, so
    /// `join_files` could give the wrong answer without ever reaching the line this ticket fixed.
    ///
    /// Deliberately pure: the classification is tested here on every OS and account, and the end-to-end
    /// test below constructs a real unstattable entry. Same split as `part_stat_error`.
    #[test]
    fn a_manifest_that_cannot_be_stat_ed_is_not_reported_as_not_found() {
        let p = Path::new("some/dir/a.bin.split-manifest.json");

        // Genuine absence — the one case that may say "not found". Must not regress.
        let absent =
            manifest_stat_error(p, None, Some(&std::io::Error::from(std::io::ErrorKind::NotFound)));
        assert!(absent.contains("manifest not found"), "genuine absence must still say so: {absent}");

        // Every other stat failure is an unknown, not an absence. `!Path::is_file()` folded all of these
        // into the same `false` and produced "manifest not found" for each.
        // `ErrorKind::FilesystemLoop` — the kind a self-referential symlink actually produces, and the
        // one the end-to-end test below constructs for real — is still unstable to *name*, so it cannot
        // be listed here. `Other` stands in for it: the code branches on `NotFound` and nothing else, so
        // every non-`NotFound` kind takes the identical path.
        for kind in [
            std::io::ErrorKind::PermissionDenied,
            std::io::ErrorKind::Other,
            std::io::ErrorKind::InvalidData,
        ] {
            let e = std::io::Error::new(kind, "the OS said so");
            let msg = manifest_stat_error(p, None, Some(&e));
            assert!(!msg.contains("not found"), "a {kind:?} stat failure must not claim absence: {msg}");
            assert!(msg.contains("the OS said so"), "it must name the real cause: {msg}");
        }

        // A successful stat of something that is not a file is a *type* answer, not an absence.
        let wrong_type = manifest_stat_error(p, None, None);
        assert!(!wrong_type.contains("not found"), "a directory is not an absence: {wrong_type}");
        assert!(wrong_type.contains("not a file"), "{wrong_type}");

        // The part-name suffix survives on the numbered-part path, so the message still says which part
        // sent us looking for this manifest.
        let e = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "Access is denied.");
        let with_part = manifest_stat_error(p, Some("a.bin.002"), Some(&e));
        assert!(with_part.contains("for part a.bin.002"), "{with_part}");

        // And a manifest failing at stat reads the same way a part failing at stat does — the point of
        // the fix is that the user gets one consistent story, not two vocabularies for one problem.
        let part = part_stat_error(2, p, &e);
        assert!(!part.contains("missing"), "{part}");
        assert!(part.contains("Access is denied.") && with_part.contains("Access is denied."));
    }

    #[test]
    fn a_present_but_unstattable_part_names_the_cause_instead_of_calling_itself_missing() {
        // CPE-1687, end to end through the real `join_files` — the entry point the Tauri command calls —
        // because the message is what reaches the user and the internal helper's return type isn't.
        // Before the fix every `stat` failure came back "part 2 missing: <path>" about a part sitting
        // right there in the folder.
        let d = scratch("unstattable");
        let src = d.join("a.bin");
        std::fs::write(&src, pattern(2500)).unwrap();
        split_file(&src, 1000, &d).unwrap(); // 3 parts: .001 .002 .003
        let part2 = part_path(&d, "a.bin", 2, 3);
        let out = d.join("out.bin");

        // Armed BEFORE the assertions, not after: a failing assertion unwinds, and a plain cleanup call
        // after the asserts never runs on exactly the path that leaves debris. This repo mandates a red
        // run per guard, so that would be one permanently-odd file per developer per red run — the
        // reviewer of PR #865 found three real orphans from precisely this mistake (CPE-1678).
        struct Restore<'a>(&'a Path, &'a Path);
        impl Drop for Restore<'_> {
            fn drop(&mut self) {
                undo_unstattable(self.0);
                let _ = std::fs::remove_file(self.0);
                let _ = std::fs::remove_dir_all(self.1);
            }
        }
        let _restore = Restore(&part2, &d);

        let Some(mechanism) = make_unstattable(&part2) else {
            // A machine that cannot produce an unstattable-but-present entry (no symlink privilege on
            // Windows, an ACL-less filesystem, running as root) is a real and benign condition, so this
            // skips rather than fails. But it SAYS SO, because this is the only test that covers
            // CPE-1687 end to end — a silent skip would leave the suite green while guarding nothing,
            // which is the same "confident answer standing in for an unknown" the fix itself is about
            // (CPE-1680). CI's 3-OS matrix means the real run happens somewhere even if one leg skips.
            //
            // `writeln!(std::io::stderr(), ..)` and NOT `eprintln!` — load-bearing, do not "simplify".
            // libtest captures stdout/stderr per test and replays it only for FAILING tests; a skip is a
            // pass, so an `eprintln!` here is swallowed and reaches nobody. The capture works by
            // intercepting the `print!`/`eprint!` macros, so writing to the process's stderr handle goes
            // around it. CI runs plain `cargo test` with no `--nocapture`, which is the only case that
            // matters. CPE-1678 shipped this wrong once by verifying under `--nocapture` and
            // generalising; this notice was verified under plain `cargo test`.
            use std::io::Write;
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1687] SKIPPED the unstattable-part leg: could not make {} present-but-unstattable \
                 on this machine (no symlink privilege, an ACL-less filesystem, or running elevated). \
                 The remaining assertions do NOT cover CPE-1687 end to end.",
                part2.display()
            );
            return;
        };

        let err = join_files(&d.join("a.bin.001"), &out).unwrap_err();
        assert!(
            !err.contains("missing"),
            "a stat failure ({mechanism}) must not be reported as absence — the part is right there: {err}"
        );
        assert!(err.contains("part 2"), "the message must still say which part failed: {err}");
        assert!(
            err.contains(&part2.display().to_string()),
            "the message must still name the part's path: {err}"
        );
        assert!(!out.exists(), "no output should be left behind on failure");
        // `_restore` cleans up on the way out, panic or not.
    }

    #[test]
    fn part_stat_error_says_missing_only_for_a_genuine_absence() {
        // The deterministic half of the CPE-1687 guard: it runs on every OS and CI account, where the
        // end-to-end test above depends on a condition some machines cannot construct. Same reasoning as
        // `dispatch::classify_path_error`'s unit tests — permission bits are platform- and
        // privilege-dependent, the taxonomy is not.
        let p = Path::new("parts").join("a.bin.002");

        let absent = part_stat_error(2, &p, &std::io::Error::from(std::io::ErrorKind::NotFound));
        assert!(absent.contains("part 2 missing"), "a genuine absence is still 'missing': {absent}");

        // Every other stat outcome means "we do not know", and must name the OS's own cause instead.
        // `Other` stands in for the kinds Rust does not classify — a dead network mount typically
        // arrives as a raw OS error with no dedicated `ErrorKind`.
        for kind in [
            std::io::ErrorKind::PermissionDenied,
            std::io::ErrorKind::Other,
            std::io::ErrorKind::TimedOut,
        ] {
            let e = std::io::Error::from(kind);
            let msg = part_stat_error(2, &p, &e);
            assert!(!msg.contains("missing"), "{kind:?} must not be reported as absence: {msg}");
            assert!(msg.contains("part 2"), "{kind:?} must still say which part: {msg}");
            assert!(msg.contains(&e.to_string()), "{kind:?} must name the OS's own cause: {msg}");
        }
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
    fn hostile_manifest_part_size_overflow_is_rejected_not_a_panic() {
        // part_size = u64::MAX, part_count = 2 passes every earlier structural check (plain original_name,
        // 64-hex sha, part_count under cap) and used to overflow `part_size*(part_count-1)+1` → panic in
        // debug / silent wrap in release. Must now be a clean Err, and never panic.
        let manifest = SplitManifest {
            original_name: "f.bin".to_string(),
            total_size: 1,
            part_count: 2,
            part_size: u64::MAX,
            sha256: "a".repeat(64),
        };
        let err = validate_manifest(&manifest).unwrap_err();
        assert!(err.contains("overflow"), "overflow manifest must be rejected as corrupt: {err}");

        // And end-to-end through join_files (writing the hostile manifest), which must Err, not panic.
        let d = scratch("hostile-overflow");
        std::fs::write(
            d.join("f.bin.split-manifest.json"),
            serde_json::to_string(&manifest).unwrap(),
        )
        .unwrap();
        let out = d.join("out.bin");
        let e2 = join_files(&d.join("f.bin.split-manifest.json"), &out).unwrap_err();
        assert!(e2.contains("overflow"), "join must reject the overflow manifest: {e2}");
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
