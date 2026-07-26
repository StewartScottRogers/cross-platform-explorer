//! Extract planner (CPE-1055, epic CPE-705 "Archive & compression suite"): given an archive's entry
//! list and the destination's existing entry names, compute exactly what an extract *would* do —
//! without opening the archive or touching the filesystem. Resolves each entry's normalised
//! destination path, rejects zip-slip escapes, flags name collisions with what's already on disk,
//! collects the directories that would need creating (deduped, parents before children), and sums
//! file count / total uncompressed bytes.
//!
//! Pure over caller-supplied [`crate::archive::ArchiveEntry`] data; no I/O, no new dependencies.
//! `total_uncompressed` (and each [`PlannedEntry::size`]) is shaped to feed straight into
//! [`crate::archive_safety::expansion_ratio`] for a zip-bomb cross-check before an extract proceeds.
//!
//! Reuses the archive module's existing, already-tested zip-slip guard
//! ([`crate::archive::entry_name_is_safe`]) rather than duplicating the traversal/absolute-path
//! check here.

use std::collections::HashSet;

use crate::archive::{entry_name_is_safe, ArchiveEntry};

/// One safe archive entry, resolved to where it would land on disk.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct PlannedEntry {
    /// The entry's raw name as it appears in the archive.
    pub archive_name: String,
    /// The normalised (`/`-separated, no leading `./`) path it would be written to, relative to the
    /// extraction root.
    pub dest_rel: String,
    /// True if `dest_rel` already names something at the destination (per `existing_dest`).
    pub collides: bool,
    /// Uncompressed size in bytes, carried straight from the archive entry.
    pub size: u64,
}

/// The full result of planning an extract: what would be written, what was rejected, what
/// directories need creating first, and running totals.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub struct ExtractPlan {
    /// Safe, plannable file entries (directory entries are folded into `dirs_to_create` instead).
    pub files: Vec<PlannedEntry>,
    /// Raw archive names rejected as zip-slip (absolute path, `..` traversal, drive-letter escape).
    pub skipped_unsafe: Vec<String>,
    /// Directories that would need creating, deduped, ordered parents-before-children.
    pub dirs_to_create: Vec<String>,
    /// Count of planned (non-directory) files — i.e. `files.len()`.
    pub file_count: usize,
    /// Sum of `size` across `files`, saturating (a crafted archive could claim sizes that overflow a
    /// running `u64` sum; saturating only ever makes the total look *more* alarming, never masks it).
    pub total_uncompressed: u64,
}

