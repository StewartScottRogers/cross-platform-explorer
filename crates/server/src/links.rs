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

/// Inspect `path`: is it a symlink, what does it point at, and is its target missing (broken)? Never fails
/// — an unreadable/absent path reports as a non-symlink. `broken` follows the link via `metadata()` and is
/// true only when that resolution fails for a symlink.
pub fn link_status(path: &str) -> LinkStatus {
    let p = std::path::Path::new(path);
    let is_symlink = std::fs::symlink_metadata(p)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false);
    if !is_symlink {
        return LinkStatus::default();
    }
    let target = std::fs::read_link(p).ok().map(|t| t.to_string_lossy().into_owned());
    let broken = std::fs::metadata(p).is_err(); // metadata follows the link; err ⇒ target gone
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

    fn scratch() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-links-{}-{}", std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
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
