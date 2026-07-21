//! Shared filesystem model types used across the explorer's commands (CPE-815): the directory-listing
//! [`DirEntry`], the Properties-dialog [`EntryInfo`], the sidebar [`Place`], and the bulk-operation
//! [`OpResult`], plus the pure `extension_of` / `is_hidden` helpers. Pure and Tauri-free; re-exported into
//! the app so its many construction/usage sites resolve unchanged.

use std::fs;
use std::path::Path;

use serde::Serialize;

/// One entry in a directory listing. Fields serialize by name to match the frontend `DirEntry`.
#[derive(Serialize)]
pub struct DirEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    /// Last-modified time as milliseconds since the Unix epoch. `None` when the platform or filesystem
    /// does not report one.
    pub modified: Option<u64>,
    /// Lowercased file extension without the dot ("png"), empty for directories and extensionless files.
    pub extension: String,
    /// Hidden per the OS convention: the hidden attribute on Windows, a leading dot on POSIX.
    pub hidden: bool,
}

/// Per-item outcome of a bulk operation. Bulk file operations must NOT be all-or-nothing and must not
/// abort on the first failure: if 9 of 10 files copy and one is locked, the user needs to know exactly
/// which one failed.
#[derive(Serialize, Debug)]
pub struct OpResult {
    pub path: String,
    pub ok: bool,
    pub error: String,
}

impl OpResult {
    pub fn ok(path: &Path) -> Self {
        Self {
            path: path.to_string_lossy().to_string(),
            ok: true,
            error: String::new(),
        }
    }
    pub fn err(path: &Path, e: impl std::fmt::Display) -> Self {
        Self {
            path: path.to_string_lossy().to_string(),
            ok: false,
            error: e.to_string(),
        }
    }
}

/// Detailed metadata for the Properties dialog.
#[derive(Serialize)]
pub struct EntryInfo {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<u64>,
    pub created: Option<u64>,
    pub readonly: bool,
    pub hidden: bool,
}

/// A sidebar quick-access location (special folder or drive).
#[derive(Serialize)]
pub struct Place {
    /// Display name, e.g. "Documents" or "Local Disk (C:)".
    pub name: String,
    pub path: String,
    /// Logical kind, used by the UI to pick an icon:
    /// "desktop" | "documents" | "downloads" | "pictures" | "music" | "videos" | "drive" | "home".
    pub kind: String,
}

/// Detailed metadata for the Properties dialog: name/size/dir + modified/created (epoch-ms) + the
/// readonly/hidden flags. A missing/unreadable path is an `Err`.
pub fn entry_info(path: &str) -> Result<EntryInfo, String> {
    let p = Path::new(path);
    let meta = fs::metadata(p).map_err(|e| format!("{path}: {e}"))?;
    Ok(EntryInfo {
        name: p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string()),
        path: path.to_string(),
        is_dir: meta.is_dir(),
        size: if meta.is_dir() { 0 } else { meta.len() },
        modified: meta.modified().ok().and_then(crate::fsutil::to_epoch_ms),
        created: meta.created().ok().and_then(crate::fsutil::to_epoch_ms),
        readonly: meta.permissions().readonly(),
        hidden: is_hidden(p, &meta),
    })
}

/// Lowercased extension without the dot; empty when there is none.
pub fn extension_of(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default()
}

/// Hidden per OS convention: the `FILE_ATTRIBUTE_HIDDEN` bit on Windows, a leading dot on POSIX.
pub fn is_hidden(path: &Path, meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
        if meta.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0 {
            return true;
        }
    }
    #[cfg(not(windows))]
    {
        let _ = meta;
    }
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with('.'))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_of_lowercases_and_handles_none() {
        assert_eq!(extension_of(Path::new("/a/b/Photo.PNG")), "png");
        assert_eq!(extension_of(Path::new("/a/b/archive.tar.gz")), "gz");
        assert_eq!(extension_of(Path::new("/a/b/README")), "");
    }

    #[test]
    fn entry_info_reports_metadata_and_errors_on_missing() {
        let dir = std::env::temp_dir().join(format!("cpe-entryinfo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("x.txt");
        std::fs::write(&f, b"hello").unwrap();
        let info = entry_info(&f.to_string_lossy()).unwrap();
        assert_eq!(info.name, "x.txt");
        assert!(!info.is_dir && info.size == 5);
        assert!(entry_info(&dir.join("nope").to_string_lossy()).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn op_result_constructors() {
        let ok = OpResult::ok(Path::new("/x/y.txt"));
        assert!(ok.ok && ok.error.is_empty());
        let err = OpResult::err(Path::new("/x/y.txt"), "locked");
        assert!(!err.ok && err.error == "locked");
    }

    #[test]
    fn is_hidden_by_dot_on_posix_paths() {
        let dir = std::env::temp_dir().join(format!("cpe-model-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // On POSIX the leading dot marks a file hidden; on Windows the dot alone doesn't, so only
        // assert the leading-dot direction where it holds.
        #[cfg(not(windows))]
        {
            let dotfile = dir.join(".secret");
            std::fs::write(&dotfile, b"x").unwrap();
            let meta = std::fs::metadata(&dotfile).unwrap();
            assert!(is_hidden(&dotfile, &meta));
        }
        // A plain file is never hidden on any platform.
        let plain = dir.join("plain.txt");
        std::fs::write(&plain, b"x").unwrap();
        assert!(!is_hidden(&plain, &std::fs::metadata(&plain).unwrap()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
