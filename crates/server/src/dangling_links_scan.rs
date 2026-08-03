//! Dangling / cyclic symlink **scan** pipeline (CPE-1284, epic CPE-1002). Walks a folder tree
//! (skip-unreadable, mirroring [`crate::folder_similarity_scan`]'s discipline), finds symlinks via
//! [`crate::fsutil::entry_is_symlink`] + [`std::fs::read_link`], resolves each one's target
//! (relative targets joined against the link's own parent directory, then **lexically**
//! normalised — never `canonicalize`, so a self/loop symlink can't make target resolution error or
//! hang), records whether the resolved target currently exists, and hands the whole set of
//! [`crate::dangling_links::LinkEntry`] records to the pure [`crate::dangling_links::scan_dangling`]
//! classifier. This module's only job is building well-formed `LinkEntry`s from what's on disk — all
//! Missing/Cyclic classification logic lives in [`crate::dangling_links`], same division of labour as
//! [`crate::folder_similarity_scan`] over [`crate::folder_similarity`].

use std::path::{Component, Path, PathBuf};

use serde::Serialize;

use crate::dangling_links::{scan_dangling, DanglingLink, LinkEntry};
use crate::fsutil::entry_is_symlink;

/// Cap on filesystem entries visited across the whole walk — bounds a pathological tree even before
/// any symlinks are found. Hitting it sets `truncated`. Mirrors the caps in
/// [`crate::folder_similarity_scan`] / [`crate::duplicates`].
const MAX_ENTRIES: u64 = 200_000;

/// Cap on symlinks collected for classification. Hitting it sets `truncated`.
const MAX_LINKS: u64 = 50_000;

/// The result of a dangling/cyclic symlink scan: the flagged links (healthy symlinks are omitted,
/// same convention as [`scan_dangling`]), how many filesystem entries were visited, and whether
/// either cap truncated the walk.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct DanglingReport {
    pub links: Vec<DanglingLink>,
    pub scanned: u64,
    pub truncated: bool,
}

/// Lexically normalise `path` — resolve `.`/`..` components by string manipulation only, never
/// touching the filesystem. Deliberately **not** [`std::fs::canonicalize`]: canonicalising a
/// self/loop symlink's target would follow the cycle at the OS level and fail, where this walk needs
/// to hand the raw (still-cyclic) target straight to [`scan_dangling`] for classification.
fn normalize(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                result.pop();
            }
            Component::CurDir => {}
            other => result.push(other.as_os_str()),
        }
    }
    result
}

