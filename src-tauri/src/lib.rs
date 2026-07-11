use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize)]
pub struct DirEntry {
    name: String,
    path: String,
    is_dir: bool,
    size: u64,
    /// Last-modified time as milliseconds since the Unix epoch.
    /// `None` when the platform or filesystem does not report one.
    modified: Option<u64>,
    /// Lowercased file extension without the dot ("png"), empty for directories
    /// and extensionless files.
    extension: String,
}

#[derive(Serialize)]
pub struct Place {
    /// Display name, e.g. "Documents" or "Local Disk (C:)".
    name: String,
    path: String,
    /// Logical kind, used by the UI to pick an icon:
    /// "desktop" | "documents" | "downloads" | "pictures" | "music" | "videos" | "drive" | "home".
    kind: String,
}

/// Convert a `SystemTime` into epoch milliseconds, if representable.
fn to_epoch_ms(t: SystemTime) -> Option<u64> {
    t.duration_since(UNIX_EPOCH).ok().map(|d| d.as_millis() as u64)
}

/// Lowercased extension without the dot; empty when there is none.
fn extension_of(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default()
}

/// List the immediate children of `path`.
#[tauri::command]
fn list_dir(path: String) -> Result<Vec<DirEntry>, String> {
    let mut out = Vec::new();
    let read = fs::read_dir(&path).map_err(|e| format!("{path}: {e}"))?;
    for entry in read {
        // Skip entries we can't read rather than failing the whole listing.
        let Ok(entry) = entry else { continue };
        let Ok(meta) = entry.metadata() else { continue };

        let entry_path = entry.path();
        let is_dir = meta.is_dir();

        out.push(DirEntry {
            name: entry.file_name().to_string_lossy().to_string(),
            path: entry_path.to_string_lossy().to_string(),
            is_dir,
            size: if is_dir { 0 } else { meta.len() },
            modified: meta.modified().ok().and_then(to_epoch_ms),
            extension: if is_dir {
                String::new()
            } else {
                extension_of(&entry_path)
            },
        });
    }
    Ok(out)
}

/// Return the user's home directory.
#[tauri::command]
fn home_dir() -> Result<String, String> {
    dirs_home()
        .map(|p| p.to_string_lossy().to_string())
        .ok_or_else(|| "could not determine home directory".to_string())
}

