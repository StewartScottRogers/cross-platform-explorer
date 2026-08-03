//! Filesystem-walk adapter for the pure `empty_dirs::cascade_empty` core (CPE-1282, epic
//! CPE-1002). Recursively walks a real directory tree into an `empty_dirs::DirNode` tree and
//! calls `cascade_empty` to report the topmost **cascade-empty** directories — a directory that
//! is empty or contains only (nested) empty directories, so removing it removes the whole
//! cascade. Skips subdirectories it can't read (the walk continues rather than failing), caps
//! how many directories it visits, and never panics. Command/UI wiring is a separate ticket
//! (CPE-1287); this module is a pure adapter plus its own tests.

use std::path::Path;

use serde::Serialize;

use crate::empty_dirs::{cascade_empty, DirNode};
use crate::fsutil::entry_is_symlink;

/// Cap on directories visited during the walk — bounds a runaway tree, mirroring
/// [`crate::folder_similarity_scan::FOLDER_MAX_DIRS`]. Hitting it sets `truncated`; directories
/// beyond the cap are simply not descended into (their parent is judged on what was seen).
const EMPTY_DIRS_MAX: u64 = 20_000;

/// The result of an empty-folder cascade scan: the topmost cascade-empty directory paths, how
/// many directories were visited, and whether the walk cap was hit.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct EmptyDirsReport {
    /// Topmost cascade-empty directory paths, in deterministic pre-order (see
    /// [`crate::empty_dirs::cascade_empty`]).
    pub dirs: Vec<String>,
    /// How many directories were successfully read during the walk.
    pub scanned: u64,
    /// Set when [`EMPTY_DIRS_MAX`] was hit — the walk stopped early, so `dirs` may be incomplete.
    pub truncated: bool,
}

/// Find the topmost cascade-empty directories under `root` (a real directory on disk).
///
/// Skips subdirectories it can't read rather than failing the whole scan — an unreadable child
/// is simply excluded from its parent's tree, so a parent whose *readable* children are all
/// empty can still be reported (matching the `list_dir` skip-unreadable convention). Skips
/// symlinked subdirectories to avoid cycles. Caps the walk at [`EMPTY_DIRS_MAX`] visited
/// directories. Never panics — an unreadable/non-existent `root` yields an empty, untruncated
/// report rather than an error.
pub fn find_empty_dirs(root: &Path) -> EmptyDirsReport {
    let mut scanned = 0u64;
    let mut truncated = false;
    let dirs = match build_node(root, &mut scanned, &mut truncated) {
        Some(node) => cascade_empty(&node),
        None => Vec::new(),
    };
    EmptyDirsReport { dirs, scanned, truncated }
}

/// Recursively build a [`DirNode`] for `path`. Returns `None` if `path` can't be read (skipped,
/// not an error) or the walk cap has already been hit — either way the caller (the parent call)
/// simply omits this entry from its own children rather than propagating a failure.
fn build_node(path: &Path, scanned: &mut u64, truncated: &mut bool) -> Option<DirNode> {
    if *scanned >= EMPTY_DIRS_MAX {
        *truncated = true;
        return None;
    }
    let entries = std::fs::read_dir(path).ok()?;
    *scanned += 1;

    let mut file_count = 0usize;
    let mut children = Vec::new();
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            // Skip symlinked dirs to avoid cycles (same discipline as `folder_similarity_scan`).
            if entry_is_symlink(&entry) {
                continue;
            }
            if let Some(child) = build_node(&entry.path(), scanned, truncated) {
                children.push(child);
            }
        } else if meta.is_file() {
            file_count += 1;
        }
    }

    Some(DirNode { path: path.to_string_lossy().into_owned(), file_count, children })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-emptydirs-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn nested_empty_dirs_collapse_to_common_ancestor() {
        // b/c and b/d are both empty leaves; their parent b has no files of its own either, so
        // the whole branch is cascade-empty and only the topmost `b` should be reported. A
        // sibling `keep/` with a real file keeps `root` itself out of the report, so this
        // exercises collapsing to an ancestor short of the root.
        let root = scratch("nested");
        fs::create_dir_all(root.join("b/c")).unwrap();
        fs::create_dir_all(root.join("b/d")).unwrap();
        fs::create_dir_all(root.join("keep")).unwrap();
        fs::write(root.join("keep/file.txt"), "not empty").unwrap();

        let report = find_empty_dirs(&root);
        assert!(!report.truncated);
        assert_eq!(report.dirs.len(), 1, "expected exactly one topmost cascade-empty dir: {:?}", report.dirs);
        let reported = Path::new(&report.dirs[0]);
        assert_eq!(reported, root.join("b"), "topmost ancestor `b` reported, not its empty children");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn branch_with_a_real_file_is_not_flagged_nor_are_its_ancestors_on_its_account() {
        // g/h contains a real file, so g/h is not cascade-empty, and neither is its parent `g`
        // (which otherwise has no files of its own) — an unrelated sibling `e` stays reported.
        let root = scratch("hasfile");
        fs::create_dir_all(root.join("g/h")).unwrap();
        fs::write(root.join("g/h/keep.txt"), "not empty").unwrap();
        fs::create_dir_all(root.join("e")).unwrap();

        let report = find_empty_dirs(&root);
        assert!(!report.truncated);
        let reported: Vec<_> = report.dirs.iter().map(Path::new).collect();
        assert!(!reported.iter().any(|p| p.starts_with(root.join("g"))), "g/h and g must not be flagged: {:?}", report.dirs);
        assert!(reported.contains(&root.join("e").as_path()), "unrelated empty sibling `e` still reported: {:?}", report.dirs);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn fully_empty_tree_reports_the_root() {
        let root = scratch("fullyempty");
        fs::create_dir_all(root.join("x/y/z")).unwrap();

        let report = find_empty_dirs(&root);
        assert!(!report.truncated);
        assert_eq!(report.dirs, vec![root.to_string_lossy().into_owned()]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn scanned_counts_readable_directories_visited() {
        let root = scratch("scanned");
        fs::create_dir_all(root.join("a")).unwrap();
        fs::create_dir_all(root.join("b")).unwrap();

        let report = find_empty_dirs(&root);
        // root + a + b = 3 directories read.
        assert_eq!(report.scanned, 3);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn non_existent_root_yields_empty_untruncated_report_not_a_panic() {
        let missing = std::env::temp_dir().join("cpe-emptydirs-does-not-exist-at-all");
        let report = find_empty_dirs(&missing);
        assert!(report.dirs.is_empty());
        assert_eq!(report.scanned, 0);
        assert!(!report.truncated);
    }

    #[test]
    fn symlinked_subdirs_are_skipped() {
        // `keep/` with a real file keeps `root` out of the report so `real_empty` must be
        // reported on its own merits, independent of whether the symlink could be created (may
        // need privileges on Windows) — either way nothing panics and no cycle is followed.
        let root = scratch("symlink");
        fs::create_dir_all(root.join("real_empty")).unwrap();
        fs::create_dir_all(root.join("target")).unwrap();
        fs::create_dir_all(root.join("keep")).unwrap();
        fs::write(root.join("keep/file.txt"), "not empty").unwrap();
        #[cfg(unix)]
        {
            let _ = std::os::unix::fs::symlink(root.join("target"), root.join("link"));
        }
        #[cfg(windows)]
        {
            let _ = std::os::windows::fs::symlink_dir(root.join("target"), root.join("link"));
        }

        let report = find_empty_dirs(&root);
        assert!(report.dirs.iter().any(|p| Path::new(p) == root.join("real_empty")));
        let _ = fs::remove_dir_all(&root);
    }
}
