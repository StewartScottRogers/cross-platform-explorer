//! Orphaned-sidecar scan pipeline (CPE-1283, epic CPE-1002 "File inspection & safety utilities").
//! Wires the pure [`crate::orphan_sidecars`] core (`FileEntry` + `default_rules` + `find_orphans`)
//! to a real directory walk: list a folder's files (optionally recursive; skip-unreadable, mirroring
//! `list_dir`), group them **per directory** (a sidecar and its primary only ever pair within the
//! same folder — `find_orphans` assumes its whole input slice lives in one directory, per its own
//! docs), and report every orphaned sidecar's full path.
//!
//! ## Scope
//!
//! Symlinked/dot directories are not specially excluded here (unlike `folder_similarity_scan`) —
//! sidecar detection has no cycle risk since it never recurses into a directory twice, but a caller
//! walking a tree with a real symlink cycle would still loop forever if `recursive` is set on such a
//! tree; that's an existing risk shared by `list_dir` itself and out of scope for this ticket.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::glob::any_glob_matches;
use crate::orphan_sidecars::{default_rules, find_orphans, FileEntry};

/// Cap on files scanned across the whole walk — mirrors the caps in `duplicates` /
/// `folder_similarity_scan`. Hitting it sets `truncated` and stops the walk early.
const ORPHAN_MAX_FILES: u64 = 50_000;

/// The result of an orphaned-sidecar scan: the orphaned sidecar files' full paths, how many files
/// were scanned, and whether the file cap truncated the walk.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct OrphanSidecarResult {
    pub orphans: Vec<String>,
    pub scanned: u64,
    pub truncated: bool,
}

/// The tail of a [`walk_orphan_sidecars`] walk: how many files were scanned and whether the file cap
/// truncated it early. Mirrors the non-`orphans` fields of [`OrphanSidecarResult`].
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ScanTail {
    pub scanned: u64,
    pub truncated: bool,
}

/// Walk `root` for orphaned sidecar files using [`default_rules`], flushing each directory's orphan
/// batch to `flush` as soon as it's computed — a directory's files are gathered, `find_orphans` runs
/// once per directory (it assumes its whole input lives in one directory), and that directory's
/// orphaned paths are the natural streaming boundary. When `recursive` is `false`, only `root`'s direct
/// file children are considered; when `true`, every subdirectory is walked too. Directories/files that
/// can't be read are skipped rather than failing the whole scan (skip-on-error, like `list_dir`). A
/// `root` that doesn't exist or isn't readable yields an empty, non-truncated tail without ever calling
/// `flush` — this function never panics.
///
/// `flush` returns a `ControlFlow` so a streaming caller can stop the walk early (cancellation) at a
/// per-directory batch boundary; a non-empty batch that returns `Break` is still delivered to `flush`
/// before the walk stops.
///
/// `excludes` are glob patterns (CPE-1302): when `recursive`, any discovered sub-directory whose base
/// name matches one is pruned — skipped and never descended into, so no sidecar inside it is scanned or
/// reported. An **empty** `excludes` slice prunes nothing, so passing `&[]` is byte-identical to the
/// pre-exclude behavior. The `root` itself is always scanned regardless of `excludes`.
pub fn walk_orphan_sidecars(
    root: &Path,
    recursive: bool,
    excludes: &[String],
    mut flush: impl FnMut(Vec<String>) -> std::ops::ControlFlow<()>,
) -> ScanTail {
    let rules = default_rules();
    let mut scanned = 0u64;
    let mut truncated = false;
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];

    'walk: while let Some(dir) = stack.pop() {
        let Ok(read_dir) = std::fs::read_dir(&dir) else { continue };
        let mut dir_entries: Vec<FileEntry> = Vec::new();
        for entry in read_dir.flatten() {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                if recursive && !any_glob_matches(&entry.file_name().to_string_lossy(), excludes) {
                    stack.push(path);
                }
                continue;
            }
            if !meta.is_file() {
                continue;
            }
            if scanned >= ORPHAN_MAX_FILES {
                truncated = true;
                break 'walk;
            }
            scanned += 1;
            let name = entry.file_name().to_string_lossy().into_owned();
            let (stem, ext) = split_stem_ext(&name);
            dir_entries.push(FileEntry::new(name, stem, ext));
        }
        let batch: Vec<String> = find_orphans(&dir_entries, &rules)
            .into_iter()
            .map(|name| dir.join(&name).to_string_lossy().into_owned())
            .collect();
        if !batch.is_empty() && flush(batch).is_break() {
            break 'walk;
        }
    }

    ScanTail { scanned, truncated }
}

