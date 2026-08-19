//! Link forge (CPE-802, epic CPE-715): create symbolic links and hard links. Pure `std` (only `std::os`
//! platform branches — no extra deps), so it lives in the Server (CPE-815); the Tauri commands are thin
//! `spawn_blocking` dispatchers. It IS OS-specific, but it's filesystem *domain* logic, not Tauri
//! plumbing, so the 3-OS CI compiles both cfg branches.

/// Create a symbolic link at `link_path` pointing to `target`. On Windows a directory target makes a
/// dir-symlink, else a file-symlink; the OS error is returned on failure (e.g. Windows symlink creation
/// without Developer Mode / admin), so the UI can prompt for elevation.
#[cfg(unix)]
pub fn create_symlink(target: &str, link_path: &str) -> Result<(), String> {
    std::os::unix::fs::symlink(target, link_path).map_err(|e| e.to_string())
}

/// Create a symbolic link at `link_path` pointing to `target` (Windows).
#[cfg(windows)]
pub fn create_symlink(target: &str, link_path: &str) -> Result<(), String> {
    let res = if std::path::Path::new(target).is_dir() {
        std::os::windows::fs::symlink_dir(target, link_path)
    } else {
        std::os::windows::fs::symlink_file(target, link_path)
    };
    res.map_err(|e| format!("{e} (Windows symbolic links require Developer Mode or elevation)"))
}

/// Create a hard link at `link_path` for the same file data as `target`. Cross-platform.
pub fn create_hard_link(target: &str, link_path: &str) -> Result<(), String> {
    std::fs::hard_link(target, link_path).map_err(|e| e.to_string())
}

/// Create a Windows directory junction at `link_path` pointing to `target` (CPE-1210, epic CPE-715). A
/// junction is an NTFS reparse point on a *directory* — unlike a symlink it needs no Developer Mode /
/// elevation, but `target` must already exist and be a directory. Uses the `junction` crate rather than
/// hand-rolling the `DeviceIoControl` reparse-point buffer layout (error-prone to get right); see the
/// `[target.'cfg(windows)'.dependencies]` block in `Cargo.toml` for the dep note.
#[cfg(windows)]
pub fn create_junction(target: &str, link_path: &str) -> Result<(), String> {
    let target_path = std::path::Path::new(target);
    if !target_path.is_dir() {
        return Err(format!("{target} is not a directory (junctions can only target directories)"));
    }
    junction::create(target_path, std::path::Path::new(link_path)).map_err(|e| e.to_string())
}

/// Non-Windows stub (CPE-1210): junctions are an NTFS-only reparse-point kind, so this always reports a
/// clear "Windows-only" error rather than compiling a no-op or failing to build.
#[cfg(not(windows))]
pub fn create_junction(_target: &str, _link_path: &str) -> Result<(), String> {
    Err("Directory junctions are Windows-only".to_string())
}

/// A path's link status for the file list + link tooling (CPE-804, epic CPE-715): whether it's a symlink,
/// where it points, and whether that target is currently missing (a broken link).
#[derive(serde::Serialize, Default, PartialEq, Debug)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct LinkStatus {
    pub is_symlink: bool,
    /// The link's stored target (may be relative). `None` for a non-symlink or an unreadable link.
    pub target: Option<String>,
    /// True only for a symlink whose target does not currently resolve.
    pub broken: bool,
}