/// Find dangling (target missing) and cyclic (self/loop) symlinks under `root`. Directories that
/// can't be read are skipped rather than failing the whole walk (never panics); a symlinked
/// directory is recorded as a link but never descended into (avoids walk cycles, same discipline as
/// [`crate::folder_similarity_scan`] / `list_dir`). A non-directory `root` is an `Err`.
pub fn find_dangling_links(root: &Path) -> Result<DanglingReport, String> {
    if !root.is_dir() {
        return Err(format!("{}: not a folder", root.display()));
    }

    let mut links: Vec<LinkEntry> = Vec::new();
    let mut scanned = 0u64;
    let mut truncated = false;
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];

    'walk: while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            if scanned >= MAX_ENTRIES {
                truncated = true;
                break 'walk;
            }
            scanned += 1;

            let path = normalize(&entry.path());

            if entry_is_symlink(&entry) {
                if links.len() as u64 >= MAX_LINKS {
                    truncated = true;
                    break 'walk;
                }
                if let Ok(raw_target) = std::fs::read_link(entry.path()) {
                    let resolved = if raw_target.is_absolute() {
                        normalize(&raw_target)
                    } else {
                        let parent = path.parent().map(Path::to_path_buf).unwrap_or_default();
                        normalize(&parent.join(&raw_target))
                    };
                    // metadata() follows symlinks; a self/loop chain fails at the OS level (ELOOP)
                    // rather than hanging, so this is safe even for a cyclic target.
                    let target_exists = std::fs::metadata(&resolved).is_ok();
                    links.push(LinkEntry {
                        path: path.to_string_lossy().into_owned(),
                        target: resolved.to_string_lossy().into_owned(),
                        target_exists,
                    });
                }
                continue;
            }

            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                stack.push(entry.path());
            }
        }
    }

    let flagged = scan_dangling(&links);
    Ok(DanglingReport { links: flagged, scanned, truncated })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-danglinglinks-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn non_directory_root_is_err() {
        let d = scratch("nondir");
        let f = d.join("file.txt");
        fs::write(&f, "x").unwrap();
        assert!(find_dangling_links(&f).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn empty_tree_scans_clean() {
        let d = scratch("empty");
        let r = find_dangling_links(&d).unwrap();
        assert!(r.links.is_empty());
        assert!(!r.truncated);
        let _ = fs::remove_dir_all(&d);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_to_deleted_target_is_missing() {
        use crate::dangling_links::DanglingReason;
        use std::os::unix::fs::symlink;

        let d = scratch("missing");
        let target = d.join("gone.txt");
        fs::write(&target, "will be deleted").unwrap();
        let link = d.join("broken_link");
        symlink(&target, &link).unwrap();
        fs::remove_file(&target).unwrap();

        let r = find_dangling_links(&d).unwrap();
        assert_eq!(r.links.len(), 1);
        assert_eq!(r.links[0].reason, DanglingReason::Missing);
        assert!(r.links[0].path.ends_with("broken_link"));
        let _ = fs::remove_dir_all(&d);
    }

    #[cfg(unix)]
    #[test]
    fn self_loop_symlink_is_cyclic() {
        use crate::dangling_links::DanglingReason;
        use std::os::unix::fs::symlink;

        let d = scratch("selfloop");
        let link = d.join("loopy");
        // Relative self-loop: the link's own stored target is just its own file name.
        symlink("loopy", &link).unwrap();

        let r = find_dangling_links(&d).unwrap();
        assert_eq!(r.links.len(), 1);
        assert_eq!(r.links[0].reason, DanglingReason::Cyclic);
        assert!(r.links[0].path.ends_with("loopy"));
        let _ = fs::remove_dir_all(&d);
    }

    #[cfg(unix)]
    #[test]
    fn two_symlink_loop_is_cyclic() {
        use crate::dangling_links::DanglingReason;
        use std::os::unix::fs::symlink;

        let d = scratch("twoloop");
        let a = d.join("a_link");
        let b = d.join("b_link");
        symlink("b_link", &a).unwrap();
        symlink("a_link", &b).unwrap();

        let r = find_dangling_links(&d).unwrap();
        assert_eq!(r.links.len(), 2);
        assert!(r.links.iter().all(|l| l.reason == DanglingReason::Cyclic));
        let _ = fs::remove_dir_all(&d);
    }

    #[cfg(unix)]
    #[test]
    fn valid_symlink_is_not_flagged() {
        use std::os::unix::fs::symlink;

        let d = scratch("valid");
        let target = d.join("real.txt");
        fs::write(&target, "still here").unwrap();
        let link = d.join("good_link");
        symlink(&target, &link).unwrap();

        let r = find_dangling_links(&d).unwrap();
        assert!(r.links.is_empty(), "a healthy symlink must not be reported");
        assert!(!r.truncated);
        let _ = fs::remove_dir_all(&d);
    }

    #[cfg(unix)]
    #[test]
    fn relative_target_in_subdir_resolves_against_link_parent() {
        use std::os::unix::fs::symlink;

        let d = scratch("relative");
        fs::create_dir_all(d.join("sub")).unwrap();
        fs::write(d.join("sub/real.txt"), "content").unwrap();
        // Relative target resolved against the link's own parent dir (sub/), not the walk root.
        symlink("real.txt", d.join("sub/rel_link")).unwrap();

        let r = find_dangling_links(&d).unwrap();
        assert!(r.links.is_empty(), "relative target must resolve against the link's own directory");
        let _ = fs::remove_dir_all(&d);
    }

    #[cfg(unix)]
    #[test]
    fn scanned_counts_every_entry_visited() {
        use std::os::unix::fs::symlink;

        let d = scratch("counts");
        fs::write(d.join("a.txt"), "a").unwrap();
        fs::write(d.join("b.txt"), "b").unwrap();
        symlink(d.join("a.txt"), d.join("link_to_a")).unwrap();

        let r = find_dangling_links(&d).unwrap();
        assert_eq!(r.scanned, 3, "a.txt, b.txt, link_to_a");
        let _ = fs::remove_dir_all(&d);
    }
}