/// Find orphaned sidecar files under `root` using [`default_rules`]. When `recursive` is `false`,
/// only `root`'s direct file children are considered; when `true`, every subdirectory is walked too.
/// Directories/files that can't be read are skipped rather than failing the whole scan (skip-on-error,
/// like `list_dir`). A `root` that doesn't exist or isn't readable yields an empty, non-truncated
/// result rather than an error — this function never panics. `excludes` prunes matching sub-directories
/// when `recursive` (CPE-1302); pass `&[]` for the original whole-tree scan.
pub fn find_orphan_sidecars(root: &Path, recursive: bool, excludes: &[String]) -> OrphanSidecarResult {
    let mut orphans: Vec<String> = Vec::new();
    let tail = walk_orphan_sidecars(root, recursive, excludes, |batch| {
        orphans.extend(batch);
        std::ops::ControlFlow::Continue(())
    });
    OrphanSidecarResult { orphans, scanned: tail.scanned, truncated: tail.truncated }
}

/// Split a file name into its `(stem, lowercased-extension-without-dot)`, matching the contract
/// documented on [`FileEntry`]. A name with no extension (or a leading-dot-only dotfile like
/// `.gitignore`) gets an empty extension and the whole name as its stem.
fn split_stem_ext(name: &str) -> (String, String) {
    let p = Path::new(name);
    let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let stem = if ext.is_empty() {
        name.to_string()
    } else {
        p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| name.to_string())
    };
    (stem, ext)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(tag: &str) -> crate::fsutil::ScratchDir {
        crate::fsutil::scratch_dir(&format!("cpe-orphansidecar-{tag}"))
    }

    fn norm(paths: &[String]) -> Vec<String> {
        let mut v: Vec<String> = paths.iter().map(|p| p.replace('\\', "/")).collect();
        v.sort();
        v
    }

    #[test]
    fn srt_with_present_mp4_primary_is_not_flagged() {
        let d = scratch("srt-ok");
        fs::write(d.join("movie.mp4"), "video bytes").unwrap();
        fs::write(d.join("movie.srt"), "1\n00:00:00,000 --> 00:00:01,000\nhi\n").unwrap();

        let r = find_orphan_sidecars(&d, false, &[]);
        assert!(r.orphans.is_empty(), "srt with a present mp4 primary must not be reported: {:?}", r.orphans);
        assert_eq!(r.scanned, 2);
        assert!(!r.truncated);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn xmp_with_absent_primary_is_flagged() {
        let d = scratch("xmp-orphan");
        fs::write(d.join("orphan.xmp"), "<x:xmpmeta/>").unwrap();

        let r = find_orphan_sidecars(&d, false, &[]);
        let names = norm(&r.orphans);
        assert_eq!(names.len(), 1);
        assert!(names[0].ends_with("orphan.xmp"));
        assert_eq!(r.scanned, 1);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn xmp_with_present_jpg_primary_is_not_flagged() {
        let d = scratch("xmp-ok");
        fs::write(d.join("photo.jpg"), "jpeg bytes").unwrap();
        fs::write(d.join("photo.xmp"), "<x:xmpmeta/>").unwrap();

        let r = find_orphan_sidecars(&d, false, &[]);
        assert!(r.orphans.is_empty(), "xmp with a present jpg primary must not be reported: {:?}", r.orphans);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn non_recursive_ignores_subdirectory_files() {
        let d = scratch("nonrecursive");
        fs::create_dir_all(d.join("sub")).unwrap();
        fs::write(d.join("top.xmp"), "orphan at top").unwrap();
        fs::write(d.join("sub/nested.xmp"), "orphan nested").unwrap();

        let r = find_orphan_sidecars(&d, false, &[]);
        let names = norm(&r.orphans);
        assert_eq!(names.len(), 1, "only the top-level orphan is scanned: {names:?}");
        assert!(names[0].ends_with("top.xmp"));
        assert_eq!(r.scanned, 1);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn recursive_finds_orphans_in_subdirectories() {
        let d = scratch("recursive");
        fs::create_dir_all(d.join("sub")).unwrap();
        fs::write(d.join("top.xmp"), "orphan at top").unwrap();
        fs::write(d.join("sub/nested.xmp"), "orphan nested").unwrap();

        let r = find_orphan_sidecars(&d, true, &[]);
        let names = norm(&r.orphans);
        assert_eq!(names.len(), 2, "both orphans found recursively: {names:?}");
        assert!(names.iter().any(|p| p.ends_with("top.xmp")));
        assert!(names.iter().any(|p| p.ends_with("sub/nested.xmp")));
        assert_eq!(r.scanned, 2);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn recursive_does_not_pair_a_sidecar_with_a_primary_in_a_different_directory() {
        // `movie.mp4` lives in a sibling directory from `movie.srt` — even though the stems match,
        // find_orphans only ever compares entries gathered from the SAME directory, so the srt must
        // still be reported as orphaned.
        let d = scratch("cross-dir");
        fs::create_dir_all(d.join("videos")).unwrap();
        fs::create_dir_all(d.join("subs")).unwrap();
        fs::write(d.join("videos/movie.mp4"), "video bytes").unwrap();
        fs::write(d.join("subs/movie.srt"), "subtitle text").unwrap();

        let r = find_orphan_sidecars(&d, true, &[]);
        let names = norm(&r.orphans);
        assert_eq!(names.len(), 1, "the srt is still orphaned despite the same-stem mp4 next door: {names:?}");
        assert!(names[0].ends_with("subs/movie.srt"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn unreadable_root_yields_empty_non_truncated_result_not_a_panic() {
        let d = scratch("missing-parent");
        let missing = d.join("does-not-exist");
        let r = find_orphan_sidecars(&missing, true, &[]);
        assert!(r.orphans.is_empty());
        assert_eq!(r.scanned, 0);
        assert!(!r.truncated);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn non_sidecar_files_are_never_reported() {
        let d = scratch("plain");
        fs::write(d.join("readme.txt"), "hello").unwrap();
        let r = find_orphan_sidecars(&d, false, &[]);
        assert!(r.orphans.is_empty());
        assert_eq!(r.scanned, 1);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn walk_flushes_one_batch_per_directory() {
        // Two subdirectories, each with one orphan — the walker must flush them as separate batches
        // (the per-directory grouping is the natural streaming boundary), not one combined batch.
        let d = scratch("walk-batches");
        fs::create_dir_all(d.join("a")).unwrap();
        fs::create_dir_all(d.join("b")).unwrap();
        fs::write(d.join("a/one.xmp"), "orphan a").unwrap();
        fs::write(d.join("b/two.xmp"), "orphan b").unwrap();

        let mut batches: Vec<Vec<String>> = Vec::new();
        let tail = walk_orphan_sidecars(&d, true, &[], |batch| {
            batches.push(batch);
            std::ops::ControlFlow::Continue(())
        });

        assert_eq!(batches.len(), 2, "each directory's orphans flush as their own batch: {batches:?}");
        assert!(batches.iter().all(|b| b.len() == 1));
        let mut all: Vec<String> = batches.into_iter().flatten().collect();
        all = norm(&all);
        assert_eq!(all.len(), 2);
        assert!(all.iter().any(|p| p.ends_with("a/one.xmp")));
        assert!(all.iter().any(|p| p.ends_with("b/two.xmp")));
        assert_eq!(tail.scanned, 2);
        assert!(!tail.truncated);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn walk_never_flushes_an_empty_batch() {
        // A directory with no orphans (e.g. a plain file, or a sidecar whose primary is present) must
        // not trigger a flush call at all.
        let d = scratch("walk-empty-batch");
        fs::write(d.join("readme.txt"), "hello").unwrap();

        let mut flush_calls = 0usize;
        let tail = walk_orphan_sidecars(&d, false, &[], |_batch| {
            flush_calls += 1;
            std::ops::ControlFlow::Continue(())
        });

        assert_eq!(flush_calls, 0, "no orphans in this directory means no flush call");
        assert_eq!(tail.scanned, 1);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn walk_stops_early_on_break() {
        // Three directories each with one orphan; breaking on the first flushed batch must stop the
        // walk rather than visiting the remaining directories.
        let d = scratch("walk-break");
        for sub in ["a", "b", "c"] {
            fs::create_dir_all(d.join(sub)).unwrap();
            fs::write(d.join(sub).join("orphan.xmp"), "x").unwrap();
        }

        let mut batches_seen = 0usize;
        let tail = walk_orphan_sidecars(&d, true, &[], |batch| {
            batches_seen += 1;
            assert_eq!(batch.len(), 1);
            std::ops::ControlFlow::Break(())
        });

        assert_eq!(batches_seen, 1, "the walk stops after the first flushed batch is Broken");
        assert_eq!(tail.scanned, 1, "only the one directory that produced the first batch was scanned");
        assert!(!tail.truncated);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn find_orphan_sidecars_matches_a_manual_walk_collection() {
        // The collect-to-vec entry point must be byte-identical to driving the walker by hand.
        let d = scratch("collect-parity");
        fs::create_dir_all(d.join("videos")).unwrap();
        fs::create_dir_all(d.join("subs")).unwrap();
        fs::write(d.join("videos/movie.mp4"), "video bytes").unwrap();
        fs::write(d.join("subs/movie.srt"), "subtitle text").unwrap();
        fs::write(d.join("top.xmp"), "orphan at top").unwrap();

        let mut manual: Vec<String> = Vec::new();
        let manual_tail = walk_orphan_sidecars(&d, true, &[], |batch| {
            manual.extend(batch);
            std::ops::ControlFlow::Continue(())
        });

        let via_collect = find_orphan_sidecars(&d, true, &[]);

        assert_eq!(norm(&manual), norm(&via_collect.orphans));
        assert_eq!(manual_tail.scanned, via_collect.scanned);
        assert_eq!(manual_tail.truncated, via_collect.truncated);
        let _ = fs::remove_dir_all(&d);
    }

    // --- CPE-1302: exclude-glob support. ---

    #[test]
    fn excluded_directory_is_pruned_and_its_orphan_is_not_reported() {
        let d = scratch("exclude-prune");
        fs::create_dir_all(d.join("node_modules")).unwrap();
        fs::create_dir_all(d.join("subs")).unwrap();
        // An orphaned sidecar (no primary) hides inside node_modules, another inside subs.
        fs::write(d.join("node_modules/orphan_inner.xmp"), "<x:xmpmeta/>").unwrap();
        fs::write(d.join("subs/orphan_kept.xmp"), "<x:xmpmeta/>").unwrap();

        // Baseline: empty excludes reports both orphans recursively.
        let all = find_orphan_sidecars(&d, true, &[]);
        assert_eq!(all.orphans.len(), 2, "empty excludes reports every orphan: {:?}", all.orphans);

        // Excluding node_modules prunes it: its orphan is neither scanned nor reported.
        let excl = find_orphan_sidecars(&d, true, &["node_modules".to_string()]);
        let names = norm(&excl.orphans);
        assert_eq!(names.len(), 1, "the orphan inside node_modules must not be reported: {names:?}");
        assert!(names[0].ends_with("subs/orphan_kept.xmp"));
        assert!(!names.iter().any(|p| p.contains("node_modules")));
        assert_eq!(excl.scanned, all.scanned - 1, "the pruned dir's file is not scanned");
        let _ = fs::remove_dir_all(&d);
    }
}
