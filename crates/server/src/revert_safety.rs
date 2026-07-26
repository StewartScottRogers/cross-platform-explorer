//! Revert safety classifier (CPE-1080, epic CPE-732): given a checkpoint restore [`RestoreAction`] plan
//! ([`crate::restore_plan`]) and the set of root-relative paths the agent is recorded as having touched
//! since the checkpoint, classify each action as [`RevertSafety::Safe`] (the agent's own edit — reverting
//! it is expected) or a [`RevertSafety::Conflict`] (something *outside* the agent changed that path, so a
//! blind revert would silently clobber it). Pure map/set diff — no filesystem I/O, no new deps. Reuses
//! [`crate::restore_plan::RestoreAction`] and [`crate::restore_plan::Snapshot`] as-is; does not modify them.
//!
//! `agent_touched` is a plain `&BTreeSet<String>` parameter (root-relative `/`-segment keys, the same
//! convention `revert_attribution::agent_touched` produces — CPE-1079) so this module has no dependency
//! on that one and can land independently.

use std::collections::BTreeSet;

use crate::restore_plan::{RestoreAction, Snapshot};

/// Why a path is a conflict: the shape of the divergence between `checkpoint` and `current` that the
/// agent's recorded activity does not explain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictKind {
    /// Present in both checkpoint and current, but the content differs (the restore action is an
    /// `Overwrite`) — someone outside the agent edited it.
    ChangedOutsideAgent,
    /// Present in current but not in the checkpoint (the restore action is a `Delete`) — someone outside
    /// the agent created it after the checkpoint was taken.
    RecreatedOutsideAgent,
    /// Present in the checkpoint but missing from current (the restore action is a `Create`) — someone
    /// outside the agent removed it.
    DeletedOutsideAgent,
}

/// Whether reverting one action is safe (the agent's own change) or a conflict (an outside change the
/// agent isn't recorded as having made).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevertSafety {
    Safe,
    Conflict(ConflictKind),
}

/// One restore-plan action paired with its safety classification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassifiedAction {
    pub action: RestoreAction,
    pub safety: RevertSafety,
}

/// Classify every action in `plan`: `Safe` iff `agent_touched` contains that action's path (the agent is
/// recorded as the one who made the change the revert would undo); otherwise a `Conflict`, whose kind is
/// picked from the checkpoint-vs-current shape at that path. Preserves `plan`'s order.
pub fn classify_plan(
    plan: &[RestoreAction],
    checkpoint: &Snapshot,
    current: &Snapshot,
    agent_touched: &BTreeSet<String>,
) -> Vec<ClassifiedAction> {
    plan.iter()
        .map(|action| {
            let safety = if agent_touched.contains(&action.path) {
                RevertSafety::Safe
            } else {
                RevertSafety::Conflict(conflict_kind(checkpoint, current, &action.path))
            };
            ClassifiedAction { action: action.clone(), safety }
        })
        .collect()
}

/// The checkpoint-vs-current shape at `path`, mapped to a [`ConflictKind`]:
/// - in checkpoint, missing from current → the outside world deleted it (`DeletedOutsideAgent`).
/// - missing from checkpoint, in current → the outside world (re)created it (`RecreatedOutsideAgent`).
/// - in both (content differs, or this is reached defensively) → the outside world edited it
///   (`ChangedOutsideAgent`).
fn conflict_kind(checkpoint: &Snapshot, current: &Snapshot, path: &str) -> ConflictKind {
    match (checkpoint.contains_key(path), current.contains_key(path)) {
        (true, false) => ConflictKind::DeletedOutsideAgent,
        (false, true) => ConflictKind::RecreatedOutsideAgent,
        _ => ConflictKind::ChangedOutsideAgent,
    }
}

/// Split a classified plan into the actions safe to apply as-is and the ones that conflict with an
/// outside change (kept with their full classification so the caller can show why). Preserves order.
pub fn partition(classified: &[ClassifiedAction]) -> (Vec<RestoreAction>, Vec<ClassifiedAction>) {
    let mut safe = Vec::new();
    let mut conflicts = Vec::new();
    for c in classified {
        match &c.safety {
            RevertSafety::Safe => safe.push(c.action.clone()),
            RevertSafety::Conflict(_) => conflicts.push(c.clone()),
        }
    }
    (safe, conflicts)
}

