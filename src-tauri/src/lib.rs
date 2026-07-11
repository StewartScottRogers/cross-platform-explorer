use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize)]
pub struct DirEntry {
    name: String,
    path: String,
    is_dir: bool,
    size: u64,
}

/// List the immediate children of `path`.
#[tauri::command]
fn list_dir(path: String) -> Result<Vec<DirEntry>, String> {
    let mut out = Vec::new();
    let read = fs::read_dir(&path).map_err(|e| format!("{path}: {e}"))?;
    for entry in read {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue, // skip entries we can't read
        };
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        out.push(DirEntry {
            name: entry.file_name().to_string_lossy().to_string(),
            path: entry.path().to_string_lossy().to_string(),
            is_dir: meta.is_dir(),
            size: if meta.is_dir() { 0 } else { meta.len() },
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_dir_returns_the_parent() {
        let parent = parent_dir("/home/user/docs".to_string());
        assert_eq!(parent, Some("/home/user".to_string()));
    }

    #[test]
    fn parent_dir_at_root_returns_none() {
        assert_eq!(parent_dir("/".to_string()), None);
    }

    #[test]
    fn list_dir_errors_on_a_missing_path() {
        let result = list_dir("/definitely/not/a/real/path/xyz".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn list_dir_lists_a_real_directory() {
        let dir = std::env::temp_dir();
        let result = list_dir(dir.to_string_lossy().to_string());
        assert!(result.is_ok(), "temp dir should be listable");
    }

    #[test]
    fn home_dir_resolves() {
        assert!(home_dir().is_ok(), "home directory should resolve");
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
            list_dir, home_dir, parent_dir
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
