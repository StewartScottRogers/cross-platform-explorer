//! Folder statistics (CPE-649): recursive file count, sub-folder count, and total bytes for a folder.
//! Cycle-safe (doesn't follow symlinked dirs, CPE-609/611) and bounded by an entry cap so it can't spin
//! on a pathological tree. Pure and Tauri-free (CPE-815); the Tauri `folder_stats` command dispatches.

use std::fs;
use std::path::Path;

use serde::Serialize;

use crate::fsutil::entry_is_symlink;

/// Recursive folder totals. Serialized to match the frontend `FolderStats`.
#[derive(Serialize, Default)]
pub struct FolderStats {
    files: u64,
    dirs: u64,
    bytes: u64,
    truncated: bool,
}

const FOLDER_STATS_MAX_ENTRIES: u64 = 500_000;

/// Count files/dirs/bytes under `path`. `path` must be a directory. Beyond the entry cap the walk stops
/// and `truncated` is set. Symlinked dirs are not descended (cycle-safe); unreadable dirs are skipped.
pub fn compute(path: &str) -> Result<FolderStats, String> {
    let root = Path::new(path);
    if !root.is_dir() {
        return Err(format!("{path}: not a folder"));
    }
    let mut stats = FolderStats::default();
    let mut seen = 0u64;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            seen += 1;
            if seen > FOLDER_STATS_MAX_ENTRIES {
                stats.truncated = true;
                return Ok(stats);
            }
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                stats.dirs += 1;
                if !entry_is_symlink(&entry) {
                    stack.push(entry.path());
                }
            } else {
                stats.files += 1;
                stats.bytes += meta.len();
            }
        }
    }
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-folderstats-{}-{}", std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn folder_stats_counts_files_dirs_and_bytes() {
        let d = scratch();
        fs::write(d.join("a.bin"), b"12345").unwrap();
        fs::create_dir(d.join("sub")).unwrap();
        fs::write(d.join("sub").join("b.bin"), b"678").unwrap();
        let s = compute(&d.to_string_lossy()).unwrap();
        assert_eq!((s.files, s.dirs, s.bytes, s.truncated), (2, 1, 8, false));
        // A file (not a folder) is an error.
        assert!(compute(&d.join("a.bin").to_string_lossy()).is_err());
        let _ = fs::remove_dir_all(&d);
    }
}
