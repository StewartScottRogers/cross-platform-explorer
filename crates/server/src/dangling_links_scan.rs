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

use std::ops::ControlFlow;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;

use crate::dangling_links::{scan_dangling, DanglingLink, LinkEntry};
use crate::fsutil::entry_is_symlink;
use crate::glob::any_glob_matches;

/// Cap on filesystem entries visited across the whole walk — bounds a pathological tree even before
/// any symlinks are found. Hitting it sets `truncated`. Mirrors the caps in
/// [`crate::folder_similarity_scan`] / [`crate::duplicates`].
const MAX_ENTRIES: u64 = 200_000;

/// Cap on symlinks collected for classification. Hitting it sets `truncated`.
const MAX_LINKS: u64 = 50_000;

/// Number of classified links per flush — mirrors [`crate::listing::LIST_DIR_BATCH`]'s role: small
/// enough that a streaming caller (CPE-1299) paints promptly, large enough that a typical tree's
/// results are one flush.
pub const DANGLING_LINKS_BATCH: usize = 256;

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

/// The tail of a [`walk_dangling_links`] run: how many filesystem entries were visited and whether
/// either cap truncated the walk. Doesn't carry the links themselves — those are delivered to
/// `flush` as the walk (and classification) completes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanTail {
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

/// Whether a symlink's resolved-target `stat` outcome means `target_exists` (CPE-1692). Split out,
/// mirroring `dispatch::classify_path_error`, so the `NotFound`-vs-everything-else split is
/// unit-testable without touching a real filesystem (permission bits are platform- and
/// privilege-dependent — inert as root, or defeated by Windows's default "bypass traverse checking"
/// privilege — so an ACL-based test alone would leave this taxonomy unverified on some machines).
///
/// `.is_ok()` used to fold every non-`NotFound` stat failure (permission denied on a directory along the
/// resolved path, a dead network mount, ELOOP itself) into `target_exists = false`, which
/// [`scan_dangling`] reports as [`crate::dangling_links::DanglingReason::Missing`] — a confident "gone"
/// for a target that was never actually confirmed absent. Only a genuine `NotFound` means the target is
/// actually gone; every other stat failure leaves `target_exists = true` so the link is NOT flagged
/// (mirrors [`crate::links::link_status`]: "unknown" is not "confirmed missing"). A link that really IS a
/// cycle among the walked symlinks is still caught — `scan_dangling`'s own chain-walk over the supplied
/// `LinkEntry`s classifies that, taking precedence over `target_exists` regardless of its value, so ELOOP
/// folding to `true` here costs nothing for the self/loop case (see `scan_dangling`'s doc comment and its
/// `cyclic_precedence_over_false_target_exists` test).
fn target_exists_from_stat(stat_err_kind: Option<std::io::ErrorKind>) -> bool {
    stat_err_kind != Some(std::io::ErrorKind::NotFound)
}