/// Whether a symlink's resolved-target `stat` outcome should be reported `broken` (CPE-1692). Split out,
/// mirroring `dispatch::classify_path_error`, so the `NotFound`-vs-everything-else split is
/// unit-testable without touching a real filesystem (permission bits are platform- and
/// privilege-dependent — inert as root, or defeated by Windows's default "bypass traverse checking"
/// privilege — so an ACL-based test alone would leave this taxonomy unverified on some machines).
///
/// Only a genuine `NotFound` means the target is actually gone; any other stat failure (permission
/// denied on a directory along the resolved path, a dead network mount, …) means we don't know, and
/// `broken` must not claim otherwise — this flag's one production consumer, `RepairLinkDialog`'s
/// pre-delete TOCTOU recheck, only proceeds with the destructive delete when `broken` is true, so
/// treating "unknown" as "not confirmed broken" is the safe default (never destroys a link on an
/// uncertain read), not merely the honest one.
///
/// **Decided, documented (PR #874 review, "F6"): a cyclic/self-referential symlink is deliberately NOT
/// flagged `broken` by this function.** Unlike a genuine permission-denied stat, an ELOOP-family failure
/// (`ErrorKind::FilesystemLoop` on Unix, `ERROR_CANT_RESOLVE_FILENAME` on Windows) is not actually an
/// "unknown" — the OS has definitively said the chain can never resolve — so this IS a case this
/// function could in principle classify as confidently broken rather than folding it into "everything
/// non-`NotFound` is unknown". It stays folded in anyway, for the same reason `split_join.rs`'s own
/// CPE-1687 guard does (see its `a_manifest_that_cannot_be_stat_ed_is_not_reported_as_not_found` test
/// comment): `ErrorKind::FilesystemLoop` is not yet practical to match on reliably against this crate's
/// `rust-version = "1.77.2"` MSRV, and this repo's established practice (there, and here) is not to
/// special-case an error kind it can't name in code. The trade-off this creates: `RepairLinkDialog` will
/// no longer offer to repair a cyclic self-loop link (`broken` flips `true` → `false` for that one
/// case), pinned by `link_status_does_not_flag_a_cyclic_self_loop_as_broken` below. If a later MSRV bump
/// makes matching `FilesystemLoop` practical, add it as a second `true` arm here and update that test.
fn target_stat_is_broken(stat_err_kind: Option<std::io::ErrorKind>) -> bool {
    stat_err_kind == Some(std::io::ErrorKind::NotFound)
}

/// Inspect `path`: is it a symlink, what does it point at, and is its target missing (broken)? Never fails
/// — an unreadable/absent path reports as a non-symlink. `broken` follows the link via `metadata()` and is
/// true only when that resolution genuinely confirms the target is gone.
pub fn link_status(path: &str) -> LinkStatus {
    let p = std::path::Path::new(path);
    let is_symlink = std::fs::symlink_metadata(p)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false);
    if !is_symlink {
        return LinkStatus::default();
    }
    let target = std::fs::read_link(p).ok().map(|t| t.to_string_lossy().into_owned());
    // metadata() follows the link.
    let broken = target_stat_is_broken(std::fs::metadata(p).err().map(|e| e.kind()));
    LinkStatus { is_symlink: true, target, broken }
}

/// Bounded-depth walk to keep repair search cheap and cycle-safe (CPE-1027).
const REPAIR_SEARCH_MAX_DEPTH: u32 = 4;

/// Suggest a repair target for a broken symlink (CPE-1027, epic CPE-715): read `broken_link`'s stored
/// target, take its basename, then search `search_roots` in order (each a bounded-depth ≤4 recursive
/// walk, skipping unreadable dirs — mirrors `list_dir`'s skip-on-error convention) for the first entry
/// whose file name matches. Returns that entry's full path, or `None` if `broken_link` isn't a readable
/// symlink or nothing matches in any root.
pub fn suggest_repair(broken_link: &str, search_roots: &[&str]) -> Option<String> {
    let link_path = std::path::Path::new(broken_link);
    let is_symlink = std::fs::symlink_metadata(link_path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false);
    if !is_symlink {
        return None;
    }
    let target = std::fs::read_link(link_path).ok()?;
    let basename = target.file_name()?.to_os_string();

    for root in search_roots {
        let root_path = std::path::Path::new(root);
        if let Some(found) = find_by_name(root_path, &basename, REPAIR_SEARCH_MAX_DEPTH) {
            return Some(found.to_string_lossy().into_owned());
        }
    }
    None
}