/// Split a normalised (`/`-separated, no leading/trailing slash) `path` into its ancestor
/// directories, top-down and excluding `path` itself — e.g. `"a/b/c.txt"` -> `["a", "a/b"]`. A
/// bare leaf name (no `/`) has no ancestors and yields an empty vec.
fn ancestor_dirs(path: &str) -> Vec<String> {
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if segments.len() <= 1 {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(segments.len() - 1);
    let mut acc = String::new();
    for seg in &segments[..segments.len() - 1] {
        if !acc.is_empty() {
            acc.push('/');
        }
        acc.push_str(seg);
        out.push(acc.clone());
    }
    out
}

/// Append `dir` to `dirs` if it isn't already present (keeps `dirs_to_create` deduped).
fn push_dir_once(dirs: &mut Vec<String>, dir: String) {
    if !dirs.contains(&dir) {
        dirs.push(dir);
    }
}

/// Normalise an archive entry name into a `/`-separated relative path with no leading `./` and no
/// trailing slash (archives commonly store directory entries with one, e.g. `"foo/"`). Assumes the
/// name already passed [`entry_name_is_safe`].
fn normalize_dest(name: &str) -> String {
    let mut s = name.replace('\\', "/");
    while let Some(stripped) = s.strip_prefix("./") {
        s = stripped.to_string();
    }
    while s.ends_with('/') {
        s.pop();
    }
    s
}

/// Plan an extract of `entries` into a destination whose current entries (relative paths) are
/// `existing_dest`. Never opens an archive or touches the filesystem — pure over the entry list.
pub fn plan_extract(entries: &[ArchiveEntry], existing_dest: &[String]) -> ExtractPlan {
    let existing: HashSet<&str> = existing_dest.iter().map(String::as_str).collect();

    let mut files = Vec::new();
    let mut skipped_unsafe = Vec::new();
    let mut dirs_to_create: Vec<String> = Vec::new();
    let mut file_count: usize = 0;
    let mut total_uncompressed: u64 = 0;

    for entry in entries {
        if !entry_name_is_safe(&entry.name) {
            skipped_unsafe.push(entry.name.clone());
            continue;
        }

        let dest_rel = normalize_dest(&entry.name);
        if dest_rel.is_empty() {
            // Normalised away to nothing (e.g. a bare "./" entry) — nothing sane to plan; treat as
            // unsafe/unplannable rather than emitting an empty path.
            skipped_unsafe.push(entry.name.clone());
            continue;
        }

        for ancestor in ancestor_dirs(&dest_rel) {
            push_dir_once(&mut dirs_to_create, ancestor);
        }

        if entry.is_dir {
            push_dir_once(&mut dirs_to_create, dest_rel);
            continue;
        }

        let collides = existing.contains(dest_rel.as_str());
        total_uncompressed = total_uncompressed.saturating_add(entry.size);
        file_count += 1;
        files.push(PlannedEntry { archive_name: entry.name.clone(), dest_rel, collides, size: entry.size });
    }

    ExtractPlan { files, skipped_unsafe, dirs_to_create, file_count, total_uncompressed }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, size: u64) -> ArchiveEntry {
        ArchiveEntry { name: name.to_string(), size, is_dir: false }
    }

    fn dir(name: &str) -> ArchiveEntry {
        ArchiveEntry { name: name.to_string(), size: 0, is_dir: true }
    }

    #[test]
    fn unsafe_entries_are_rejected_into_skipped_unsafe() {
        let entries = vec![file("../evil", 10), file("/abs/path", 10), file("C:\\x", 10), file("ok.txt", 5)];
        let plan = plan_extract(&entries, &[]);

        assert_eq!(plan.skipped_unsafe, vec!["../evil", "/abs/path", "C:\\x"]);
        assert_eq!(plan.files.len(), 1);
        assert_eq!(plan.files[0].archive_name, "ok.txt");
        assert_eq!(plan.file_count, 1);
        assert_eq!(plan.total_uncompressed, 5);
    }

    #[test]
    fn dest_name_already_present_is_flagged_as_colliding() {
        let entries = vec![file("keep.txt", 1), file("dupe.txt", 2)];
        let existing = vec!["dupe.txt".to_string(), "other.txt".to_string()];
        let plan = plan_extract(&entries, &existing);

        let keep = plan.files.iter().find(|e| e.dest_rel == "keep.txt").unwrap();
        let dupe = plan.files.iter().find(|e| e.dest_rel == "dupe.txt").unwrap();
        assert!(!keep.collides);
        assert!(dupe.collides);
    }

    #[test]
    fn dirs_to_create_are_deduped_and_parents_precede_children() {
        let entries = vec![
            file("a/b/c/one.txt", 1),
            file("a/b/two.txt", 1),
            file("a/b/c/three.txt", 1),
            file("top.txt", 1),
        ];
        let plan = plan_extract(&entries, &[]);

        assert_eq!(plan.dirs_to_create, vec!["a".to_string(), "a/b".to_string(), "a/b/c".to_string()]);
        // No duplicates even though "a" and "a/b" are ancestors of multiple entries.
        let unique: HashSet<&String> = plan.dirs_to_create.iter().collect();
        assert_eq!(unique.len(), plan.dirs_to_create.len());
    }

    #[test]
    fn explicit_directory_entries_are_folded_into_dirs_to_create_not_files() {
        let entries = vec![dir("photos/"), dir("photos/2026/"), file("photos/2026/a.jpg", 100)];
        let plan = plan_extract(&entries, &[]);

        assert_eq!(plan.dirs_to_create, vec!["photos".to_string(), "photos/2026".to_string()]);
        assert_eq!(plan.files.len(), 1);
        assert_eq!(plan.file_count, 1);
        assert_eq!(plan.total_uncompressed, 100);
    }

    #[test]
    fn file_count_and_total_uncompressed_sum_correctly() {
        let entries = vec![file("a.txt", 10), file("b.txt", 20), file("c/d.txt", 30)];
        let plan = plan_extract(&entries, &[]);

        assert_eq!(plan.file_count, 3);
        assert_eq!(plan.total_uncompressed, 60);
    }

    #[test]
    fn empty_archive_yields_empty_plan_without_panicking() {
        let plan = plan_extract(&[], &[]);

        assert!(plan.files.is_empty());
        assert!(plan.skipped_unsafe.is_empty());
        assert!(plan.dirs_to_create.is_empty());
        assert_eq!(plan.file_count, 0);
        assert_eq!(plan.total_uncompressed, 0);
    }

    #[test]
    fn backslash_and_leading_dot_slash_are_normalised() {
        let entries = vec![file("./a.txt", 1), file("dir\\nested\\b.txt", 2)];
        let plan = plan_extract(&entries, &[]);

        let names: Vec<&str> = plan.files.iter().map(|e| e.dest_rel.as_str()).collect();
        assert!(names.contains(&"a.txt"));
        assert!(names.contains(&"dir/nested/b.txt"));
        assert_eq!(plan.dirs_to_create, vec!["dir".to_string(), "dir/nested".to_string()]);
    }

    #[test]
    fn a_leaf_only_entry_has_no_ancestor_dirs() {
        let entries = vec![file("solo.txt", 1)];
        let plan = plan_extract(&entries, &[]);

        assert!(plan.dirs_to_create.is_empty());
    }
}
