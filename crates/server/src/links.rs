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
}
