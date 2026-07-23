//! Checkpoint restore-plan computation (CPE-917, epic CPE-732): given a checkpoint [`Snapshot`] of the
//! watched subtree and its current state (each a path → content-addressed [`FileState`]), compute the
//! minimal set of filesystem operations that would revert the tree back to the checkpoint. Pure diff over
//! two maps — the revert engine executes the plan (respecting the skip-unreadable rule), and the restore
//! UI previews it. Complements the activity replay ([`crate::activity_timeline`], epic CPE-728).

use std::collections::{BTreeMap, BTreeSet};

/// One file's identity in a snapshot: its content hash (content-addressed, so unchanged files dedup) and
/// byte size. Directories are implied by their files; this slice diffs files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileState {
    pub hash: String,
    pub size: u64,
}

impl FileState {
    pub fn new(hash: impl Into<String>, size: u64) -> Self {
        Self { hash: hash.into(), size }
    }
}

/// A snapshot of a subtree: path → its [`FileState`], sorted for a stable, diffable order.
pub type Snapshot = BTreeMap<String, FileState>;

/// What to do to one path to move `current` back toward the checkpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreOp {
    /// In the checkpoint but gone now → recreate it (restore a deleted file).
    Create,
    /// In both but the content differs → overwrite with the checkpoint content.
    Overwrite,
    /// Present now but absent from the checkpoint → created after the checkpoint; delete it.
    Delete,
}

/// One step of a restore plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreAction {
    pub path: String,
    pub op: RestoreOp,
}

/// Compute the ops that revert `current` to `checkpoint`. Only changed paths appear (an unchanged file —
/// same hash — is skipped, the point of content addressing). Sorted by path for a stable preview.
///
/// Execution note for the revert engine: `Create`/`Overwrite` write files, so ensure parent dirs first;
/// `Delete` removes files, so apply deletes deepest-first (reverse path order) to empty dirs before them.
pub fn plan_restore(checkpoint: &Snapshot, current: &Snapshot) -> Vec<RestoreAction> {
    let paths: BTreeSet<&String> = checkpoint.keys().chain(current.keys()).collect();
    let mut plan = Vec::new();
    for path in paths {
        if let Some(action) = diff_one(path, checkpoint.get(path), current.get(path)) {
            plan.push(action);
        }
    }
    plan
}

/// The single-path revert (cherry-revert from a timeline entry): what would it take to restore just
/// `path` to its checkpoint state? `None` if it's already at the checkpoint (or absent from both).
pub fn revert_one(path: &str, checkpoint: &Snapshot, current: &Snapshot) -> Option<RestoreAction> {
    diff_one(path, checkpoint.get(path), current.get(path))
}

fn diff_one(path: &str, was: Option<&FileState>, now: Option<&FileState>) -> Option<RestoreAction> {
    let op = match (was, now) {
        (Some(_), None) => RestoreOp::Create,               // deleted since checkpoint → restore it
        (None, Some(_)) => RestoreOp::Delete,               // created since checkpoint → remove it
        (Some(w), Some(n)) if w.hash != n.hash => RestoreOp::Overwrite, // modified → revert content
        _ => return None,                                   // identical, or absent from both
    };
    Some(RestoreAction { path: path.to_string(), op })
}

/// A one-line summary of a restore plan for the confirm dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RestorePlanSummary {
    pub creates: usize,
    pub overwrites: usize,
    pub deletes: usize,
    /// Bytes the plan writes back (Create + Overwrite checkpoint sizes); Deletes free space, not counted.
    pub bytes_written: u64,
}

impl RestorePlanSummary {
    /// Total number of paths the plan touches.
    pub fn total(&self) -> usize {
        self.creates + self.overwrites + self.deletes
    }

    /// True when `current` already matches the checkpoint (nothing to revert).
    pub fn is_empty(&self) -> bool {
        self.total() == 0
    }
}

/// Summarise a plan, sizing writes from the checkpoint (the content being restored).
pub fn summarize_plan(plan: &[RestoreAction], checkpoint: &Snapshot) -> RestorePlanSummary {
    let mut s = RestorePlanSummary::default();
    for a in plan {
        match a.op {
            RestoreOp::Create => {
                s.creates += 1;
                s.bytes_written += checkpoint.get(&a.path).map_or(0, |f| f.size);
            }
            RestoreOp::Overwrite => {
                s.overwrites += 1;
                s.bytes_written += checkpoint.get(&a.path).map_or(0, |f| f.size);
            }
            RestoreOp::Delete => s.deletes += 1,
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(entries: &[(&str, &str, u64)]) -> Snapshot {
        entries.iter().map(|(p, h, sz)| (p.to_string(), FileState::new(*h, *sz))).collect()
    }

    #[test]
    fn identical_trees_need_no_restore() {
        let s = snap(&[("a.rs", "h1", 10), ("b.rs", "h2", 20)]);
        assert!(plan_restore(&s, &s).is_empty());
        assert!(summarize_plan(&plan_restore(&s, &s), &s).is_empty());
    }

    #[test]
    fn detects_create_overwrite_and_delete() {
        // checkpoint had a.rs + gone.rs; current modified a.rs, deleted gone.rs, added new.rs.
        let checkpoint = snap(&[("a.rs", "old", 10), ("gone.rs", "g", 5)]);
        let current = snap(&[("a.rs", "new", 12), ("new.rs", "n", 7)]);
        let plan = plan_restore(&checkpoint, &current);
        // Sorted by path: a.rs (overwrite), gone.rs (create/restore), new.rs (delete).
        assert_eq!(plan.len(), 3);
        assert_eq!(plan[0], RestoreAction { path: "a.rs".into(), op: RestoreOp::Overwrite });
        assert_eq!(plan[1], RestoreAction { path: "gone.rs".into(), op: RestoreOp::Create });
        assert_eq!(plan[2], RestoreAction { path: "new.rs".into(), op: RestoreOp::Delete });

        let s = summarize_plan(&plan, &checkpoint);
        assert_eq!((s.creates, s.overwrites, s.deletes), (1, 1, 1));
        assert_eq!(s.total(), 3);
        // bytes_written = gone.rs(5, create) + a.rs(10, overwrite from checkpoint); delete not counted.
        assert_eq!(s.bytes_written, 15);
    }

    #[test]
    fn cherry_revert_single_path() {
        let checkpoint = snap(&[("x.rs", "orig", 4)]);
        let current = snap(&[("x.rs", "edited", 6)]);
        assert_eq!(
            revert_one("x.rs", &checkpoint, &current),
            Some(RestoreAction { path: "x.rs".into(), op: RestoreOp::Overwrite })
        );
        // A path already at the checkpoint → nothing to do.
        assert_eq!(revert_one("x.rs", &checkpoint, &checkpoint), None);
        // A path absent from both → nothing.
        assert_eq!(revert_one("ghost.rs", &checkpoint, &current), None);
    }
}
