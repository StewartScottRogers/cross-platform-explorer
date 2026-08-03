//! Disguised-file (extension-mismatch) **tree sweep** (CPE-1285, epic CPE-1000). Walks a real directory
//! tree (skip-unreadable, mirroring [`crate::folder_similarity_scan`]'s discipline), reads a **capped**
//! header per regular file, and calls [`crate::file_type::mismatch`] to flag every file whose sniffed
//! content disagrees with its claimed extension — the security-review complement to the per-row
//! `TypeMismatch` metadata column: a `.jpg` that's really a Windows PE, a `.pdf` that's really a bare
//! ZIP. Container formats (`.docx`/`.xlsx`/`.jar`/…) are never flagged; that safety already lives in
//! [`crate::file_type::mismatch`] via [`crate::file_type::FileType::extensions`], so this adapter simply
//! relies on it rather than re-implementing container awareness. Pure adapter + its own tests; no
//! `#[tauri::command]` here — that wiring is a separate ticket (CPE-1287).

use std::io::Read;
use std::path::Path;

use serde::Serialize;

use crate::file_type::mismatch;
use crate::fsutil::entry_is_symlink;
use crate::model::extension_of;

/// Capped header read per file, in bytes. 64 bytes comfortably covers every signature
/// [`crate::file_type::detect_type`] checks (the longest, SQLite's `"SQLite format 3\0"`, is 16 bytes;
/// the offset-based RIFF/`ftyp` checks look no further than byte 12), while staying far short of a full
/// read even for a huge file — this walk never reads whole files.
const HEADER_CAP: u64 = 64;

/// Cap on regular files scanned across the whole walk — mirrors
/// [`crate::folder_similarity_scan::FOLDER_MAX_FILES`]. Hitting it sets `truncated` and stops the walk
/// early rather than scanning an unbounded tree.
const MAX_FILES: u64 = 50_000;

/// One flagged content/extension disagreement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MismatchHit {
    /// The full path of the flagged file.
    pub path: String,
    /// The extension the file claims (lowercased, no leading dot).
    pub claimed_ext: String,
    /// Human-readable name of the type actually sniffed from the file's bytes (e.g. "Windows
    /// executable/library").
    pub detected_label: String,
    /// The canonical extension for the detected type (e.g. `"exe"` for a sniffed PE) — the first entry
    /// of [`crate::file_type::FileType::extensions`], which is always non-empty.
    pub detected_ext: String,
}

/// The result of a type-mismatch tree sweep: every flagged file, how many regular files were
/// considered, and whether [`MAX_FILES`] cut the walk short.
#[derive(Debug, Clone, Default, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct MismatchReport {
    pub hits: Vec<MismatchHit>,
    pub scanned: u64,
    pub truncated: bool,
}

