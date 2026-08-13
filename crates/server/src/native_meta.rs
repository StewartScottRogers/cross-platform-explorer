//! Native metadata I/O core (CPE-826, epic CPE-717): read/write/remove a **named metadata blob**
//! on a path using the OS-native mechanism, behind one cross-platform API.
//!
//! - **Windows** — an NTFS **alternate data stream** (`path:streamname`), plain file I/O, no extra
//!   dependency.
//! - **Unix** — a POSIX **extended attribute** (`user.<name>`), via the pure-Rust `xattr` crate.
//!
//! Filesystems that can't store native metadata (FAT/exFAT, or a kernel/mount without xattr) yield a
//! graceful [`MetaError::Unsupported`] — never a hard error that could fail a listing, matching the
//! `list_dir` skip-on-error spirit. This is only the storage primitive: the reconciliation with the
//! internal [`crate::tags`] store (CPE-827) and the UI surfacing (CPE-828) build on top. It stays
//! Tauri-free and headless-testable; CI's 3-OS `Server crates` job exercises both the ADS and xattr
//! paths.

use std::path::Path;

/// An error from a native-metadata operation.
#[derive(Debug)]
pub enum MetaError {
    /// The path's filesystem can't store native metadata at all (FAT/exFAT, or a kernel/mount
    /// without extended-attribute support). Callers degrade to "no native metadata".
    Unsupported,
    /// A genuine I/O error (missing base path, permission denied, …).
    Io(String),
}

impl std::fmt::Display for MetaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MetaError::Unsupported => write!(f, "native metadata is not supported on this filesystem"),
            MetaError::Io(e) => write!(f, "native metadata I/O error: {e}"),
        }
    }
}

impl std::error::Error for MetaError {}

/// The platform-native attribute/stream name for a CPE logical `key`, namespaced so CPE metadata
/// never collides with OS- or other-app metadata. Windows → an NTFS ADS name (`cpe.<key>`); Unix →
/// a `user.` extended attribute (`user.cpe.<key>`). Interop with foreign metadata (macOS Finder
/// tags, etc.) passes the raw native name directly to [`read`]/[`write`] instead (CPE-827).
pub fn cpe_name(key: &str) -> String {
    #[cfg(windows)]
    {
        format!("cpe.{key}")
    }
    #[cfg(not(windows))]
    {
        format!("user.cpe.{key}")
    }
}

/// Write `value` to the named native metadata blob on `path`, replacing any existing value. Does not
/// create the base path and does not alter the base file's contents.
pub fn write(path: &Path, name: &str, value: &[u8]) -> Result<(), MetaError> {
    imp::write(path, name, value)
}

/// Read the named native metadata blob: `Ok(Some(bytes))` when present, `Ok(None)` when the path
/// supports native metadata but this name is unset, `Err(Unsupported)` when the filesystem can't
/// store it, `Err(Io)` on a genuine error.
pub fn read(path: &Path, name: &str) -> Result<Option<Vec<u8>>, MetaError> {
    imp::read(path, name)
}

/// Remove the named native metadata blob. Idempotent: removing an absent blob succeeds.
pub fn remove(path: &Path, name: &str) -> Result<(), MetaError> {
    imp::remove(path, name)
}

/// Whether `path`'s filesystem can store native metadata, probed non-destructively (a read of an
/// unset CPE probe name). A best-effort hint for the UI's opt-in bridge toggle.
pub fn is_supported(path: &Path) -> bool {
    !matches!(read(path, &cpe_name("__probe__")), Err(MetaError::Unsupported))
}

// ---------------------------------------------------------------------------
// Windows — NTFS alternate data streams.
// ---------------------------------------------------------------------------
#[cfg(windows)]
mod imp {
    use super::MetaError;
    use std::ffi::OsString;
    use std::fs::{File, OpenOptions};
    use std::io::{Read, Write};
    use std::path::Path;

