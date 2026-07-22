//! File checksums (CPE-412) + the recursive folder-checksum baseline for the integrity guard
//! (CPE-791, epic CPE-737). Pure and Tauri-free (CPE-815); the Tauri commands are thin
//! `spawn_blocking` dispatchers. Symlinks are not followed and unreadable files are skipped
//! (matching `dir_size`/`list_dir`).

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::fsutil::{sha256_file, to_epoch_ms};

/// SHA-256 the single file at `path` (lowercase hex). A directory is an `Err`.
pub fn hash_file(path: &str) -> Result<String, String> {
    let p = Path::new(path);
    if p.is_dir() {
        return Err(format!("{path}: is a folder"));
    }
    sha256_file(p).map_err(|e| format!("{path}: {e}"))
}

/// A file's checksum baseline entry (CPE-791) — matches the frontend `ChecksumEntry` (CPE-790).
/// `modified` is epoch-ms.
#[derive(Serialize, Deserialize)]
pub struct ChecksumEntry {
    path: String,
    sha256: String,
    size: u64,
    modified: Option<u64>,
}

/// Recursively checksum every file under `path` into a baseline manifest, sorted by path for a stable
/// diff. `path` must be a directory.
pub fn checksum_folder(path: &str) -> Result<Vec<ChecksumEntry>, String> {
    let p = Path::new(path);
    if !p.is_dir() {
        return Err(format!("{path}: not a folder"));
    }
    let mut out = Vec::new();
    checksum_walk(p, &mut out);
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

fn checksum_walk(dir: &Path, out: &mut Vec<ChecksumEntry>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return }; // skip folders we can't read
    for entry in entries.flatten() {
        // DirEntry::metadata() does not traverse symlinks, so a symlink is neither a dir nor a file
        // here and is skipped — matching `dir_size`'s "don't follow symlinks" behaviour.
        let Ok(meta) = entry.metadata() else { continue };
        let path = entry.path();
        if meta.is_dir() {
            checksum_walk(&path, out);
        } else if meta.is_file() {
            if let Ok(sha256) = sha256_file(&path) {
                out.push(ChecksumEntry {
                    path: path.to_string_lossy().to_string(),
                    sha256,
                    size: meta.len(),
                    modified: meta.modified().ok().and_then(to_epoch_ms),
                });
            }
        }
    }
}

/// Paths grouped by how a fresh scan changed relative to a baseline (CPE-870, epic CPE-737). Mirrors the
/// frontend `verifyManifest` (CPE-790): hash + mtime both moved → intended `edited`; hash moved but mtime
/// did NOT → silent `corrupted` (bitrot); baseline-only → `missing`; scan-only → `new`.
#[derive(Serialize, Default)]
pub struct IntegrityReport {
    pub intact: Vec<String>,
    pub edited: Vec<String>,
    pub corrupted: Vec<String>,
    pub missing: Vec<String>,
    pub new: Vec<String>,
}

/// Classify a fresh `current` scan against `baseline`, matched by path. Pure — the same heuristic as the
/// frontend model, so verification can run headlessly and ship only the (small) report instead of the whole
/// manifest across the IPC boundary.
pub fn verify_manifest(baseline: &[ChecksumEntry], current: &[ChecksumEntry]) -> IntegrityReport {
    use std::collections::{HashMap, HashSet};
    let cur: HashMap<&str, &ChecksumEntry> = current.iter().map(|e| (e.path.as_str(), e)).collect();
    let base_paths: HashSet<&str> = baseline.iter().map(|e| e.path.as_str()).collect();
    let mut report = IntegrityReport::default();
    for b in baseline {
        match cur.get(b.path.as_str()) {
            None => report.missing.push(b.path.clone()),
            Some(c) if c.sha256 == b.sha256 => report.intact.push(b.path.clone()),
            Some(c) if c.modified != b.modified => report.edited.push(b.path.clone()),
            Some(_) => report.corrupted.push(b.path.clone()),
        }
    }
    for c in current {
        if !base_paths.contains(c.path.as_str()) {
            report.new.push(c.path.clone());
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn entry(path: &str, sha: &str, modified: Option<u64>) -> ChecksumEntry {
        ChecksumEntry { path: path.into(), sha256: sha.into(), size: 0, modified }
    }

    #[test]
    fn verify_manifest_classifies_by_the_bitrot_heuristic() {
        let baseline = vec![
            entry("a", "h1", Some(10)), // unchanged
            entry("b", "h2", Some(20)), // edited: hash + mtime both move
            entry("c", "h3", Some(30)), // corrupted: hash moves, mtime does NOT
            entry("d", "h4", Some(40)), // missing from the new scan
        ];
        let current = vec![
            entry("a", "h1", Some(10)),
            entry("b", "h2b", Some(21)),
            entry("c", "h3b", Some(30)),
            entry("e", "h5", Some(50)), // new
        ];
        let r = verify_manifest(&baseline, &current);
        assert_eq!(r.intact, ["a"]);
        assert_eq!(r.edited, ["b"]);
        assert_eq!(r.corrupted, ["c"]);
        assert_eq!(r.missing, ["d"]);
        assert_eq!(r.new, ["e"]);
    }

    fn scratch(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-cksum-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn checksum_folder_hashes_files_recursively() {
        let d = scratch("cksum");
        fs::write(d.join("a.txt"), b"hello").unwrap();
        fs::create_dir(d.join("sub")).unwrap();
        fs::write(d.join("sub").join("b.txt"), b"world!!").unwrap();
        let manifest = checksum_folder(&d.to_string_lossy()).unwrap();
        assert_eq!(manifest.len(), 2, "one entry per file, recursing into subfolders");
        // sorted by path, and each hash matches sha256_file for that path
        assert!(manifest.windows(2).all(|w| w[0].path <= w[1].path));
        for e in &manifest {
            assert_eq!(e.sha256, sha256_file(Path::new(&e.path)).unwrap());
        }
        let a = manifest.iter().find(|e| e.path.ends_with("a.txt")).unwrap();
        assert_eq!(a.size, 5);
        assert!(a.modified.is_some());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn hash_file_matches_known_vector_and_rejects_folders_and_missing() {
        let d = scratch("hf");
        // The canonical SHA-256("abc") test vector.
        fs::write(d.join("abc.txt"), b"abc").unwrap();
        assert_eq!(
            hash_file(&d.join("abc.txt").to_string_lossy()).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        // A directory and a missing path are errors, not panics.
        assert!(hash_file(&d.to_string_lossy()).is_err());
        assert!(hash_file(&d.join("nope.txt").to_string_lossy()).is_err());
        let _ = fs::remove_dir_all(&d);
    }
}