/// A confirm-dialog-ready summary of a classified plan: how many actions are safe to revert
/// automatically, how many conflict with an outside change, and which paths those conflicts are at
/// (deterministic order — the order `classified` is given in, i.e. the plan's own order).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ConflictReport {
    pub safe: usize,
    pub conflicts: usize,
    pub conflict_paths: Vec<String>,
    /// `conflicts / (safe + conflicts)`, or `None` if there are no actions at all (division-safe:
    /// never NaN/inf).
    pub conflict_ratio: Option<f64>,
}

/// Summarize a classified plan for display: counts of safe vs. conflicting actions, the list of
/// conflicting paths (in `classified`'s order), and a division-safe conflict ratio.
pub fn summarize_conflicts(classified: &[ClassifiedAction]) -> ConflictReport {
    let mut safe = 0usize;
    let mut conflicts = 0usize;
    let mut conflict_paths = Vec::new();
    for c in classified {
        match &c.safety {
            RevertSafety::Safe => safe += 1,
            RevertSafety::Conflict(_) => {
                conflicts += 1;
                conflict_paths.push(c.action.path.clone());
            }
        }
    }
    let total = safe + conflicts;
    let conflict_ratio = if total == 0 { None } else { Some(conflicts as f64 / total as f64) };
    ConflictReport { safe, conflicts, conflict_paths, conflict_ratio }
}

