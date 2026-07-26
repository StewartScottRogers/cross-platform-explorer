//! Archive-vs-folder diff (CPE-1057, epic CPE-705): classify an archive's entries against a
//! destination folder's listing — "what's in this zip I don't already have" before extracting.
//! Pure, dependency-free: given the archive's [`archive::ArchiveEntry`] list and a caller-supplied
//! [`FolderEntry`] listing of the destination, match files by normalised relative path and bucket
//! each into [`DiffClass`]. No filesystem access here — the caller does the walking.

use crate::archive;

/// One entry on the destination-folder side of the diff (mirrors [`archive::ArchiveEntry`] but
/// named `rel_path` since it's already relative to the comparison root, not an archive-internal name).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct FolderEntry {
    pub rel_path: String,
    pub size: u64,
    pub is_dir: bool,
}

/// How one normalised relative path classifies between the archive and the folder.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case")]
pub enum DiffClass {
    /// Present in the archive but not in the folder — extracting it would add a new file.
    OnlyInArchive,
    /// Present in the folder but not in the archive.
    OnlyInFolder,
    /// Present on both sides under the same path, but the sizes differ.
    SizeDiffers,
    /// Present on both sides with the same size.
    Same,
}

/// The result of diffing an archive against a folder: normalised relative path → classification,
/// in deterministic (sorted-by-path) order.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ArchiveDiff {
    pub entries: Vec<(String, DiffClass)>,
}

/// Normalise a raw entry name/path for comparison: backslashes become forward slashes, and a
/// leading `./` is stripped. Deliberately string-based (not `std::path`) so the comparison is
/// identical on Linux, macOS, and Windows regardless of the host's native path semantics.
fn normalize(raw: &str) -> String {
    let s = raw.replace('\\', "/");
    s.strip_prefix("./").map(str::to_string).unwrap_or(s)
}