    // NTFS stream operations fail with these Win32 codes when the filesystem doesn't support named
    // streams (FAT/exFAT): ERROR_INVALID_PARAMETER (87) / ERROR_INVALID_NAME (123).
    const ERROR_FILE_NOT_FOUND: i32 = 2;
    const ERROR_PATH_NOT_FOUND: i32 = 3;
    const ERROR_INVALID_PARAMETER: i32 = 87;
    const ERROR_INVALID_NAME: i32 = 123;

    /// `C:\dir\file.txt` + `cpe.tags` → `C:\dir\file.txt:cpe.tags`.
    fn stream_path(path: &Path, name: &str) -> OsString {
        let mut s = path.as_os_str().to_os_string();
        s.push(":");
        s.push(name);
        s
    }

    fn map_open_err(e: std::io::Error) -> MetaError {
        match e.raw_os_error() {
            Some(ERROR_INVALID_PARAMETER) | Some(ERROR_INVALID_NAME) => MetaError::Unsupported,
            _ => MetaError::Io(e.to_string()),
        }
    }

    /// Turn a `try_exists()` outcome for `path` into the `require_present` error, or `None` meaning
    /// "proceed" (CPE-1692). Split out, mirroring `disk_usage::dir_size_stat_error`, so the
    /// `NotFound`-vs-everything-else split is unit-testable without touching a real filesystem
    /// (permission bits are platform- and privilege-dependent, so a real ACL-based test alone would
    /// leave this taxonomy unverified on some machines/CI accounts; this stays deterministic).
    ///
    /// Only a genuine `NotFound` says "no such path"; any other stat failure (permission denied, a dead
    /// network mount, …) names the OS's real cause instead — [`Path::exists`] used to swallow every
    /// `stat` failure into the same `false`, reporting "we don't know" as "it isn't there".
    pub(super) fn present_stat_error(path: &Path, stat: std::io::Result<bool>) -> Option<MetaError> {
        match stat {
            Ok(true) => None,
            Ok(false) => Some(MetaError::Io(format!("no such path: {}", path.display()))),
            Err(e) => Some(MetaError::Io(format!("{}: {e}", path.display()))),
        }
    }