/// The plan to actually execute, given a user's choice of whether to include conflicting actions:
/// `include_conflicts = false` (the default, "revert safe only") keeps just the `Safe` actions;
/// `true` ("revert anyway") keeps every action. Preserves `classified`'s order either way.
pub fn plan_for(classified: &[ClassifiedAction], include_conflicts: bool) -> Vec<RestoreAction> {
    classified
        .iter()
        .filter(|c| include_conflicts || c.safety == RevertSafety::Safe)
        .map(|c| c.action.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::restore_plan::{plan_restore, FileState, RestoreOp};

    fn snap(entries: &[(&str, &str, u64)]) -> Snapshot {
        entries.iter().map(|(p, h, sz)| (p.to_string(), FileState::new(*h, *sz))).collect()
    }

    fn touched(paths: &[&str]) -> BTreeSet<String> {
        paths.iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn agent_only_modification_is_safe() {
        // a.rs changed, and the agent is recorded as having touched it.
        let checkpoint = snap(&[("a.rs", "old", 10)]);
        let current = snap(&[("a.rs", "new", 12)]);
        let plan = plan_restore(&checkpoint, &current);
        let agent_touched = touched(&["a.rs"]);

        let classified = classify_plan(&plan, &checkpoint, &current, &agent_touched);
        assert_eq!(classified.len(), 1);
        assert_eq!(classified[0].safety, RevertSafety::Safe);
        assert_eq!(classified[0].action, RestoreAction { path: "a.rs".into(), op: RestoreOp::Overwrite });
    }

    #[test]
    fn third_party_change_is_conflict_changed_outside_agent() {
        // a.rs changed, but the agent isn't recorded as having touched it.
        let checkpoint = snap(&[("a.rs", "old", 10)]);
        let current = snap(&[("a.rs", "new", 12)]);
        let plan = plan_restore(&checkpoint, &current);
        let agent_touched = touched(&[]); // agent touched nothing

        let classified = classify_plan(&plan, &checkpoint, &current, &agent_touched);
        assert_eq!(classified.len(), 1);
        assert_eq!(
            classified[0].safety,
            RevertSafety::Conflict(ConflictKind::ChangedOutsideAgent)
        );
    }

    #[test]
    fn delete_of_file_agent_never_created_is_conflict_recreated_outside_agent() {
        // new.rs isn't in the checkpoint but exists in current (op = Delete) and the agent didn't
        // create it, so it was recreated/created by someone outside the agent.
        let checkpoint = snap(&[("a.rs", "h", 1)]);
        let current = snap(&[("a.rs", "h", 1), ("new.rs", "n", 7)]);
        let plan = plan_restore(&checkpoint, &current);
        let agent_touched = touched(&[]);

        let classified = classify_plan(&plan, &checkpoint, &current, &agent_touched);
        assert_eq!(classified.len(), 1);
        assert_eq!(classified[0].action.op, RestoreOp::Delete);
        assert_eq!(
            classified[0].safety,
            RevertSafety::Conflict(ConflictKind::RecreatedOutsideAgent)
        );
    }

    #[test]
    fn recreate_of_file_agent_did_not_delete_is_conflict_deleted_outside_agent() {
        // gone.rs is in the checkpoint but missing from current (op = Create) and the agent isn't
        // recorded as having touched it, so someone outside the agent deleted it.
        let checkpoint = snap(&[("a.rs", "h", 1), ("gone.rs", "g", 5)]);
        let current = snap(&[("a.rs", "h", 1)]);
        let plan = plan_restore(&checkpoint, &current);
        let agent_touched = touched(&[]);

        let classified = classify_plan(&plan, &checkpoint, &current, &agent_touched);
        assert_eq!(classified.len(), 1);
        assert_eq!(classified[0].action.op, RestoreOp::Create);
        assert_eq!(
            classified[0].safety,
            RevertSafety::Conflict(ConflictKind::DeletedOutsideAgent)
        );
    }

    #[test]
    fn empty_agent_touched_with_real_diff_is_all_conflict() {
        let checkpoint = snap(&[("a.rs", "old", 10), ("gone.rs", "g", 5)]);
        let current = snap(&[("a.rs", "new", 12), ("new.rs", "n", 7)]);
        let plan = plan_restore(&checkpoint, &current);
        let agent_touched = touched(&[]);

        let classified = classify_plan(&plan, &checkpoint, &current, &agent_touched);
        assert_eq!(classified.len(), 3);
        assert!(classified.iter().all(|c| matches!(c.safety, RevertSafety::Conflict(_))));
    }

    #[test]
    fn partition_splits_safe_and_conflicts_preserving_order() {
        // a.rs (agent-touched, overwrite) → safe; gone.rs (create, not touched) → conflict;
        // new.rs (delete, not touched) → conflict. Plan is sorted by path already.
        let checkpoint = snap(&[("a.rs", "old", 10), ("gone.rs", "g", 5)]);
        let current = snap(&[("a.rs", "new", 12), ("new.rs", "n", 7)]);
        let plan = plan_restore(&checkpoint, &current);
        assert_eq!(plan.len(), 3); // a.rs overwrite, gone.rs create, new.rs delete
        let agent_touched = touched(&["a.rs"]);

        let classified = classify_plan(&plan, &checkpoint, &current, &agent_touched);
        let (safe, conflicts) = partition(&classified);

        assert_eq!(safe, vec![RestoreAction { path: "a.rs".into(), op: RestoreOp::Overwrite }]);
        assert_eq!(conflicts.len(), 2);
        assert_eq!(conflicts[0].action.path, "gone.rs");
        assert_eq!(conflicts[1].action.path, "new.rs");
    }

    #[test]
    fn empty_plan_yields_empty_output() {
        let checkpoint = snap(&[("a.rs", "h", 1)]);
        let current = checkpoint.clone();
        let plan = plan_restore(&checkpoint, &current);
        assert!(plan.is_empty());

        let agent_touched: BTreeSet<String> = BTreeSet::new();
        let classified = classify_plan(&plan, &checkpoint, &current, &agent_touched);
        assert!(classified.is_empty());

        let (safe, conflicts) = partition(&classified);
        assert!(safe.is_empty());
        assert!(conflicts.is_empty());
    }

    #[test]
    fn deterministic_order_matches_plan_order() {
        let checkpoint = snap(&[("a.rs", "1", 1), ("b.rs", "1", 1), ("c.rs", "1", 1)]);
        let current = snap(&[("a.rs", "2", 1), ("b.rs", "2", 1), ("c.rs", "2", 1)]);
        let plan = plan_restore(&checkpoint, &current);
        let agent_touched = touched(&["b.rs"]);

        let classified = classify_plan(&plan, &checkpoint, &current, &agent_touched);
        let paths: Vec<&str> = classified.iter().map(|c| c.action.path.as_str()).collect();
        assert_eq!(paths, vec!["a.rs", "b.rs", "c.rs"]);
        assert_eq!(classified[1].safety, RevertSafety::Safe);
    }

    #[test]
    fn summarize_conflicts_counts_and_collects_paths_in_order() {
        // a.rs safe (agent-touched), gone.rs + new.rs conflicts, in plan order.
        let checkpoint = snap(&[("a.rs", "old", 10), ("gone.rs", "g", 5)]);
        let current = snap(&[("a.rs", "new", 12), ("new.rs", "n", 7)]);
        let plan = plan_restore(&checkpoint, &current);
        let agent_touched = touched(&["a.rs"]);

        let classified = classify_plan(&plan, &checkpoint, &current, &agent_touched);
        let report = summarize_conflicts(&classified);

        assert_eq!(report.safe, 1);
        assert_eq!(report.conflicts, 2);
        assert_eq!(report.conflict_paths, vec!["gone.rs".to_string(), "new.rs".to_string()]);
        assert_eq!(report.conflict_ratio, Some(2.0 / 3.0));
    }

    #[test]
    fn summarize_conflicts_empty_plan_has_no_ratio() {
        let report = summarize_conflicts(&[]);
        assert_eq!(report, ConflictReport::default());
        assert_eq!(report.conflict_ratio, None);
    }

    #[test]
    fn summarize_conflicts_all_safe_has_zero_ratio_not_nan() {
        let checkpoint = snap(&[("a.rs", "old", 10)]);
        let current = snap(&[("a.rs", "new", 12)]);
        let plan = plan_restore(&checkpoint, &current);
        let agent_touched = touched(&["a.rs"]);

        let classified = classify_plan(&plan, &checkpoint, &current, &agent_touched);
        let report = summarize_conflicts(&classified);

        assert_eq!(report.safe, 1);
        assert_eq!(report.conflicts, 0);
        assert!(report.conflict_paths.is_empty());
        assert_eq!(report.conflict_ratio, Some(0.0));
        assert!(report.conflict_ratio.unwrap().is_finite());
    }

    #[test]
    fn summarize_conflicts_all_conflict_has_ratio_one() {
        let checkpoint = snap(&[("a.rs", "old", 10)]);
        let current = snap(&[("a.rs", "new", 12)]);
        let plan = plan_restore(&checkpoint, &current);
        let agent_touched = touched(&[]);

        let classified = classify_plan(&plan, &checkpoint, &current, &agent_touched);
        let report = summarize_conflicts(&classified);

        assert_eq!(report.safe, 0);
        assert_eq!(report.conflicts, 1);
        assert_eq!(report.conflict_ratio, Some(1.0));
        assert!(report.conflict_ratio.unwrap().is_finite());
    }

    #[test]
    fn plan_for_false_keeps_only_safe_subset() {
        let checkpoint = snap(&[("a.rs", "old", 10), ("gone.rs", "g", 5)]);
        let current = snap(&[("a.rs", "new", 12), ("new.rs", "n", 7)]);
        let plan = plan_restore(&checkpoint, &current);
        let agent_touched = touched(&["a.rs"]);

        let classified = classify_plan(&plan, &checkpoint, &current, &agent_touched);
        let safe_only = plan_for(&classified, false);

        assert_eq!(safe_only, vec![RestoreAction { path: "a.rs".into(), op: RestoreOp::Overwrite }]);
    }

    #[test]
    fn plan_for_true_keeps_all_actions() {
        let checkpoint = snap(&[("a.rs", "old", 10), ("gone.rs", "g", 5)]);
        let current = snap(&[("a.rs", "new", 12), ("new.rs", "n", 7)]);
        let plan = plan_restore(&checkpoint, &current);
        let agent_touched = touched(&["a.rs"]);

        let classified = classify_plan(&plan, &checkpoint, &current, &agent_touched);
        let all = plan_for(&classified, true);

        assert_eq!(all.len(), classified.len());
        let expected: Vec<RestoreAction> = classified.iter().map(|c| c.action.clone()).collect();
        assert_eq!(all, expected);
    }

    #[test]
    fn plan_for_empty_plan_yields_empty_output() {
        assert!(plan_for(&[], false).is_empty());
        assert!(plan_for(&[], true).is_empty());
    }
}