/// Read at most [`HEADER_CAP`] bytes of `path`. Any I/O failure (missing file, no permission, a
/// directory that slipped through, a locked file, …) surfaces as `Err` so the caller skips the file
/// rather than panicking or aborting the whole sweep.
fn read_capped_header(path: &Path) -> std::io::Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(HEADER_CAP).read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Walk `root` (recursive, skip-unreadable directories, skip symlinked directories to avoid cycles —
/// same discipline as [`crate::folder_similarity_scan::find_similar_folders`]) and flag every regular
/// file whose sniffed content disagrees with its claimed extension via [`crate::file_type::mismatch`].
///
/// Never panics: a directory that can't be listed, or a file that can't be opened/read (permissions,
/// mid-scan deletion, a locked/quarantined file), is silently skipped rather than aborting the sweep. A
/// file with no extension, or whose bytes don't match a known signature, is never reported — see
/// [`crate::file_type::mismatch`]'s own contract. Caps at [`MAX_FILES`] regular files considered; hitting
/// the cap sets `truncated` and stops the walk early.
pub fn find_type_mismatches(root: &Path) -> MismatchReport {
    let mut hits = Vec::new();
    let mut scanned = 0u64;
    let mut truncated = false;
    let mut stack: Vec<std::path::PathBuf> = vec![root.to_path_buf()];

    'walk: while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                // Skip symlinked dirs to avoid walk cycles (same discipline as `folder_similarity_scan`
                // / `empty_dirs_scan` / `dangling_links_scan`).
                if !entry_is_symlink(&entry) {
                    stack.push(path);
                }
                continue;
            }
            if !meta.is_file() {
                continue;
            }
            if scanned >= MAX_FILES {
                truncated = true;
                break 'walk;
            }
            scanned += 1;

            let ext = extension_of(&path);
            let Ok(header) = read_capped_header(&path) else { continue };
            if let Some(m) = mismatch(&header, &ext) {
                hits.push(MismatchHit {
                    path: path.to_string_lossy().into_owned(),
                    claimed_ext: m.actual_ext,
                    detected_label: m.detected.label().to_string(),
                    detected_ext: m
                        .detected
                        .extensions()
                        .first()
                        .copied()
                        .unwrap_or_default()
                        .to_string(),
                });
            }
        }
    }

    MismatchReport { hits, scanned, truncated }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-typemismatch-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn pe_disguised_as_jpg_is_flagged_with_right_claimed_and_detected() {
        let d = scratch("pe-as-jpg");
        // A real PE/MZ header ("MZ" DOS stub) written into a file claiming to be a .jpg.
        fs::write(d.join("foo.jpg"), [0x4D, 0x5A, 0x90, 0x00, 0x03, 0x00]).unwrap();

        let r = find_type_mismatches(&d);
        assert!(!r.truncated);
        assert_eq!(r.scanned, 1);
        assert_eq!(r.hits.len(), 1, "the disguised PE must be flagged: {:?}", r.hits);
        let hit = &r.hits[0];
        assert!(hit.path.ends_with("foo.jpg"));
        assert_eq!(hit.claimed_ext, "jpg");
        assert_eq!(hit.detected_label, "Windows executable/library");
        assert_eq!(hit.detected_ext, "exe");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn genuine_png_named_png_is_not_flagged() {
        let d = scratch("real-png");
        let png_bytes = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0];
        fs::write(d.join("photo.png"), png_bytes).unwrap();

        let r = find_type_mismatches(&d);
        assert!(!r.truncated);
        assert_eq!(r.scanned, 1);
        assert!(r.hits.is_empty(), "a genuine PNG must not be flagged: {:?}", r.hits);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn real_zip_named_docx_is_not_flagged_container_safe() {
        let d = scratch("docx-zip");
        // ZIP local-file-header magic — a real .docx is a ZIP container under the hood.
        fs::write(d.join("report.docx"), [0x50, 0x4B, 0x03, 0x04, 0x14, 0x00]).unwrap();

        let r = find_type_mismatches(&d);
        assert!(r.hits.is_empty(), "a genuine ZIP-backed .docx must not be flagged: {:?}", r.hits);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn file_with_no_extension_is_not_flagged_even_when_content_is_a_detected_binary() {
        let d = scratch("no-ext");
        // Real PE bytes, but no extension at all — nothing to disagree with.
        fs::write(d.join("myapp"), [0x4D, 0x5A, 0x90, 0x00]).unwrap();

        let r = find_type_mismatches(&d);
        assert_eq!(r.scanned, 1);
        assert!(r.hits.is_empty(), "an extensionless file must never be flagged: {:?}", r.hits);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn unreadable_content_is_none_and_reports_no_mismatch() {
        // Plain text bytes claiming an unrelated extension: unknown content sniff → no verdict, not a
        // false positive.
        let d = scratch("plaintext");
        fs::write(d.join("notes.jpg"), b"just some plain text, not a binary format").unwrap();

        let r = find_type_mismatches(&d);
        assert!(r.hits.is_empty(), "undetectable content must not be flagged: {:?}", r.hits);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn nested_subdirectories_are_walked() {
        let d = scratch("nested");
        fs::create_dir_all(d.join("a/b")).unwrap();
        fs::write(d.join("a/b/hidden.png"), [0x4D, 0x5A, 0x90, 0x00]).unwrap();

        let r = find_type_mismatches(&d);
        assert_eq!(r.hits.len(), 1);
        assert!(r.hits[0].path.replace('\\', "/").ends_with("a/b/hidden.png"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn missing_root_scans_clean_without_panicking() {
        let missing = std::env::temp_dir().join("cpe-typemismatch-does-not-exist-at-all");
        let r = find_type_mismatches(&missing);
        assert!(r.hits.is_empty());
        assert_eq!(r.scanned, 0);
        assert!(!r.truncated);
    }

    #[cfg(windows)]
    #[test]
    fn locked_file_is_skipped_without_panicking() {
        // Open the file exclusively (no sharing) so a concurrent open from the scan fails with a sharing
        // violation — exercising the same `Err` path as an unreadable/permission-denied file, never a
        // panic. (Note for whoever reruns this: os error 225 elsewhere in this repo's test runs is
        // Windows Defender quarantining a binary test fixture, a different failure mode from this.)
        use std::os::windows::fs::OpenOptionsExt;

        let d = scratch("locked");
        let path = d.join("locked.jpg");
        fs::write(&path, [0x4D, 0x5A, 0x90, 0x00]).unwrap();
        let _handle = fs::OpenOptions::new()
            .read(true)
            .share_mode(0) // deny all sharing while held open
            .open(&path)
            .expect("exclusive open for the test fixture itself");

        let r = find_type_mismatches(&d);
        assert_eq!(r.scanned, 1, "the file is still counted as considered");
        assert!(r.hits.is_empty(), "an unreadable file must be skipped, not flagged or panicked on");
        drop(_handle);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn scanned_counts_every_regular_file_flagged_or_not() {
        let d = scratch("counts");
        fs::write(d.join("a.jpg"), [0x4D, 0x5A, 0x90, 0x00]).unwrap(); // flagged
        fs::write(d.join("b.png"), [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]).unwrap(); // ok
        fs::write(d.join("c.txt"), b"plain text").unwrap(); // undetectable

        let r = find_type_mismatches(&d);
        assert_eq!(r.scanned, 3);
        assert_eq!(r.hits.len(), 1);
        let _ = fs::remove_dir_all(&d);
    }
}