/// Depth-bounded recursive search for an entry named `name` directly under `dir` or one of its
/// subdirectories, up to `depth_left` levels down. Unreadable dirs/entries are skipped rather than
/// failing the whole walk (never panics). Never descends into a symlink — a symlinked "directory" reports
/// `is_dir() == false` here since `DirEntry::file_type()` doesn't follow links, which also keeps this from
/// chasing the broken link itself or a cyclical link back into an ancestor.
fn find_by_name(dir: &std::path::Path, name: &std::ffi::OsStr, depth_left: u32) -> Option<std::path::PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        if entry.file_name() == *name {
            return Some(entry.path());
        }
        if depth_left > 0 && entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            subdirs.push(entry.path());
        }
    }
    for sub in subdirs {
        if let Some(found) = find_by_name(&sub, name, depth_left - 1) {
            return Some(found);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch() -> crate::fsutil::ScratchDir {
        crate::fsutil::scratch_dir("cpe-links")
    }

    #[test]
    fn hard_link_shares_content() {
        let d = scratch();
        let src = d.join("a.txt");
        fs::write(&src, b"hello").unwrap();
        let link = d.join("b.txt");
        create_hard_link(&src.to_string_lossy(), &link.to_string_lossy()).unwrap();
        assert_eq!(fs::read(&link).unwrap(), b"hello");
        // A hard link to a missing source errors, not panics.
        assert!(create_hard_link(&d.join("missing").to_string_lossy(), &d.join("c.txt").to_string_lossy()).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn symlink_points_at_target_where_permitted() {
        let d = scratch();
        let target = d.join("t.txt");
        fs::write(&target, b"data").unwrap();
        let link = d.join("l.txt");
        // Symlink creation is unprivileged on Windows only with Developer Mode / admin — skip if it fails
        // there; POSIX always permits it.
        match create_symlink(&target.to_string_lossy(), &link.to_string_lossy()) {
            Ok(()) => assert_eq!(fs::read(&link).unwrap(), b"data"),
            Err(_) => { /* unprivileged Windows — the error path is what we return to the UI */ }
        }
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn link_status_detects_symlink_and_broken_target() {
        let d = scratch();
        let target = d.join("t.txt");
        fs::write(&target, b"data").unwrap();
        // A plain file is not a symlink.
        let plain = link_status(&target.to_string_lossy());
        assert!(!plain.is_symlink && !plain.broken && plain.target.is_none());

        let link = d.join("l.txt");
        match create_symlink(&target.to_string_lossy(), &link.to_string_lossy()) {
            Ok(()) => {
                let intact = link_status(&link.to_string_lossy());
                assert!(intact.is_symlink && !intact.broken, "target exists → not broken");
                // Remove the target → the symlink is now broken.
                fs::remove_file(&target).unwrap();
                let broken = link_status(&link.to_string_lossy());
                assert!(broken.is_symlink && broken.broken, "target removed → broken link");
            }
            Err(_) => { /* unprivileged Windows — skip (symlink creation gated) */ }
        }
        let _ = fs::remove_dir_all(&d);
    }

    /// The deterministic half of the CPE-1692 guard (runs on every OS/account, no privilege needed) —
    /// same role as `dispatch::classify_path_error`'s own unit tests. Proves the taxonomy the wiring
    /// below (and the end-to-end test after it) depends on.
    #[test]
    fn target_stat_is_broken_only_for_a_genuine_absence() {
        assert!(target_stat_is_broken(Some(std::io::ErrorKind::NotFound)));
        assert!(!target_stat_is_broken(None), "no stat error at all (target exists) is not broken");
        for kind in [std::io::ErrorKind::PermissionDenied, std::io::ErrorKind::Other, std::io::ErrorKind::TimedOut] {
            assert!(!target_stat_is_broken(Some(kind)), "{kind:?} must not be reported as broken");
        }
    }

    /// The end-to-end half, driving the real `link_status` entry point rather than the pure classifier
    /// above. `link_status` calls `fs::metadata(p)` (it needs to *follow* the link, not just check
    /// existence), so this genuinely belongs on `fsutil::deny_dir_traversal` — unlike the `try_exists`
    /// sites (`disk_usage`, `native_meta`, `move_exact_impl`, `crates/sftp` `open`), which the PR #874
    /// review found need a *different* mechanism (`fsutil::deny_stat_of`) because `try_exists` is a
    /// different syscall on Windows that a target-level deny ACE does refuse. `deny_dir_traversal` itself
    /// stays genuinely Unix-only (see its doc comment for the measurement); this leg SKIPS on Windows
    /// even with symlink privilege, for real, and that's the correct mechanism for this specific site —
    /// the deterministic test above is what pins the taxonomy on every OS/account regardless. Two
    /// independent conditions must both be constructible on this machine (symlink creation, then the
    /// deny), so both get their own loud, non-silent skip notice — a bare "skip" here would be exactly
    /// the kind of under-covered sweep this ticket exists to close a fourth time.
    #[test]
    fn link_status_does_not_report_broken_for_a_permission_denied_target_not_missing() {
        use std::path::Path;

        let d = scratch();
        let denied_dir = d.join("denied");
        fs::create_dir_all(&denied_dir).unwrap();
        let real_target = denied_dir.join("real.txt");
        fs::write(&real_target, b"data").unwrap();
        let link = d.join("l.txt");

        if create_symlink(&real_target.to_string_lossy(), &link.to_string_lossy()).is_err() {
            use std::io::Write;
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1692] SKIPPED link_status permission-denied leg: symlink creation is not permitted \
                 on this machine (unprivileged Windows). The remaining assertions do NOT cover CPE-1692 \
                 for links::link_status."
            );
            let _ = fs::remove_dir_all(&d);
            return;
        }

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
                "[CPE-1692] SKIPPED link_status permission-denied leg: could not deny traversal on {} on \
                 this machine (elevated/root, or a filesystem ignoring ACLs/mode bits). The remaining \
                 assertions do NOT cover CPE-1692 for links::link_status.",
                denied_dir.display()
            );
            return;
        }

        let status = link_status(&link.to_string_lossy());
        assert!(status.is_symlink);
        assert!(
            !status.broken,
            "a permission-denied target must not be confidently reported as broken — it's right there, \
             just unreadable through its parent"
        );
        // `_restore` cleans up on the way out, panic or not.
    }

    /// F6 (PR #874 review), pinning the decision documented on `target_stat_is_broken`: a cyclic
    /// self-loop symlink is NOT flagged `broken`, even though — unlike a permission-denied stat — the OS
    /// has definitively said the chain can never resolve. Needs no OS permission mechanism (a self-loop
    /// symlink is constructible everywhere symlink creation itself is permitted), so it runs for real
    /// wherever the existing symlink-creation-privilege gate lets it (this file's established pattern —
    /// see `symlink_points_at_target_where_permitted` above), skipping silently only on unprivileged
    /// Windows exactly as every other symlink-creation test in this file already does.
    #[test]
    fn link_status_does_not_flag_a_cyclic_self_loop_as_broken() {
        let d = scratch();
        let link = d.join("loopy");
        // A relative self-loop: the link's own stored target is just its own file name, so resolving it
        // chases the link back to itself — the same construction `dangling_links_scan`'s
        // `self_loop_symlink_is_cyclic` test uses, here driving `link_status` instead of the walker.
        match create_symlink("loopy", &link.to_string_lossy()) {
            Ok(()) => {
                let status = link_status(&link.to_string_lossy());
                assert!(status.is_symlink, "a self-loop is still a symlink");
                assert!(
                    !status.broken,
                    "documented CPE-1692/F6 decision: a cyclic loop folds into the same \
                     not-confidently-classified bucket as an unknown stat failure, not into `broken`"
                );
            }
            Err(_) => { /* unprivileged Windows — skip (symlink creation gated), same as every other
                          symlink-creation test in this file */ }
        }
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn suggest_repair_finds_same_named_entry_under_a_search_root() {
        let d = scratch();
        // The broken link "points at" a target that doesn't exist right here...
        let missing_target = d.join("gone").join("report.txt");
        let link = d.join("l.txt");
        match create_symlink(&missing_target.to_string_lossy(), &link.to_string_lossy()) {
            Ok(()) => {
                // ...but a same-named file exists a couple of levels down under a search root.
                let root = d.join("root");
                let nested = root.join("a").join("b");
                fs::create_dir_all(&nested).unwrap();
                let found_file = nested.join("report.txt");
                fs::write(&found_file, b"data").unwrap();

                let result = suggest_repair(&link.to_string_lossy(), &[&root.to_string_lossy()]);
                assert_eq!(result, Some(found_file.to_string_lossy().into_owned()));
            }
            Err(_) => { /* unprivileged Windows — skip (symlink creation gated) */ }
        }
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn suggest_repair_searches_roots_in_order_and_returns_first_match() {
        let d = scratch();
        let missing_target = d.join("gone.txt");
        let link = d.join("l.txt");
        match create_symlink(&missing_target.to_string_lossy(), &link.to_string_lossy()) {
            Ok(()) => {
                let root_a = d.join("root_a");
                let root_b = d.join("root_b");
                fs::create_dir_all(&root_a).unwrap();
                fs::create_dir_all(&root_b).unwrap();
                // Only the second root has a match.
                let found_file = root_b.join("gone.txt");
                fs::write(&found_file, b"data").unwrap();

                let result = suggest_repair(
                    &link.to_string_lossy(),
                    &[&root_a.to_string_lossy(), &root_b.to_string_lossy()],
                );
                assert_eq!(result, Some(found_file.to_string_lossy().into_owned()));
            }
            Err(_) => { /* unprivileged Windows — skip (symlink creation gated) */ }
        }
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn suggest_repair_returns_none_when_no_match_found() {
        let d = scratch();
        let missing_target = d.join("nowhere.txt");
        let link = d.join("l.txt");
        match create_symlink(&missing_target.to_string_lossy(), &link.to_string_lossy()) {
            Ok(()) => {
                let root = d.join("root");
                fs::create_dir_all(&root).unwrap();
                fs::write(root.join("unrelated.txt"), b"data").unwrap();

                let result = suggest_repair(&link.to_string_lossy(), &[&root.to_string_lossy()]);
                assert_eq!(result, None);
            }
            Err(_) => { /* unprivileged Windows — skip (symlink creation gated) */ }
        }
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn suggest_repair_returns_none_for_non_symlink_input() {
        let d = scratch();
        let plain = d.join("plain.txt");
        fs::write(&plain, b"data").unwrap();
        let root = d.join("root");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("plain.txt"), b"data").unwrap();

        // `plain` is a regular file, not a symlink, so no target to resolve → None even though a
        // same-named file exists under the root.
        let result = suggest_repair(&plain.to_string_lossy(), &[&root.to_string_lossy()]);
        assert_eq!(result, None);
        let _ = fs::remove_dir_all(&d);
    }

    /// Windows-only (CPE-1210): a junction to a directory resolves — listing through it reaches the
    /// target's contents — and, unlike a symlink, creation needs no Developer Mode / elevation, so this
    /// isn't gated behind the unprivileged-Windows skip pattern used for the symlink tests above.
    #[cfg(windows)]
    #[test]
    fn junction_resolves_to_target_directory() {
        let d = scratch();
        let target_dir = d.join("target_dir");
        fs::create_dir_all(&target_dir).unwrap();
        fs::write(target_dir.join("inside.txt"), b"junction-data").unwrap();

        let link = d.join("j_link");
        create_junction(&target_dir.to_string_lossy(), &link.to_string_lossy()).unwrap();

        // Resolves: the file inside the target is reachable through the junction path.
        let via_link = fs::read(link.join("inside.txt")).unwrap();
        assert_eq!(via_link, b"junction-data");

        // Reports as a reparse point (symlink-like) to the rest of the app's link tooling.
        let status = link_status(&link.to_string_lossy());
        assert!(status.is_symlink, "a junction is a reparse point — should read as symlink-like");
        assert!(!status.broken, "target exists — should not read as broken");

        let _ = fs::remove_dir_all(&d);
    }

    /// Windows-only: junctions can only target directories — a file target is a clear validation error,
    /// not a confusing OS error surfaced straight from the syscall.
    #[cfg(windows)]
    #[test]
    fn junction_rejects_file_target() {
        let d = scratch();
        let file_target = d.join("not_a_dir.txt");
        fs::write(&file_target, b"data").unwrap();
        let link = d.join("j_link2");

        let result = create_junction(&file_target.to_string_lossy(), &link.to_string_lossy());
        assert!(result.is_err());
        assert!(!link.exists(), "no junction should be created for a file target");

        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn suggest_repair_returns_none_when_search_root_is_unreadable_or_missing() {
        let d = scratch();
        let missing_target = d.join("nowhere.txt");
        let link = d.join("l.txt");
        match create_symlink(&missing_target.to_string_lossy(), &link.to_string_lossy()) {
            Ok(()) => {
                // The search root itself doesn't exist — read_dir fails, the walk skips it (not fatal),
                // and the overall result is None rather than a panic or an Err.
                let missing_root = d.join("does-not-exist");
                let result = suggest_repair(&link.to_string_lossy(), &[&missing_root.to_string_lossy()]);
                assert_eq!(result, None);
            }
            Err(_) => { /* unprivileged Windows — skip (symlink creation gated) */ }
        }
        let _ = fs::remove_dir_all(&d);
    }
}