/// Return the parent of `path`, or null if already at a root.
#[tauri::command]
fn parent_dir(path: String) -> Option<String> {
    Path::new(&path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
}

/// Available drives (Windows) or filesystem roots (Unix).
#[tauri::command]
fn list_drives() -> Vec<Place> {
    let mut drives = Vec::new();

    #[cfg(target_os = "windows")]
    {
        for letter in b'A'..=b'Z' {
            let root = format!("{}:\\", letter as char);
            if Path::new(&root).exists() {
                drives.push(Place {
                    name: format!("Local Disk ({}:)", letter as char),
                    path: root,
                    kind: "drive".to_string(),
                });
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        drives.push(Place {
            name: "File System".to_string(),
            path: "/".to_string(),
            kind: "drive".to_string(),
        });
    }

    drives
}

/// The OneDrive root, when OneDrive is configured (Windows sets %OneDrive%).
fn onedrive_root() -> Option<PathBuf> {
    std::env::var_os("OneDrive")
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
}

/// Resolve a known folder, trying the user profile first, then OneDrive.
///
/// Windows "Known Folder redirection" moves folders such as Pictures and
/// Documents into OneDrive, in which case `%USERPROFILE%\Pictures` does not
/// exist at all. Probing only the profile silently drops them (CPE-025).
///
/// We locate OneDrive via the `%OneDrive%` env var rather than the registry: a
/// registry crate would be Windows-only, so it would NOT be compiled by the
/// Linux CI job — an unverifiable dependency on a machine with no local Rust
/// toolchain.
fn resolve_known_folder(home: &Path, folder: &str) -> Option<PathBuf> {
    let in_profile = home.join(folder);
    if in_profile.is_dir() {
        return Some(in_profile);
    }
    let in_onedrive = onedrive_root()?.join(folder);
    if in_onedrive.is_dir() {
        return Some(in_onedrive);
    }
    None
}

/// The user's well-known folders. Only folders that actually exist are returned,
/// so the sidebar never shows a link that leads nowhere.
#[tauri::command]
fn special_folders() -> Vec<Place> {
    let Some(home) = dirs_home() else {
        return Vec::new();
    };

    let candidates = [
        ("Desktop", "desktop"),
        ("Documents", "documents"),
        ("Downloads", "downloads"),
        ("Pictures", "pictures"),
        ("Music", "music"),
        ("Videos", "videos"),
    ];

    candidates
        .iter()
        .filter_map(|(folder, kind)| {
            resolve_known_folder(&home, folder).map(|p| Place {
                name: (*folder).to_string(),
                path: p.to_string_lossy().to_string(),
                kind: (*kind).to_string(),
            })
        })
        .collect()
}

// Small cross-platform home-dir resolver without an extra dependency.
fn dirs_home() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init());

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        builder = builder
            .plugin(tauri_plugin_process::init())
            .plugin(tauri_plugin_updater::Builder::new().build());
    }

    builder
        .invoke_handler(tauri::generate_handler![
            list_dir,
            home_dir,
            parent_dir,
            list_drives,
            special_folders
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// NOTE: clippy's `items_after_test_module` lint requires the test module to be
// the LAST item in the file. Keep it here, at the bottom.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_dir_returns_the_parent() {
        assert_eq!(
            parent_dir("/home/user/docs".to_string()),
            Some("/home/user".to_string())
        );
    }

    #[test]
    fn parent_dir_at_root_returns_none() {
        assert_eq!(parent_dir("/".to_string()), None);
    }

    #[test]
    fn list_dir_errors_on_a_missing_path() {
        assert!(list_dir("/definitely/not/a/real/path/xyz".to_string()).is_err());
    }

    #[test]
    fn list_dir_lists_a_real_directory() {
        let dir = std::env::temp_dir();
        assert!(list_dir(dir.to_string_lossy().to_string()).is_ok());
    }

    #[test]
    fn home_dir_resolves() {
        assert!(home_dir().is_ok());
    }

    #[test]
    fn extension_is_lowercased_and_dotless() {
        assert_eq!(extension_of(Path::new("/a/b/Photo.PNG")), "png");
        assert_eq!(extension_of(Path::new("/a/b/archive.tar.gz")), "gz");
    }

    #[test]
    fn extension_is_empty_when_absent() {
        assert_eq!(extension_of(Path::new("/a/b/README")), "");
    }

    #[test]
    fn epoch_ms_of_unix_epoch_is_zero() {
        assert_eq!(to_epoch_ms(UNIX_EPOCH), Some(0));
    }

    #[test]
    fn epoch_ms_is_monotonic_for_later_times() {
        use std::time::Duration;
        let later = UNIX_EPOCH + Duration::from_millis(1_500);
        assert_eq!(to_epoch_ms(later), Some(1_500));
    }

    #[test]
    fn list_drives_returns_at_least_one_root() {
        assert!(!list_drives().is_empty(), "there is always at least one root");
    }

    #[test]
    fn special_folders_all_exist_and_are_labelled() {
        for place in special_folders() {
            assert!(Path::new(&place.path).is_dir(), "{} should exist", place.path);
            assert!(!place.kind.is_empty());
            assert!(!place.name.is_empty());
        }
    }

    #[test]
    fn known_folder_prefers_the_profile_location_when_it_exists() {
        // temp_dir() exists, so resolving a folder that lives directly under it
        // must return the profile-relative path.
        let tmp = std::env::temp_dir();
        let sub = tmp.join("cpe_known_folder_test");
        std::fs::create_dir_all(&sub).expect("create temp subdir");

        let found = resolve_known_folder(&tmp, "cpe_known_folder_test");
        assert_eq!(found, Some(sub.clone()));

        let _ = std::fs::remove_dir(&sub);
    }

    #[test]
    fn known_folder_returns_none_when_it_exists_nowhere() {
        let tmp = std::env::temp_dir();
        assert_eq!(
            resolve_known_folder(&tmp, "cpe_definitely_missing_folder_xyz"),
            None
        );
    }
}