    /// Guard shared by `write`/`read`/`remove`: never conjure the base file into existence, or open a
    /// stream on it, when it isn't confirmed present.
    fn require_present(path: &Path) -> Result<(), MetaError> {
        match present_stat_error(path, path.try_exists()) {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    pub fn write(path: &Path, name: &str, value: &[u8]) -> Result<(), MetaError> {
        require_present(path)?;
        let mut f = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(stream_path(path, name))
            .map_err(map_open_err)?;
        f.write_all(value).map_err(|e| MetaError::Io(e.to_string()))
    }

    pub fn read(path: &Path, name: &str) -> Result<Option<Vec<u8>>, MetaError> {
        require_present(path)?;
        match File::open(stream_path(path, name)) {
            Ok(mut f) => {
                let mut buf = Vec::new();
                f.read_to_end(&mut buf).map_err(|e| MetaError::Io(e.to_string()))?;
                Ok(Some(buf))
            }
            Err(e) => match e.raw_os_error() {
                // The stream simply isn't set on this (existing) file.
                Some(ERROR_FILE_NOT_FOUND) | Some(ERROR_PATH_NOT_FOUND) => Ok(None),
                Some(ERROR_INVALID_PARAMETER) | Some(ERROR_INVALID_NAME) => Err(MetaError::Unsupported),
                _ => Err(MetaError::Io(e.to_string())),
            },
        }
    }

    pub fn remove(path: &Path, name: &str) -> Result<(), MetaError> {
        require_present(path)?;
        match std::fs::remove_file(stream_path(path, name)) {
            Ok(()) => Ok(()),
            Err(e) => match e.raw_os_error() {
                // Already absent — idempotent.
                Some(ERROR_FILE_NOT_FOUND) | Some(ERROR_PATH_NOT_FOUND) => Ok(()),
                Some(ERROR_INVALID_PARAMETER) | Some(ERROR_INVALID_NAME) => Err(MetaError::Unsupported),
                _ => Err(MetaError::Io(e.to_string())),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Unix — POSIX extended attributes (via the `xattr` crate).
// ---------------------------------------------------------------------------
#[cfg(not(windows))]
mod imp {
    use super::MetaError;
    use std::path::Path;

    // ENOTSUP / EOPNOTSUPP: the filesystem doesn't support extended attributes (FAT/exFAT, or an
    // older-kernel tmpfs). Mapped to `Unsupported` so callers degrade gracefully.
    #[cfg(target_os = "macos")]
    const UNSUPPORTED_ERRNOS: &[i32] = &[45, 102]; // ENOTSUP, EOPNOTSUPP
    #[cfg(not(target_os = "macos"))]
    const UNSUPPORTED_ERRNOS: &[i32] = &[95]; // ENOTSUP == EOPNOTSUPP on Linux

    fn map_err(e: std::io::Error) -> MetaError {
        if let Some(code) = e.raw_os_error() {
            if UNSUPPORTED_ERRNOS.contains(&code) {
                return MetaError::Unsupported;
            }
        }
        MetaError::Io(e.to_string())
    }

    pub fn write(path: &Path, name: &str, value: &[u8]) -> Result<(), MetaError> {
        xattr::set(path, name, value).map_err(map_err)
    }

    pub fn read(path: &Path, name: &str) -> Result<Option<Vec<u8>>, MetaError> {
        // `xattr::get` maps a missing attribute (ENOATTR/ENODATA) to `Ok(None)`.
        xattr::get(path, name).map_err(map_err)
    }

    pub fn remove(path: &Path, name: &str) -> Result<(), MetaError> {
        match xattr::remove(path, name) {
            Ok(()) => Ok(()),
            // Removing an already-absent attribute errors (ENOATTR); treat "now absent" as success
            // so remove is idempotent. A missing path surfaces as `Io` via `read`'s `?`.
            Err(e) => {
                if read(path, name)?.is_none() {
                    Ok(())
                } else {
                    Err(map_err(e))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        // Uses the OS temp dir: NTFS on Windows / APFS on macOS (both store native metadata). On
        // Linux this may be tmpfs, which lacks xattr on older kernels — the round-trip test tolerates
        // that as `Unsupported` rather than flaking.
        let d = std::env::temp_dir().join(format!("cpe-nativemeta-{}-{}-{}", tag, std::process::id(), n));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn write_read_remove_round_trips() {
        let dir = scratch("rt");
        let f = dir.join("file.txt");
        std::fs::write(&f, b"base contents").unwrap();
        let name = cpe_name("tags");

        // If the filesystem can't store native metadata (e.g. tmpfs on an old kernel), that's a
        // valid environment, not a code failure: assert graceful degradation and stop.
        match write(&f, &name, b"work,urgent") {
            Ok(()) => {}
            Err(MetaError::Unsupported) => {
                assert!(!is_supported(&f), "an unsupported write implies an unsupported fs");
                let _ = std::fs::remove_dir_all(&dir);
                return;
            }
            Err(e) => panic!("unexpected write error: {e}"),
        }

        assert!(is_supported(&f));
        // Present → exact bytes back.
        assert_eq!(read(&f, &name).unwrap().as_deref(), Some(&b"work,urgent"[..]));
        // A different, unset name → absent (distinct from Unsupported/error).
        assert_eq!(read(&f, &cpe_name("other")).unwrap(), None);
        // The metadata write must not touch the base file's contents.
        assert_eq!(std::fs::read(&f).unwrap(), b"base contents");
        // Overwrite replaces the value.
        write(&f, &name, b"replaced").unwrap();
        assert_eq!(read(&f, &name).unwrap().as_deref(), Some(&b"replaced"[..]));
        // Remove → absent; removing again is idempotent.
        remove(&f, &name).unwrap();
        assert_eq!(read(&f, &name).unwrap(), None);
        remove(&f, &name).unwrap();

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_on_a_missing_path_is_an_io_error() {
        let dir = scratch("missing");
        let nope = dir.join("nope.txt");
        assert!(matches!(read(&nope, &cpe_name("tags")), Err(MetaError::Io(_))));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The deterministic half of the CPE-1692 guard (runs on every OS/account with Windows toolchain
    /// components, no privilege needed) — same role as `dispatch::classify_path_error`'s and
    /// `disk_usage::dir_size_stat_error`'s own unit tests. Windows-only because `present_stat_error` only
    /// exists inside `imp`'s `#[cfg(windows)]` block.
    #[cfg(windows)]
    #[test]
    fn present_stat_error_says_no_such_path_only_for_a_genuine_absence() {
        let p = Path::new(r"C:\some\file.txt");
        assert!(imp::present_stat_error(p, Ok(true)).is_none(), "an existing path proceeds, no error");
        match imp::present_stat_error(p, Ok(false)) {
            Some(MetaError::Io(msg)) => assert!(msg.contains("no such path"), "{msg}"),
            other => panic!("expected an absence Io error, got {other:?}"),
        }
        for kind in [std::io::ErrorKind::PermissionDenied, std::io::ErrorKind::Other, std::io::ErrorKind::TimedOut] {
            let e = std::io::Error::new(kind, "Access is denied.");
            match imp::present_stat_error(p, Err(e)) {
                Some(MetaError::Io(msg)) => {
                    assert!(!msg.contains("no such path"), "{kind:?} must not be reported as absence: {msg}");
                    assert!(msg.contains("Access is denied."), "{kind:?} must name the OS's own cause: {msg}");
                }
                other => panic!("{kind:?}: expected an Io error naming the real cause, got {other:?}"),
            }
        }
    }

    /// The end-to-end half, driving the real `write`/`read`/`remove` entry points rather than the pure
    /// classifier above. Constructed with a PARENT-directory traversal deny — the mechanism CPE-1687's
    /// brief names (a per-file deny ACE does not block `stat`, on either OS; see
    /// `fsutil::deny_dir_traversal`'s doc comment). MAY SKIP here: measured live that Windows's default
    /// "bypass traverse checking" privilege defeats a parent-directory deny for path resolution, so this
    /// leg's real coverage depends on the account/policy — the deterministic test above is what pins the
    /// taxonomy unconditionally.
    #[cfg(windows)]
    #[test]
    fn windows_ads_ops_report_the_real_cause_for_a_permission_denied_path_not_missing() {
        let dir = scratch("denied");
        let denied_dir = dir.join("denied");
        std::fs::create_dir_all(&denied_dir).unwrap();
        let inside = denied_dir.join("inside.txt");
        std::fs::write(&inside, b"base").unwrap();

        struct Restore<'a>(&'a Path, &'a Path);
        impl Drop for Restore<'_> {
            fn drop(&mut self) {
                crate::fsutil::undo_deny_dir_traversal(self.0);
                let _ = std::fs::remove_dir_all(self.1);
            }
        }
        let _restore = Restore(&denied_dir, &dir);

        if !crate::fsutil::deny_dir_traversal(&denied_dir, &inside) {
            use std::io::Write;
            let _ = writeln!(
                std::io::stderr(),
                "[CPE-1692] SKIPPED native_meta permission-denied leg: could not deny traversal on {} on \
                 this machine (elevated/admin, or a filesystem ignoring ACLs). The remaining assertions \
                 do NOT cover CPE-1692 for native_meta.",
                denied_dir.display()
            );
            return;
        }

        let name = cpe_name("tags");
        let write_err = write(&inside, &name, b"x").unwrap_err();
        let read_err = read(&inside, &name).unwrap_err();
        let remove_err = remove(&inside, &name).unwrap_err();
        for (label, err) in [("write", write_err), ("read", read_err), ("remove", remove_err)] {
            match err {
                MetaError::Io(msg) => assert!(
                    !msg.contains("no such path"),
                    "{label}: a permission-denied stat must not be reported as absence — the file is \
                     right there: {msg}"
                ),
                other => panic!("{label}: expected an Io error naming the real cause, got {other:?}"),
            }
        }
        // `_restore` cleans up on the way out, panic or not.
    }

    #[test]
    fn cpe_name_is_namespaced() {
        let n = cpe_name("tags");
        assert!(n.contains("cpe.tags"));
        #[cfg(not(windows))]
        assert!(n.starts_with("user."), "unix names live in the user namespace: {n}");
    }
}
