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

    pub fn write(path: &Path, name: &str, value: &[u8]) -> Result<(), MetaError> {
        // Never conjure the base file into existence by writing a stream to a missing path.
        if !path.exists() {
            return Err(MetaError::Io(format!("no such path: {}", path.display())));
        }
        let mut f = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(stream_path(path, name))
            .map_err(map_open_err)?;
        f.write_all(value).map_err(|e| MetaError::Io(e.to_string()))
    }

    pub fn read(path: &Path, name: &str) -> Result<Option<Vec<u8>>, MetaError> {
        if !path.exists() {
            return Err(MetaError::Io(format!("no such path: {}", path.display())));
        }
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
        if !path.exists() {
            return Err(MetaError::Io(format!("no such path: {}", path.display())));
        }
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

    #[test]
    fn cpe_name_is_namespaced() {
        let n = cpe_name("tags");
        assert!(n.contains("cpe.tags"));
        #[cfg(not(windows))]
        assert!(n.starts_with("user."), "unix names live in the user namespace: {n}");
    }
}
