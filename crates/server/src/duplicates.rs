//! Duplicate-file finder (CPE-420, epic CPE-706). Efficient two-pass scan: group by size first (a
//! unique size can't be a duplicate), then SHA-256 only the size-collision candidates — most files are
//! never read. Pure and Tauri-free (CPE-815); the Tauri `find_duplicates` command dispatches here.

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;

use crate::fsutil::{entry_is_symlink, sha256_file};

/// A set of byte-identical files: their shared size + hash and every path.
#[derive(Serialize)]
pub struct DupGroup {
    size: u64,
    hash: String,
    paths: Vec<String>,
}

/// The result of a duplicate scan: the groups (largest reclaimable space first), how many files were
/// considered, and whether the file cap was hit.
#[derive(Serialize)]
pub struct DupResult {
    pub groups: Vec<DupGroup>,
    pub files_scanned: u64,
    pub truncated: bool,
}

const DUP_MAX_FILES: u64 = 50_000;

/// Find duplicate files under `root`. Skips dot-directories, symlinked dirs (cycle-safe), and empty
/// files; unreadable entries are skipped (never failing the scan); stops at a file cap (reporting
/// `truncated`). Groups are sorted by reclaimable space (largest first). A non-folder root is an `Err`.
pub fn find_duplicates(root: &str) -> Result<DupResult, String> {
    let root_path = Path::new(root);
    if !root_path.is_dir() {
        return Err(format!("{root}: not a folder"));
    }

    // Pass 1: group candidate files by size (skip-on-error like `list_dir`).
    let mut by_size: HashMap<u64, Vec<std::path::PathBuf>> = HashMap::new();
    let mut files_scanned = 0u64;
    let mut truncated = false;
    let mut stack = vec![root_path.to_path_buf()];
    'walk: while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                // Skip dot-dirs and symlinked dirs (avoid cycles, CPE-611).
                if !name.to_string_lossy().starts_with('.') && !entry_is_symlink(&entry) {
                    stack.push(path);
                }
                continue;
            }
            if !meta.is_file() || meta.len() == 0 {
                continue; // empty files are all "equal" — not useful to report
            }
            if files_scanned >= DUP_MAX_FILES {
                truncated = true;
                break 'walk;
            }
            files_scanned += 1;
            by_size.entry(meta.len()).or_default().push(path);
        }
    }

    // Pass 2: within each size collision, hash the candidates and group identical content.
    let mut groups: Vec<DupGroup> = Vec::new();
    for (size, paths) in by_size {
        if paths.len() < 2 {
            continue;
        }
        let mut by_hash: HashMap<String, Vec<String>> = HashMap::new();
        for p in &paths {
            if let Ok(h) = sha256_file(p) {
                by_hash.entry(h).or_default().push(p.to_string_lossy().into_owned());
            }
        }
        for (hash, group_paths) in by_hash {
            if group_paths.len() > 1 {
                groups.push(DupGroup { size, hash, paths: group_paths });
            }
        }
    }

    // Largest reclaimable space first: size × (copies − 1).
    groups.sort_by_key(|g| std::cmp::Reverse(g.size * (g.paths.len() as u64 - 1)));
    Ok(DupResult { groups, files_scanned, truncated })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-dups-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn find_duplicates_groups_identical_files_and_ignores_unique_sizes() {
        let d = scratch("dups");
        fs::create_dir_all(d.join("sub")).unwrap();
        // Three identical files across subfolders (a 3-way group).
        for n in ["one.txt", "sub/two.txt", "sub/three.txt"] {
            fs::write(d.join(n), b"duplicate payload").unwrap();
        }
        // A same-SIZE-but-different file — must NOT group with the above.
        fs::write(d.join("decoy.txt"), b"DUPLICATE payloaD").unwrap(); // 17 bytes, like the others
        // A unique file — never hashed, never grouped.
        fs::write(d.join("unique.txt"), b"i am one of a kind").unwrap();
        // Empty files are ignored.
        fs::write(d.join("empty1"), b"").unwrap();
        fs::write(d.join("empty2"), b"").unwrap();

        let r = find_duplicates(&d.to_string_lossy()).unwrap();
        assert_eq!(r.groups.len(), 1, "exactly one duplicate group");
        let g = &r.groups[0];
        assert_eq!(g.paths.len(), 3, "the 3-way group");
        assert_eq!(g.size, 17);
        let names: Vec<String> = g.paths.iter().map(|p| p.replace('\\', "/")).collect();
        assert!(names.iter().any(|p| p.ends_with("one.txt")));
        assert!(names.iter().any(|p| p.ends_with("sub/two.txt")));
        assert!(!names.iter().any(|p| p.ends_with("decoy.txt")));

        // No-duplicate folder → empty; a non-folder root → Err.
        let d2 = scratch("nodups");
        fs::write(d2.join("only.txt"), b"solo").unwrap();
        assert!(find_duplicates(&d2.to_string_lossy()).unwrap().groups.is_empty());
        assert!(find_duplicates(&d.join("one.txt").to_string_lossy()).is_err());
        let _ = fs::remove_dir_all(&d);
        let _ = fs::remove_dir_all(&d2);
    }
}
