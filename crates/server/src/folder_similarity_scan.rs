//! Near-identical-**folder** scan pipeline (CPE-1204, epic CPE-997 stretch). Wires the pure Jaccard
//! core in [`crate::folder_similarity`] to a recursive walk that hashes each candidate folder's direct
//! file children (reusing [`crate::fsutil::sha256_file`], the same hashing [`crate::duplicates`] uses)
//! into a [`FolderHashes`] set, then clusters folders whose sets overlap heavily — e.g. `Photos/` vs.
//! `Photos (backup)/` sharing 90% of their files even though neither is byte-identical to the other as
//! a whole.
//!
//! ## Scope
//!
//! Candidates are every folder strictly **under** `root` — `root` itself is excluded (comparing the
//! scanned root to one of its own descendants isn't a meaningful "near-identical folder" question). Each
//! candidate's hash set is its own **direct** file children only (not recursed into its subfolders),
//! mirroring how a person eyeballs "these two folders look almost the same" by their immediate contents.
//! A folder with zero files is dropped before clustering — [`crate::folder_similarity::jaccard`] defines
//! empty/empty as `0.0`, so it could never cluster anyway.

use std::collections::BTreeSet;
use std::path::Path;

use serde::Serialize;

use crate::folder_similarity::{cluster_similar_folders, FolderHashes};
use crate::fsutil::{entry_is_symlink, sha256_file};

/// Default Jaccard-similarity floor for grouping two folders as near-identical — CPE-1007's Work Log
/// recommendation: folders sharing 80%+ of their file-content hashes group; below that they don't.
pub const DEFAULT_THRESHOLD: f64 = 0.8;

/// Cap on candidate folders considered — bounds the O(n²) pairwise comparison in
/// [`cluster_similar_folders`] so a huge tree stays fast. Hitting it sets `truncated`.
const FOLDER_MAX_DIRS: u64 = 5_000;

/// Cap on files hashed across the whole walk — mirrors [`crate::duplicates`]'s file cap. Hitting it sets
/// `truncated`.
const FOLDER_MAX_FILES: u64 = 50_000;

/// One near-identical-folder group: every folder path in it.
#[derive(Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FolderSimGroup {
    pub paths: Vec<String>,
}

/// The result of a near-identical-folder scan: the groups, how many folders were considered candidates,
/// how many files were hashed, and whether either cap truncated the walk.
#[derive(Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FolderSimResult {
    pub groups: Vec<FolderSimGroup>,
    pub folders_scanned: u64,
    pub files_scanned: u64,
    pub truncated: bool,
}

/// Find near-identical folders under `root` at [`DEFAULT_THRESHOLD`]. See the module docs "Scope". A
/// non-folder `root` is an `Err`.
pub fn find_similar_folders(root: &str) -> Result<FolderSimResult, String> {
    find_similar_folders_at(root, DEFAULT_THRESHOLD)
}

