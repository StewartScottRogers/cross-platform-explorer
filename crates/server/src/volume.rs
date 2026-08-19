//! Same-volume detection (CPE-1026, epic CPE-661): decide whether two paths live on the same
//! volume — the foundation for copy-vs-move on drag-drop (same volume → move, different volume →
//! copy, the OS convention). Pure `std` (only `std::os`/`std::path` platform branches — no extra
//! deps), so it lives in the Server alongside [`crate::links`].

/// Do `a` and `b` live on the same volume?
///
/// On Unix this compares each path's device id (`st_dev`) via `metadata()`; if either path can't
/// be stat'd, we can't confirm they're the same volume, so we return `false` (the safe "different
/// volume → copy" default) rather than assume.
#[cfg(unix)]
pub fn same_volume(a: &str, b: &str) -> bool {
    use std::os::unix::fs::MetadataExt;
    let dev_of = |p: &str| std::fs::metadata(p).map(|m| m.dev());
    match (dev_of(a), dev_of(b)) {
        (Ok(da), Ok(db)) => da == db,
        _ => false,
    }
}

/// Do `a` and `b` live on the same volume (Windows)?
///
/// Compares each path's [`std::path::Prefix`] component (drive letter `C:` / UNC share
/// `\\server\share`) case-insensitively — no filesystem access, so a nonexistent path still
/// compares fine on its literal prefix.
///
/// **v1 limitation:** this is a *path-prefix* check, not a true volume-id check. Two distinct
/// mount points under one drive letter (e.g. a mounted VHD folder, or a directory junction onto
/// another disk) share the same drive-letter prefix and so read as "same volume" even though
/// they're different physical volumes. A `GetVolumePathNameW`/`GetVolumeInformationW`-based
/// refinement that resolves the true volume id is a later slice — deliberately not pulled in here
/// to keep this module dependency-free.
#[cfg(windows)]
pub fn same_volume(a: &str, b: &str) -> bool {
    use std::path::{Component, Path};

    fn prefix(p: &str) -> Option<String> {
        Path::new(p).components().find_map(|c| match c {
            Component::Prefix(prefix) => {
                Some(prefix.as_os_str().to_string_lossy().to_lowercase())
            }
            _ => None,
        })
    }

    match (prefix(a), prefix(b)) {
        (Some(pa), Some(pb)) => pa == pb,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    mod unix_tests {
        use super::*;
        use std::fs;

        fn scratch() -> crate::fsutil::ScratchDir {
            crate::fsutil::scratch_dir("cpe-volume")
        }

        #[test]
        fn same_dir_two_files_are_same_volume() {
            let d = scratch();
            let a = d.join("a.txt");
            let b = d.join("b.txt");
            fs::write(&a, b"x").unwrap();
            fs::write(&b, b"y").unwrap();
            assert!(same_volume(&a.to_string_lossy(), &b.to_string_lossy()));
            // A directory and a file it contains are on the same volume too.
            assert!(same_volume(&d.to_string_lossy(), &a.to_string_lossy()));
            let _ = fs::remove_dir_all(&d);
        }

        #[test]
        fn missing_path_is_not_confirmed_same_volume() {
            let d = scratch();
            let real = d.join("real.txt");
            fs::write(&real, b"x").unwrap();
            let missing = d.join("does-not-exist.txt");
            assert!(!same_volume(&real.to_string_lossy(), &missing.to_string_lossy()));
            assert!(!same_volume(&missing.to_string_lossy(), &real.to_string_lossy()));
            // Both missing → still false, never panics.
            let missing2 = d.join("also-missing.txt");
            assert!(!same_volume(&missing.to_string_lossy(), &missing2.to_string_lossy()));
            let _ = fs::remove_dir_all(&d);
        }
    }

    #[cfg(windows)]
    mod windows_tests {
        use super::*;

        #[test]
        fn same_drive_case_insensitive_is_same_volume() {
            assert!(same_volume(r"C:\a", r"c:\b"));
            assert!(same_volume(r"C:\Users\foo", r"C:\Users\bar\baz.txt"));
        }

        #[test]
        fn different_drive_is_different_volume() {
            assert!(!same_volume(r"C:\a", r"D:\b"));
        }

        #[test]
        fn unc_shares_compare_by_prefix_case_insensitively() {
            assert!(same_volume(r"\\server\share\a", r"\\SERVER\SHARE\b"));
            assert!(!same_volume(r"\\server\share1\a", r"\\server\share2\b"));
        }

        #[test]
        fn path_without_a_prefix_is_never_confirmed_same_volume() {
            // A relative path has no `Prefix` component, so it can't be confirmed same-volume.
            assert!(!same_volume("relative\\path", r"C:\a"));
            assert!(!same_volume("relative\\a", "relative\\b"));
        }
    }
}