/// Diff an archive's entries against a destination folder's listing. Directory entries on either
/// side are excluded from the file comparison — a directory is never reported as `SizeDiffers`,
/// and a directory-only path doesn't count as "only in archive/folder" for file purposes. Matching
/// is by normalised relative path (`\` → `/`, leading `./` stripped) so `a\b.txt` and `a/b.txt`
/// match identically cross-platform. Ordering is deterministic: sorted by normalised path.
pub fn diff_archive(archive: &[archive::ArchiveEntry], folder: &[FolderEntry]) -> ArchiveDiff {
    use std::collections::BTreeMap;

    // BTreeMap keyed by normalised path gives deterministic (sorted) iteration for free.
    let mut archive_sizes: BTreeMap<String, u64> = BTreeMap::new();
    for e in archive {
        if e.is_dir {
            continue;
        }
        archive_sizes.insert(normalize(&e.name), e.size);
    }

    let mut folder_sizes: BTreeMap<String, u64> = BTreeMap::new();
    for e in folder {
        if e.is_dir {
            continue;
        }
        folder_sizes.insert(normalize(&e.rel_path), e.size);
    }

    let mut paths: Vec<&String> = archive_sizes.keys().chain(folder_sizes.keys()).collect();
    paths.sort();
    paths.dedup();

    let entries = paths
        .into_iter()
        .map(|path| {
            let class = match (archive_sizes.get(path), folder_sizes.get(path)) {
                (Some(_), None) => DiffClass::OnlyInArchive,
                (None, Some(_)) => DiffClass::OnlyInFolder,
                (Some(a), Some(f)) if a != f => DiffClass::SizeDiffers,
                (Some(_), Some(_)) => DiffClass::Same,
                (None, None) => unreachable!("path came from one of the two maps"),
            };
            (path.clone(), class)
        })
        .collect();

    ArchiveDiff { entries }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::ArchiveEntry;

    fn a(name: &str, size: u64, is_dir: bool) -> ArchiveEntry {
        ArchiveEntry { name: name.to_string(), size, is_dir }
    }

    fn f(rel_path: &str, size: u64, is_dir: bool) -> FolderEntry {
        FolderEntry { rel_path: rel_path.to_string(), size, is_dir }
    }

    #[test]
    fn only_in_archive_is_flagged() {
        let archive = [a("new.txt", 10, false)];
        let folder: [FolderEntry; 0] = [];
        let diff = diff_archive(&archive, &folder);
        assert_eq!(diff.entries, vec![("new.txt".to_string(), DiffClass::OnlyInArchive)]);
    }

    #[test]
    fn only_in_folder_is_flagged() {
        let archive: [ArchiveEntry; 0] = [];
        let folder = [f("local.txt", 5, false)];
        let diff = diff_archive(&archive, &folder);
        assert_eq!(diff.entries, vec![("local.txt".to_string(), DiffClass::OnlyInFolder)]);
    }

    #[test]
    fn size_differs_when_sizes_dont_match() {
        let archive = [a("a.txt", 100, false)];
        let folder = [f("a.txt", 50, false)];
        let diff = diff_archive(&archive, &folder);
        assert_eq!(diff.entries, vec![("a.txt".to_string(), DiffClass::SizeDiffers)]);
    }

    #[test]
    fn same_when_present_both_sides_with_equal_size() {
        let archive = [a("a.txt", 100, false)];
        let folder = [f("a.txt", 100, false)];
        let diff = diff_archive(&archive, &folder);
        assert_eq!(diff.entries, vec![("a.txt".to_string(), DiffClass::Same)]);
    }

    #[test]
    fn directory_entries_are_excluded_from_file_comparison() {
        // A directory present on only one side (or with mismatched "size") must never surface as
        // a file classification — dirs are dropped before comparison entirely.
        let archive = [a("sub/", 0, true), a("sub/keep.txt", 3, false)];
        let folder = [f("sub", 999, true), f("sub/keep.txt", 3, false)];
        let diff = diff_archive(&archive, &folder);
        assert_eq!(diff.entries, vec![("sub/keep.txt".to_string(), DiffClass::Same)]);
    }

    #[test]
    fn backslash_and_forward_slash_paths_match_via_normalization() {
        // Deliberately string-based normalisation (not std::path), so this holds identically on
        // Linux, macOS, AND Windows — no OS-dependent path semantics are exercised here.
        let archive = [a("a\\b.txt", 42, false)];
        let folder = [f("a/b.txt", 42, false)];
        let diff = diff_archive(&archive, &folder);
        assert_eq!(diff.entries, vec![("a/b.txt".to_string(), DiffClass::Same)]);
    }

    #[test]
    fn leading_dot_slash_is_stripped_before_matching() {
        let archive = [a("./a.txt", 7, false)];
        let folder = [f("a.txt", 7, false)];
        let diff = diff_archive(&archive, &folder);
        assert_eq!(diff.entries, vec![("a.txt".to_string(), DiffClass::Same)]);
    }

    #[test]
    fn identical_sets_are_all_same() {
        let archive = [a("a.txt", 1, false), a("b.txt", 2, false), a("dir/", 0, true)];
        let folder = [f("a.txt", 1, false), f("b.txt", 2, false), f("dir", 0, true)];
        let diff = diff_archive(&archive, &folder);
        assert_eq!(
            diff.entries,
            vec![
                ("a.txt".to_string(), DiffClass::Same),
                ("b.txt".to_string(), DiffClass::Same),
            ]
        );
    }

    #[test]
    fn empty_inputs_produce_an_empty_diff_without_panicking() {
        let archive: [ArchiveEntry; 0] = [];
        let folder: [FolderEntry; 0] = [];
        let diff = diff_archive(&archive, &folder);
        assert!(diff.entries.is_empty());
    }

    #[test]
    fn results_are_in_deterministic_sorted_order() {
        let archive = [a("z.txt", 1, false), a("a.txt", 1, false), a("m.txt", 1, false)];
        let folder: [FolderEntry; 0] = [];
        let diff = diff_archive(&archive, &folder);
        let paths: Vec<&str> = diff.entries.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(paths, vec!["a.txt", "m.txt", "z.txt"]);
    }
}
