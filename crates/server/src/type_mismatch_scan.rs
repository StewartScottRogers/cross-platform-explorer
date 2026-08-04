//! Disguised-file (extension-mismatch) **tree sweep** (CPE-1285, epic CPE-1000). Walks a real directory
//! tree (skip-unreadable, mirroring [`crate::folder_similarity_scan`]'s discipline), reads a **capped**
//! header per regular file, and calls [`crate::file_type::mismatch`] to flag every file whose sniffed
//! content disagrees with its claimed extension — the security-review complement to the per-row
//! `TypeMismatch` metadata column: a `.jpg` that's really a Windows PE, a `.pdf` that's really a bare
//! ZIP. Container formats (`.docx`/`.xlsx`/`.jar`/…) are never flagged; that safety already lives in
//! [`crate::file_type::mismatch`] via [`crate::file_type::FileType::extensions`], so this adapter simply
//! relies on it rather than re-implementing container awareness.
//!
//! CPE-1294: the DFS walk lives in [`walk_type_mismatches`], a shared flush-callback walker mirroring
//! [`crate::listing::stream_dir_entries`] — `flush` receives batches of [`MismatchHit`] as they're found
//! and can stop the walk early by returning `ControlFlow::Break`. [`find_type_mismatches`] is a thin
//! collect-to-vec wrapper over it, so its behavior is unchanged from before the refactor. A future
//! streaming command (CPE-1299) feeds `walk_type_mismatches` through an `ipc::Channel` in the app
//! adapter; that wiring is deliberately out of scope here. Pure adapter + its own tests; no
//! `#[tauri::command]` here.

use std::io::Read;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::file_type::mismatch;
use crate::fsutil::entry_is_symlink;
use crate::glob::any_glob_matches;
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

/// Number of flagged hits per streamed batch — small so a UI streaming this walk (CPE-1299) paints
/// progress steadily even on a tree where mismatches cluster in one directory. `walk_type_mismatches`
/// also flushes whatever's pending at the end of each directory, so a sparser tree still streams at a
/// directory-sized cadence rather than waiting for a full batch to fill.
pub const MISMATCH_SCAN_BATCH: usize = 64;

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

/// The tail of a [`walk_type_mismatches`] run: how many regular files were considered, and whether
/// [`MAX_FILES`] cut the walk short. Returned regardless of whether `flush` ever returned `Break` —
/// both fields reflect progress made up to the point the walk stopped.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScanTail {
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
/// same discipline as [`crate::folder_similarity_scan::find_similar_folders`]), flag every regular file
/// whose sniffed content disagrees with its claimed extension via [`crate::file_type::mismatch`], and
/// deliver flagged hits to `flush` in batches of up to [`MISMATCH_SCAN_BATCH`] (plus a flush of whatever
/// is pending at the end of each directory, so a sparse tree still streams steadily).
///
/// `flush` returns a `ControlFlow` so a streaming caller can stop the walk early at a batch boundary
/// (mirrors [`crate::listing::stream_dir_entries`]); returning `Break` stops the walk and the returned
/// [`ScanTail`] reflects progress made up to that point.
///
/// Never panics: a directory that can't be listed, or a file that can't be opened/read (permissions,
/// mid-scan deletion, a locked/quarantined file), is silently skipped rather than aborting the sweep. A
/// file with no extension, or whose bytes don't match a known signature, is never reported — see
/// [`crate::file_type::mismatch`]'s own contract. Caps at [`MAX_FILES`] regular files considered; hitting
/// the cap sets `truncated` and stops the walk early.
///
/// `excludes` are glob patterns (CPE-1302): any discovered sub-directory whose base name matches one is
/// pruned — skipped and never descended into, so nothing inside it is scanned or reported. An **empty**
/// `excludes` slice prunes nothing, so passing `&[]` is byte-identical to the pre-exclude behavior. The
/// `root` itself is always scanned regardless of `excludes`.
pub fn walk_type_mismatches(
    root: &Path,
    excludes: &[String],
    mut flush: impl FnMut(Vec<MismatchHit>) -> ControlFlow<()>,
) -> ScanTail {
    let mut scanned = 0u64;
    let mut truncated = false;
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    let mut buf: Vec<MismatchHit> = Vec::new();

    'walk: while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                // Skip symlinked dirs to avoid walk cycles (same discipline as `folder_similarity_scan`
                // / `empty_dirs_scan` / `dangling_links_scan`), and prune excluded dirs (CPE-1302).
                if !entry_is_symlink(&entry)
                    && !any_glob_matches(&entry.file_name().to_string_lossy(), excludes)
                {
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
                buf.push(MismatchHit {
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
                if buf.len() >= MISMATCH_SCAN_BATCH && flush(std::mem::take(&mut buf)).is_break() {
                    return ScanTail { scanned, truncated };
                }
            }
        }
        // End of this directory: flush whatever's pending so a streaming caller sees per-directory
        // progress rather than waiting for a full MISMATCH_SCAN_BATCH to accumulate.
        if !buf.is_empty() && flush(std::mem::take(&mut buf)).is_break() {
            return ScanTail { scanned, truncated };
        }
    }

    // The walk can end (cap hit mid-directory) with a partial batch never flushed above.
    if !buf.is_empty() {
        let _ = flush(buf);
    }

    ScanTail { scanned, truncated }
}