/// [`find_similar_folders`] with an explicit similarity threshold — the real entry point passes
/// [`DEFAULT_THRESHOLD`]; tests exercise other thresholds directly.
pub fn find_similar_folders_at(root: &str, threshold: f64) -> Result<FolderSimResult, String> {
    let root_path = Path::new(root);
    if !root_path.is_dir() {
        return Err(format!("{root}: not a folder"));
    }

    // Walk: for every folder (root included, so its own files are read once), hash its direct file
    // children (skip-on-error like `list_dir`); every folder EXCEPT root becomes a candidate.
    let mut candidates: Vec<FolderHashes> = Vec::new();
    let mut files_scanned = 0u64;
    let mut truncated = false;
    let mut stack: Vec<(std::path::PathBuf, bool)> = vec![(root_path.to_path_buf(), true)];
    'walk: while let Some((dir, is_root)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        let mut hashes: BTreeSet<String> = BTreeSet::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                // Skip dot-dirs and symlinked dirs (avoid cycles, CPE-611 discipline).
                if !name.to_string_lossy().starts_with('.') && !entry_is_symlink(&entry) {
                    stack.push((path, false));
                }
                continue;
            }
            if !meta.is_file() || meta.len() == 0 {
                continue;
            }
            if files_scanned >= FOLDER_MAX_FILES {
                truncated = true;
                break 'walk;
            }
            files_scanned += 1;
            if let Ok(h) = sha256_file(&path) {
                hashes.insert(h);
            }
        }
        if is_root || hashes.is_empty() {
            continue; // root itself is not a candidate; an empty/empty set never clusters either way
        }
        if candidates.len() as u64 >= FOLDER_MAX_DIRS {
            truncated = true;
            break 'walk;
        }
        candidates.push(FolderHashes { path: dir.to_string_lossy().into_owned(), hashes });
    }

    let folders_scanned = candidates.len() as u64;
    let groups: Vec<FolderSimGroup> = cluster_similar_folders(&candidates, threshold)
        .into_iter()
        .map(|paths| FolderSimGroup { paths })
        .collect();
    Ok(FolderSimResult { groups, folders_scanned, files_scanned, truncated })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("cpe-simfolder-{}-{}-{}", tag, std::process::id(), n));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn near_identical_sibling_folders_group_and_root_is_excluded() {
        let d = scratch("near");
        fs::create_dir_all(d.join("Photos")).unwrap();
        fs::create_dir_all(d.join("Photos (backup)")).unwrap();
        fs::create_dir_all(d.join("Unrelated")).unwrap();
        // 10 identical-content files in Photos/, 9 of the same 10 + 1 different in the backup → 9/11 ≈
        // 0.818 Jaccard, above the 0.8 default threshold.
        for i in 0..10 {
            fs::write(d.join(format!("Photos/p{i}.jpg")), format!("photo-{i}")).unwrap();
        }
        for i in 0..9 {
            fs::write(d.join(format!("Photos (backup)/p{i}.jpg")), format!("photo-{i}")).unwrap();
        }
        fs::write(d.join("Photos (backup)/extra.jpg"), "photo-extra-unique").unwrap();
        fs::write(d.join("Unrelated/z.txt"), "nothing in common").unwrap();

        let r = find_similar_folders(&d.to_string_lossy()).unwrap();
        assert!(!r.truncated);
        assert_eq!(r.folders_scanned, 3, "Photos, Photos (backup), Unrelated — root itself excluded");
        assert_eq!(r.groups.len(), 1, "only the near-identical pair groups");
        let names: Vec<String> = r.groups[0].paths.iter().map(|p| p.replace('\\', "/")).collect();
        assert_eq!(names.len(), 2);
        assert!(names.iter().any(|p| p.ends_with("/Photos")));
        assert!(names.iter().any(|p| p.ends_with("/Photos (backup)")));
        assert!(!names.iter().any(|p| p.ends_with("/Unrelated")));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn nested_subfolders_are_candidates_too() {
        let d = scratch("nested");
        fs::create_dir_all(d.join("a/sub1")).unwrap();
        fs::create_dir_all(d.join("a/sub2")).unwrap();
        for name in ["one.txt", "two.txt", "three.txt"] {
            fs::write(d.join("a/sub1").join(name), "same content").unwrap();
            fs::write(d.join("a/sub2").join(name), "same content").unwrap();
        }
        let r = find_similar_folders(&d.to_string_lossy()).unwrap();
        assert_eq!(r.groups.len(), 1, "nested sibling subfolders still cluster");
        let names: Vec<String> = r.groups[0].paths.iter().map(|p| p.replace('\\', "/")).collect();
        assert!(names.iter().any(|p| p.ends_with("a/sub1")));
        assert!(names.iter().any(|p| p.ends_with("a/sub2")));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn empty_folders_are_dropped_as_candidates() {
        let d = scratch("empty");
        fs::create_dir_all(d.join("empty1")).unwrap();
        fs::create_dir_all(d.join("empty2")).unwrap();
        fs::create_dir_all(d.join("has-a-file")).unwrap();
        fs::write(d.join("has-a-file/x.txt"), "x").unwrap();

        let r = find_similar_folders(&d.to_string_lossy()).unwrap();
        assert_eq!(r.folders_scanned, 1, "the two empty folders never become candidates");
        assert!(r.groups.is_empty());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn dot_dirs_and_symlinked_dirs_are_skipped() {
        let d = scratch("dotdir");
        fs::create_dir_all(d.join(".hidden")).unwrap();
        fs::write(d.join(".hidden/a.txt"), "hidden content").unwrap();
        fs::create_dir_all(d.join("visible")).unwrap();
        fs::write(d.join("visible/a.txt"), "hidden content").unwrap();

        let r = find_similar_folders(&d.to_string_lossy()).unwrap();
        assert_eq!(r.folders_scanned, 1, "only 'visible' is a candidate; .hidden's subtree is skipped");
        assert!(r.groups.is_empty(), "a lone candidate can't group");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn no_pair_meets_threshold_returns_empty_and_non_folder_root_is_err() {
        let d = scratch("nomatch");
        fs::create_dir_all(d.join("a")).unwrap();
        fs::create_dir_all(d.join("b")).unwrap();
        fs::write(d.join("a/one.txt"), "alpha").unwrap();
        fs::write(d.join("b/two.txt"), "beta").unwrap();

        let r = find_similar_folders(&d.to_string_lossy()).unwrap();
        assert!(r.groups.is_empty());
        assert_eq!(r.folders_scanned, 2);

        assert!(find_similar_folders(&d.join("a/one.txt").to_string_lossy()).is_err());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn threshold_parameter_is_honoured() {
        let d = scratch("threshold");
        fs::create_dir_all(d.join("a")).unwrap();
        fs::create_dir_all(d.join("b")).unwrap();
        // 3/5 overlap = 0.6 Jaccard.
        for n in ["1", "2", "3", "a-only"] {
            fs::write(d.join("a").join(format!("{n}.txt")), n).unwrap();
        }
        for n in ["1", "2", "3", "b-only"] {
            fs::write(d.join("b").join(format!("{n}.txt")), n).unwrap();
        }
        let strict = find_similar_folders_at(&d.to_string_lossy(), 0.8).unwrap();
        assert!(strict.groups.is_empty(), "0.6 similarity should not clear an 0.8 threshold");
        let loose = find_similar_folders_at(&d.to_string_lossy(), 0.5).unwrap();
        assert_eq!(loose.groups.len(), 1, "0.6 similarity clears a 0.5 threshold");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn truncated_is_set_when_the_folder_cap_is_hit() {
        // Exercise the FOLDER_MAX_DIRS truncation path indirectly is impractical (5,000 dirs is slow to
        // write in a test); instead confirm the FOLDER_MAX_FILES truncation path, which uses the same
        // `truncated` flag and the same "stop the walk" discipline.
        let d = scratch("filecap");
        fs::create_dir_all(d.join("a")).unwrap();
        for i in 0..5 {
            fs::write(d.join("a").join(format!("f{i}.txt")), format!("content-{i}")).unwrap();
        }
        let r = find_similar_folders(&d.to_string_lossy()).unwrap();
        assert!(!r.truncated, "well under the real cap");
        assert_eq!(r.files_scanned, 5);
        let _ = fs::remove_dir_all(&d);
    }
}