/// Walk `root` collecting symlinks (skip-unreadable, never descending into a symlinked directory —
/// same discipline as [`crate::folder_similarity_scan`] / `list_dir`), classify the whole set with
/// [`scan_dangling`], then hand the flagged links to `flush` in batches of up to
/// [`DANGLING_LINKS_BATCH`], honoring an early [`ControlFlow::Break`] by stopping further flushes.
///
/// Classification genuinely needs the full link set before any single link can be judged — a link's
/// target may point at another symlink discovered later, anywhere else in the tree, and cyclic
/// detection ([`scan_dangling`]) has to see every candidate to tell a cycle from a chain that simply
/// runs off the known links. So unlike [`crate::listing::stream_dir_entries`], the walk itself always
/// runs to completion (or a cap) before the first flush; `flush` still lets a streaming caller paint
/// the *results* incrementally and stop consuming further batches early. A non-directory `root` is an
/// `Err`.
///
/// `excludes` are glob patterns (CPE-1302): any discovered sub-directory whose base name matches one is
/// pruned — skipped and never descended into, so no symlink inside it is collected or classified. An
/// **empty** `excludes` slice prunes nothing, so passing `&[]` is byte-identical to the pre-exclude
/// behavior. The `root` itself is always walked regardless of `excludes`.
pub fn walk_dangling_links(
    root: &Path,
    excludes: &[String],
    mut flush: impl FnMut(Vec<DanglingLink>) -> ControlFlow<()>,
) -> Result<ScanTail, String> {
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
                    // rather than hanging, so this is safe even for a cyclic target. See
                    // `target_exists_from_stat`'s doc comment for the CPE-1692 taxonomy.
                    let target_exists =
                        target_exists_from_stat(std::fs::metadata(&resolved).err().map(|e| e.kind()));
                    links.push(LinkEntry {
                        path: path.to_string_lossy().into_owned(),
                        target: resolved.to_string_lossy().into_owned(),
                        target_exists,
                    });
                }
                continue;
            }

            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() && !any_glob_matches(&entry.file_name().to_string_lossy(), excludes) {
                stack.push(entry.path());
            }
        }
    }

    let flagged = scan_dangling(&links);
    for batch in flagged.chunks(DANGLING_LINKS_BATCH) {
        if flush(batch.to_vec()).is_break() {
            break;
        }
    }

    Ok(ScanTail { scanned, truncated })
}

