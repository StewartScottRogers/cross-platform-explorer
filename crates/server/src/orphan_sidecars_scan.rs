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

/// Find orphaned sidecar files under `root` using [`default_rules`]. When `recursive` is `false`,
/// only `root`'s direct file children are considered; when `true`, every subdirectory is walked too.
/// Directories/files that can't be read are skipped rather than failing the whole scan (skip-on-error,
/// like `list_dir`). A `root` that doesn't exist or isn't readable yields an empty, non-truncated
/// result rather than an error — this function never panics.
pub fn find_orphan_sidecars(root: &Path, recursive: bool) -> OrphanSidecarResult {
    let rules = default_rules();
    let mut orphans: Vec<String> = Vec::new();
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
                if recursive {
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
        for name in find_orphans(&dir_entries, &rules) {
            orphans.push(dir.join(&name).to_string_lossy().into_owned());
        }
    }

    OrphanSidecarResult { orphans, scanned, truncated }
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

    fn scratch(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-orphansidecar-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
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

        let r = find_orphan_sidecars(&d, false);
        assert!(r.orphans.is_empty(), "srt with a present mp4 primary must not be reported: {:?}", r.orphans);
        assert_eq!(r.scanned, 2);
        assert!(!r.truncated);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn xmp_with_absent_primary_is_flagged() {
        let d = scratch("xmp-orphan");
        fs::write(d.join("orphan.xmp"), "<x:xmpmeta/>").unwrap();

        let r = find_orphan_sidecars(&d, false);
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

        let r = find_orphan_sidecars(&d, false);
        assert!(r.orphans.is_empty(), "xmp with a present jpg primary must not be reported: {:?}", r.orphans);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn non_recursive_ignores_subdirectory_files() {
        let d = scratch("nonrecursive");
        fs::create_dir_all(d.join("sub")).unwrap();
        fs::write(d.join("top.xmp"), "orphan at top").unwrap();
        fs::write(d.join("sub/nested.xmp"), "orphan nested").unwrap();

        let r = find_orphan_sidecars(&d, false);
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

        let r = find_orphan_sidecars(&d, true);
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

        let r = find_orphan_sidecars(&d, true);
        let names = norm(&r.orphans);
        assert_eq!(names.len(), 1, "the srt is still orphaned despite the same-stem mp4 next door: {names:?}");
        assert!(names[0].ends_with("subs/movie.srt"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn unreadable_root_yields_empty_non_truncated_result_not_a_panic() {
        let d = scratch("missing-parent");
        let missing = d.join("does-not-exist");
        let r = find_orphan_sidecars(&missing, true);
        assert!(r.orphans.is_empty());
        assert_eq!(r.scanned, 0);
        assert!(!r.truncated);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn non_sidecar_files_are_never_reported() {
        let d = scratch("plain");
        fs::write(d.join("readme.txt"), "hello").unwrap();
        let r = find_orphan_sidecars(&d, false);
        assert!(r.orphans.is_empty());
        assert_eq!(r.scanned, 1);
        let _ = fs::remove_dir_all(&d);
    }
}