/// Collect-to-vec type-mismatch sweep: every flagged file under `root`, plus how many regular files
/// were considered and whether the walk was truncated. A thin wrapper over [`walk_type_mismatches`] —
/// behavior is unchanged from before the CPE-1294 refactor. `excludes` prunes matching sub-directories
/// (CPE-1302); pass `&[]` for the original whole-tree sweep.
pub fn find_type_mismatches(root: &Path, excludes: &[String]) -> MismatchReport {
    let mut hits = Vec::new();
    let tail = walk_type_mismatches(root, excludes, |batch| {
        hits.extend(batch);
        ControlFlow::Continue(())
    });
    MismatchReport { hits, scanned: tail.scanned, truncated: tail.truncated }
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

    /// Real PE/MZ header bytes — sniffs as a Windows executable regardless of the claimed extension.
    const PE_HEADER: [u8; 6] = [0x4D, 0x5A, 0x90, 0x00, 0x03, 0x00];

    #[test]
    fn pe_disguised_as_jpg_is_flagged_with_right_claimed_and_detected() {
        let d = scratch("pe-as-jpg");
        // A real PE/MZ header ("MZ" DOS stub) written into a file claiming to be a .jpg.
        fs::write(d.join("foo.jpg"), PE_HEADER).unwrap();

        let r = find_type_mismatches(&d, &[]);
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

        let r = find_type_mismatches(&d, &[]);
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

        let r = find_type_mismatches(&d, &[]);
        assert!(r.hits.is_empty(), "a genuine ZIP-backed .docx must not be flagged: {:?}", r.hits);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn file_with_no_extension_is_not_flagged_even_when_content_is_a_detected_binary() {
        let d = scratch("no-ext");
        // Real PE bytes, but no extension at all — nothing to disagree with.
        fs::write(d.join("myapp"), [0x4D, 0x5A, 0x90, 0x00]).unwrap();

        let r = find_type_mismatches(&d, &[]);
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

        let r = find_type_mismatches(&d, &[]);
        assert!(r.hits.is_empty(), "undetectable content must not be flagged: {:?}", r.hits);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn nested_subdirectories_are_walked() {
        let d = scratch("nested");
        fs::create_dir_all(d.join("a/b")).unwrap();
        fs::write(d.join("a/b/hidden.png"), [0x4D, 0x5A, 0x90, 0x00]).unwrap();

        let r = find_type_mismatches(&d, &[]);
        assert_eq!(r.hits.len(), 1);
        assert!(r.hits[0].path.replace('\\', "/").ends_with("a/b/hidden.png"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn missing_root_scans_clean_without_panicking() {
        let missing = std::env::temp_dir().join("cpe-typemismatch-does-not-exist-at-all");
        let r = find_type_mismatches(&missing, &[]);
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

        let r = find_type_mismatches(&d, &[]);
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

        let r = find_type_mismatches(&d, &[]);
        assert_eq!(r.scanned, 3);
        assert_eq!(r.hits.len(), 1);
        let _ = fs::remove_dir_all(&d);
    }

    // --- CPE-1294: walk_type_mismatches streaming-walker tests, mirroring listing.rs's
    // stream_dir_entries_* coverage. ---

    #[test]
    fn walk_type_mismatches_batches_and_flushes_all() {
        let d = scratch("walk-batches");
        // More than MISMATCH_SCAN_BATCH flagged hits in one flat directory, so both the mid-directory
        // batch-size flush and the end-of-directory leftover flush are exercised.
        let n_flagged = MISMATCH_SCAN_BATCH * 2 + 10;
        for i in 0..n_flagged {
            fs::write(d.join(format!("f{i:04}.jpg")), PE_HEADER).unwrap();
        }

        let mut batch_sizes = Vec::new();
        let mut total_hits = 0usize;
        let tail = walk_type_mismatches(&d, &[], |b| {
            batch_sizes.push(b.len());
            total_hits += b.len();
            ControlFlow::Continue(())
        });

        assert!(!tail.truncated);
        assert_eq!(tail.scanned, n_flagged as u64);
        assert_eq!(total_hits, n_flagged);
        assert!(batch_sizes.len() >= 2, "more than one batch worth of hits should flush more than once");
        assert!(
            batch_sizes.iter().all(|&s| s <= MISMATCH_SCAN_BATCH),
            "no batch exceeds MISMATCH_SCAN_BATCH: {batch_sizes:?}"
        );
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn walk_type_mismatches_stops_on_break() {
        let d = scratch("walk-break");
        let n_flagged = MISMATCH_SCAN_BATCH * 3;
        for i in 0..n_flagged {
            fs::write(d.join(format!("f{i:04}.jpg")), PE_HEADER).unwrap();
        }

        let mut seen = 0usize;
        // Break after the first flush — the walk must stop rather than deliver every hit.
        let tail = walk_type_mismatches(&d, &[], |b| {
            seen += b.len();
            ControlFlow::Break(())
        });

        assert_eq!(seen, MISMATCH_SCAN_BATCH, "break after the first batch stops the walk");
        assert!(
            tail.scanned < n_flagged as u64,
            "the walk must not have considered every file after an early break: scanned={}",
            tail.scanned
        );
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn walk_type_mismatches_flushes_pending_hits_per_directory_even_below_batch_size() {
        // Two nested directories, each with a single flagged hit — far fewer than
        // MISMATCH_SCAN_BATCH. The per-directory flush must still deliver each one rather than only
        // flushing at the very end of the whole walk.
        let d = scratch("walk-per-dir");
        fs::create_dir_all(d.join("sub")).unwrap();
        fs::write(d.join("top.jpg"), PE_HEADER).unwrap();
        fs::write(d.join("sub/nested.jpg"), PE_HEADER).unwrap();

        let mut flush_count = 0usize;
        let mut total = 0usize;
        let tail = walk_type_mismatches(&d, &[], |b| {
            flush_count += 1;
            total += b.len();
            ControlFlow::Continue(())
        });

        assert_eq!(total, 2);
        assert_eq!(tail.scanned, 2);
        assert!(flush_count >= 2, "each directory's lone hit should flush separately: {flush_count}");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn find_type_mismatches_equals_collect_to_vec_over_walk_type_mismatches() {
        let d = scratch("collect-eq");
        fs::create_dir_all(d.join("a/b")).unwrap();
        fs::write(d.join("top.jpg"), PE_HEADER).unwrap();
        fs::write(d.join("a/mid.png"), PE_HEADER).unwrap();
        fs::write(d.join("a/b/deep.docx"), [0x50, 0x4B, 0x03, 0x04, 0x14, 0x00]).unwrap(); // container-safe
        fs::write(d.join("a/b/plain.txt"), b"plain text").unwrap(); // undetectable
        fs::write(d.join("a/real.png"), [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]).unwrap(); // genuine

        let mut collected = Vec::new();
        let tail = walk_type_mismatches(&d, &[], |b| {
            collected.extend(b);
            ControlFlow::Continue(())
        });

        let report = find_type_mismatches(&d, &[]);

        assert_eq!(report.scanned, tail.scanned);
        assert_eq!(report.truncated, tail.truncated);

        // DFS order is deterministic given a fixed tree, but sort both to compare contents robustly
        // (equality of the collect-to-vec wrapper's output to the raw walk, not incidental ordering).
        let mut a: Vec<_> = collected.iter().map(|h| h.path.clone()).collect();
        let mut b: Vec<_> = report.hits.iter().map(|h| h.path.clone()).collect();
        a.sort();
        b.sort();
        assert_eq!(a, b);
        assert_eq!(report.hits, collected, "find_type_mismatches must be byte-identical to the raw walk");
        let _ = fs::remove_dir_all(&d);
    }

    // --- CPE-1302: exclude-glob support. ---

    #[test]
    fn excluded_directory_is_pruned_and_its_disguised_file_is_not_reported() {
        let d = scratch("exclude-prune");
        fs::create_dir_all(d.join("node_modules/pkg")).unwrap();
        fs::create_dir_all(d.join("src")).unwrap();
        // A disguised PE hides inside node_modules (deep) and another in src.
        fs::write(d.join("node_modules/pkg/evil.jpg"), PE_HEADER).unwrap();
        fs::write(d.join("src/evil.jpg"), PE_HEADER).unwrap();
        fs::write(d.join("top.jpg"), PE_HEADER).unwrap();

        // Baseline: with empty excludes, all three disguised files are flagged.
        let all = find_type_mismatches(&d, &[]);
        assert_eq!(all.hits.len(), 3, "empty excludes reports every disguised file: {:?}", all.hits);

        // Excluding node_modules prunes it: its file is neither scanned nor reported, and the dir
        // itself is never descended into (scanned drops by exactly the one file inside it).
        let excl = find_type_mismatches(&d, &["node_modules".to_string()]);
        assert_eq!(excl.hits.len(), 2, "the disguised file inside node_modules must not be reported: {:?}", excl.hits);
        assert!(!excl.hits.iter().any(|h| h.path.replace('\\', "/").contains("node_modules")));
        assert_eq!(excl.scanned, all.scanned - 1, "the pruned dir's file is not scanned");
        assert!(excl.hits.iter().any(|h| h.path.replace('\\', "/").ends_with("src/evil.jpg")));
        assert!(excl.hits.iter().any(|h| h.path.ends_with("top.jpg")));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn empty_excludes_equals_original_behavior() {
        let d = scratch("exclude-empty-parity");
        fs::create_dir_all(d.join("a/b")).unwrap();
        fs::write(d.join("a/b/hidden.png"), [0x4D, 0x5A, 0x90, 0x00]).unwrap();
        fs::write(d.join("real.png"), [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]).unwrap();

        // Passing an empty excludes slice must be identical to the no-exclude path.
        let a = find_type_mismatches(&d, &[]);
        let no_dirs_excluded = find_type_mismatches(&d, &["does_not_exist_anywhere".to_string()]);
        assert_eq!(a.hits, no_dirs_excluded.hits, "a non-matching exclude changes nothing");
        assert_eq!(a.scanned, no_dirs_excluded.scanned);
        let _ = fs::remove_dir_all(&d);
    }
}