/// Find dangling (target missing) and cyclic (self/loop) symlinks under `root`. Directories that
/// can't be read are skipped rather than failing the whole walk (never panics); a symlinked
/// directory is recorded as a link but never descended into (avoids walk cycles, same discipline as
/// [`crate::folder_similarity_scan`] / `list_dir`). A non-directory `root` is an `Err`. Thin
/// collect-to-vec wrapper over [`walk_dangling_links`]. `excludes` prunes matching sub-directories
/// (CPE-1302); pass `&[]` for the original whole-tree walk.
pub fn find_dangling_links(root: &Path, excludes: &[String]) -> Result<DanglingReport, String> {
    let mut links = Vec::new();
    let tail = walk_dangling_links(root, excludes, |batch| {
        links.extend(batch);
        ControlFlow::Continue(())
    })?;
    Ok(DanglingReport { links, scanned: tail.scanned, truncated: tail.truncated })
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
        assert!(find_dangling_links(&f, &[]).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn empty_tree_scans_clean() {
        let d = scratch("empty");
        let r = find_dangling_links(&d, &[]).unwrap();
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

        let r = find_dangling_links(&d, &[]).unwrap();
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

        let r = find_dangling_links(&d, &[]).unwrap();
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

        let r = find_dangling_links(&d, &[]).unwrap();
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

        let r = find_dangling_links(&d, &[]).unwrap();
        assert!(r.links.is_empty(), "a healthy symlink must not be reported");
        assert!(!r.truncated);
        let _ = fs::remove_dir_all(&d);
    }

    /// The deterministic half of the CPE-1692 guard (runs on every OS/account, no privilege needed) —
    /// same role as `dispatch::classify_path_error`'s own unit tests. Proves the taxonomy the wiring
    /// depends on; unlike the end-to-end test below it never skips.
    #[test]
    fn target_exists_from_stat_says_missing_only_for_a_genuine_absence() {
        assert!(!target_exists_from_stat(Some(std::io::ErrorKind::NotFound)), "a genuine absence");
        assert!(target_exists_from_stat(None), "no stat error at all (target exists)");
        for kind in [std::io::ErrorKind::PermissionDenied, std::io::ErrorKind::Other, std::io::ErrorKind::TimedOut] {
            assert!(target_exists_from_stat(Some(kind)), "{kind:?} must not be reported as absence");
        }
    }

    /// The end-to-end half, driving the real `find_dangling_links` entry point rather than the pure
    /// classifier above. This walk calls `fs::metadata(&resolved)` (it needs to follow the target, not
    /// just check existence), so `fsutil::deny_dir_traversal` is the right mechanism here — unlike the
    /// `try_exists`-based CPE-1692 sites (`disk_usage`, `native_meta`, `move_exact_impl`, `crates/sftp`
    /// `open`), which the PR #874 review found need `fsutil::deny_stat_of` instead, because `try_exists`
    /// is a different syscall on Windows that a target-level deny ACE does refuse even though
    /// `fs::metadata` does not (see that helper's doc comment). `deny_dir_traversal` itself stays
    /// genuinely Unix-only (its own doc comment has the measurement); this leg is `#[cfg(unix)]` like the
    /// walk's other symlink tests (symlink creation here is unconditional, unlike `links.rs`'s gated
    /// match), and a runtime probe still covers the case where the account running the suite can't set
    /// the deny (e.g. root, where permission bits are inert) — that leg gets its own loud skip notice per
    /// the Evidence Rules; the deterministic test above is what pins the taxonomy on every OS regardless.
    #[cfg(unix)]
    #[test]
    fn walk_dangling_links_does_not_flag_a_permission_denied_target_as_missing() {
        use std::os::unix::fs::symlink;

        let d = scratch("denied-target");
        let denied_dir = d.join("denied");
        fs::create_dir_all(&denied_dir).unwrap();
        let real_target = denied_dir.join("real.txt");
        fs::write(&real_target, "data").unwrap();
        let link = d.join("l");
        symlink(&real_target, &link).unwrap();

        struct Restore<'a>(&'a Path, &'a Path);
        impl Drop for Restore<'_> {
            fn drop(&mut self) {
                crate::fsutil::undo_deny_dir_traversal(self.0);
                let _ = fs::remove_dir_all(self.1);
            }
        }
        let _restore = Restore(&denied_dir, &d);

        if !crate::fsutil::deny_dir_traversal(&denied_dir, &real_target) {
            use std::io::Write;
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1692] SKIPPED walk_dangling_links permission-denied leg: could not deny traversal \
                 on {} on this machine (e.g. running as root, where permission bits are inert). The \
                 remaining assertions do NOT cover CPE-1692 for dangling_links_scan.",
                denied_dir.display()
            );
            return;
        }

        let r = find_dangling_links(&d, &[]).unwrap();
        assert!(
            r.links.is_empty(),
            "a permission-denied target must not be confidently reported as dangling: {:?}",
            r.links
        );
        // `_restore` cleans up on the way out, panic or not.
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

        let r = find_dangling_links(&d, &[]).unwrap();
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

        let r = find_dangling_links(&d, &[]).unwrap();
        assert_eq!(r.scanned, 3, "a.txt, b.txt, link_to_a");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn walk_non_directory_root_is_err() {
        let d = scratch("walk-nondir");
        let f = d.join("file.txt");
        fs::write(&f, "x").unwrap();
        assert!(walk_dangling_links(&f, &[], |_| ControlFlow::Continue(())).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[cfg(unix)]
    #[test]
    fn walk_dangling_links_matches_find_dangling_links() {
        use std::os::unix::fs::symlink;

        let d = scratch("walk-matches");
        let target = d.join("gone.txt");
        fs::write(&target, "will be deleted").unwrap();
        symlink(&target, d.join("broken_link")).unwrap();
        fs::remove_file(&target).unwrap();
        symlink("loopy", d.join("loopy")).unwrap();
        fs::write(d.join("plain.txt"), "x").unwrap();

        let mut streamed = Vec::new();
        let tail = walk_dangling_links(&d, &[], |batch| {
            streamed.extend(batch);
            ControlFlow::Continue(())
        })
        .unwrap();

        let collected = find_dangling_links(&d, &[]).unwrap();
        assert_eq!(tail.scanned, collected.scanned);
        assert_eq!(tail.truncated, collected.truncated);

        let mut a: Vec<_> = streamed.iter().map(|l| l.path.clone()).collect();
        let mut b: Vec<_> = collected.links.iter().map(|l| l.path.clone()).collect();
        a.sort();
        b.sort();
        assert_eq!(a, b, "walker's flushed batches carry the same flagged links as the collect-to-vec form");
        let _ = fs::remove_dir_all(&d);
    }

    #[cfg(unix)]
    #[test]
    fn walk_dangling_links_batches_a_large_flagged_set() {
        use std::os::unix::fs::symlink;

        // More than one DANGLING_LINKS_BATCH worth of broken symlinks, so the flush loop must run
        // more than once.
        let d = scratch("walk-batches");
        let n = DANGLING_LINKS_BATCH * 2 + 10;
        for i in 0..n {
            symlink(format!("/nowhere/{i}"), d.join(format!("broken_{i:05}"))).unwrap();
        }

        let mut batch_sizes = Vec::new();
        let mut total = 0usize;
        let tail = walk_dangling_links(&d, &[], |batch| {
            total += batch.len();
            batch_sizes.push(batch.len());
            ControlFlow::Continue(())
        })
        .unwrap();

        assert_eq!(total, n, "every flagged link is delivered across all flushes");
        assert!(batch_sizes.len() >= 2, "{n} links at batch {DANGLING_LINKS_BATCH} must flush more than once");
        assert!(batch_sizes.iter().all(|&s| s <= DANGLING_LINKS_BATCH), "no batch exceeds the cap");
        assert!(!tail.truncated);
        let _ = fs::remove_dir_all(&d);
    }

    #[cfg(unix)]
    #[test]
    fn walk_dangling_links_stops_flushing_on_break() {
        use std::os::unix::fs::symlink;

        let d = scratch("walk-break");
        let n = DANGLING_LINKS_BATCH * 2 + 10;
        for i in 0..n {
            symlink(format!("/nowhere/{i}"), d.join(format!("broken_{i:05}"))).unwrap();
        }

        let mut seen = 0usize;
        let mut flushes = 0usize;
        let tail = walk_dangling_links(&d, &[], |batch| {
            seen += batch.len();
            flushes += 1;
            ControlFlow::Break(())
        })
        .unwrap();

        assert_eq!(flushes, 1, "break on the first flush stops further batches");
        assert_eq!(seen, DANGLING_LINKS_BATCH, "only the first batch was delivered");
        // The walk + classification still ran to completion before flushing began (classification
        // needs the full link set), so the tail reflects the whole tree regardless of the break.
        assert_eq!(tail.scanned, n as u64);
        assert!(!tail.truncated);
        let _ = fs::remove_dir_all(&d);
    }

    // --- CPE-1302: exclude-glob support. ---

    #[cfg(unix)]
    #[test]
    fn excluded_directory_is_pruned_and_its_broken_link_is_not_reported() {
        use std::os::unix::fs::symlink;

        let d = scratch("exclude-prune");
        fs::create_dir_all(d.join("node_modules")).unwrap();
        // A broken symlink hides inside node_modules, another sits at the top.
        symlink("/nowhere/gone", d.join("node_modules/broken_inner")).unwrap();
        symlink("/nowhere/gone", d.join("broken_top")).unwrap();

        // Baseline: empty excludes flags both broken links.
        let all = find_dangling_links(&d, &[]).unwrap();
        assert_eq!(all.links.len(), 2, "empty excludes flags every broken link: {:?}", all.links);

        // Excluding node_modules prunes it: the inner broken link is neither visited nor reported.
        let excl = find_dangling_links(&d, &["node_modules".to_string()]).unwrap();
        assert_eq!(excl.links.len(), 1, "the broken link inside node_modules must not be reported: {:?}", excl.links);
        assert!(excl.links[0].path.ends_with("broken_top"));
        assert!(!excl.links.iter().any(|l| l.path.contains("node_modules")));
        // node_modules's contents (the inner symlink) were never visited, so fewer entries scanned.
        assert!(excl.scanned < all.scanned, "the pruned dir's contents are not scanned");
        let _ = fs::remove_dir_all(&d);
    }
}
