//! Two-way mirror / sync **planner** (CPE-438) — the interconnect core (CPE-429).
//!
//! Given a repo's [`RepoState`](crate::status::RepoState) (ahead/behind/dirty vs its upstream) and a
//! [`SyncPolicy`], compute the sequence of git actions that would bring local and remote into sync in
//! **both directions** — pull what's remote, push what's local. Pure, so the exact plan is
//! unit-tested; the sidecar executes it by shelling out to `git`.
//!
//! **Safe by default (D3):** it never force-pushes unless `allow_force` is explicitly set, it refuses
//! to plan through a diverged history under `Manual`, and it surfaces conflict risk + dirty-tree
//! warnings rather than clobbering. It is a *dry-run description* — nothing is executed here.

use crate::status::RepoState;

/// How to reconcile a **diverged** history (local and remote both moved).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DivergePolicy {
    /// Merge the remote into local (a merge commit), then push.
    Merge,
    /// Rebase local commits onto the remote, then push.
    Rebase,
    /// Don't touch it — the user resolves the divergence themselves.
    Manual,
}

/// The policy that governs a sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncPolicy {
    pub on_diverge: DivergePolicy,
    /// Allow a force-push (dangerous — overwrites remote history). Off by default.
    pub allow_force: bool,
}

impl Default for SyncPolicy {
    fn default() -> Self {
        // Safe defaults: reconcile a divergence by merge, never force-push.
        SyncPolicy { on_diverge: DivergePolicy::Merge, allow_force: false }
    }
}

/// One concrete step of a sync plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncAction {
    /// Fetch + fast-forward the local branch to the remote (no local commits at risk).
    PullFastForward,
    /// Fetch + merge the remote into local (may conflict).
    PullMerge,
    /// Fetch + rebase local commits onto the remote (may conflict).
    PullRebase,
    /// Push local commits to the remote.
    Push,
}

/// A computed, not-yet-executed sync plan.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SyncPlan {
    /// The ordered steps to run. Empty when already in sync (or blocked).
    pub actions: Vec<SyncAction>,
    /// True when an action could hit merge conflicts (diverged merge/rebase).
    pub conflicts_possible: bool,
    /// Non-fatal advisories (e.g. uncommitted changes present).
    pub warnings: Vec<String>,
    /// Set when the sync can't be planned safely (e.g. no upstream, or diverged under Manual).
    pub blocked: Option<String>,
    /// True when there is genuinely nothing to do.
    pub up_to_date: bool,
}

/// Plan a two-way sync for `state` under `policy`. Never mutates anything — describes the actions.
pub fn plan_sync(state: &RepoState, policy: &SyncPolicy) -> SyncPlan {
    let mut plan = SyncPlan::default();

    if state.upstream.is_none() {
        plan.blocked = Some("no upstream is configured for this branch".into());
        return plan;
    }
    if state.dirty {
        plan.warnings.push("uncommitted changes are present — commit or stash before syncing".into());
    }

    match (state.behind > 0, state.ahead > 0) {
        // Already in sync.
        (false, false) => plan.up_to_date = true,
        // Only remote moved → fast-forward pull (no local work at risk).
        (true, false) => plan.actions.push(SyncAction::PullFastForward),
        // Only local moved → push.
        (false, true) => plan.actions.push(SyncAction::Push),
        // Both moved (diverged) → reconcile per policy, then push.
        (true, true) if state.dirty => {
            // git merge/rebase both REFUSE to run with uncommitted changes, so don't hand back a plan
            // that's guaranteed to fail — block until the tree is clean (CPE-485).
            plan.blocked =
                Some("commit or stash your changes before reconciling a diverged history".into());
        }
        (true, true) => match policy.on_diverge {
            DivergePolicy::Manual => {
                plan.blocked =
                    Some("local and remote have diverged — reconcile manually (merge/rebase)".into());
            }
            DivergePolicy::Merge => {
                plan.actions.push(SyncAction::PullMerge);
                plan.actions.push(SyncAction::Push);
                plan.conflicts_possible = true;
            }
            DivergePolicy::Rebase => {
                plan.actions.push(SyncAction::PullRebase);
                plan.actions.push(SyncAction::Push);
                plan.conflicts_possible = true;
            }
        },
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(ahead: u32, behind: u32, dirty: bool) -> RepoState {
        RepoState {
            branch: Some("main".into()),
            upstream: Some("origin/main".into()),
            ahead,
            behind,
            dirty,
        }
    }

    #[test]
    fn up_to_date_plans_nothing() {
        let p = plan_sync(&state(0, 0, false), &SyncPolicy::default());
        assert!(p.up_to_date && p.actions.is_empty() && p.blocked.is_none());
    }

    #[test]
    fn behind_only_fast_forwards_ahead_only_pushes() {
        assert_eq!(plan_sync(&state(0, 3, false), &SyncPolicy::default()).actions, vec![SyncAction::PullFastForward]);
        assert_eq!(plan_sync(&state(2, 0, false), &SyncPolicy::default()).actions, vec![SyncAction::Push]);
    }

    #[test]
    fn diverged_merge_then_push_and_flags_conflicts() {
        let p = plan_sync(&state(2, 3, false), &SyncPolicy::default()); // default = Merge
        assert_eq!(p.actions, vec![SyncAction::PullMerge, SyncAction::Push]);
        assert!(p.conflicts_possible);
    }

    #[test]
    fn diverged_rebase_uses_rebase() {
        let policy = SyncPolicy { on_diverge: DivergePolicy::Rebase, allow_force: false };
        let p = plan_sync(&state(1, 1, false), &policy);
        assert_eq!(p.actions, vec![SyncAction::PullRebase, SyncAction::Push]);
    }

    #[test]
    fn diverged_manual_is_blocked_never_clobbers() {
        let policy = SyncPolicy { on_diverge: DivergePolicy::Manual, allow_force: false };
        let p = plan_sync(&state(2, 2, false), &policy);
        assert!(p.actions.is_empty());
        assert!(p.blocked.as_deref().unwrap().contains("diverged"));
    }

    #[test]
    fn diverged_on_a_dirty_tree_is_blocked_not_a_doomed_merge() {
        // git merge/rebase can't run with uncommitted changes, so a diverged+dirty sync must block
        // rather than plan steps that will fail (CPE-485).
        for policy in [
            SyncPolicy::default(), // Merge
            SyncPolicy { on_diverge: DivergePolicy::Rebase, allow_force: false },
        ] {
            let p = plan_sync(&state(2, 3, true), &policy);
            assert!(p.actions.is_empty(), "no doomed actions on a dirty diverged tree");
            assert!(!p.conflicts_possible);
            assert!(p.blocked.as_deref().unwrap().contains("commit or stash"));
        }
        // Clean diverged still plans normally.
        assert!(!plan_sync(&state(2, 3, false), &SyncPolicy::default()).actions.is_empty());
    }

    #[test]
    fn no_upstream_is_blocked() {
        let mut s = state(0, 0, false);
        s.upstream = None;
        assert!(plan_sync(&s, &SyncPolicy::default()).blocked.is_some());
    }

    #[test]
    fn a_dirty_tree_warns_but_still_plans() {
        let p = plan_sync(&state(0, 2, true), &SyncPolicy::default());
        assert_eq!(p.actions, vec![SyncAction::PullFastForward]);
        assert!(p.warnings.iter().any(|w| w.contains("uncommitted")));
    }

    #[test]
    fn default_policy_is_safe_no_force() {
        assert!(!SyncPolicy::default().allow_force);
    }
}
