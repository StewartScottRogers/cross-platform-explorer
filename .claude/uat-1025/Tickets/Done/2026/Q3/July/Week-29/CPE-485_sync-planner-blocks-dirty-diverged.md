---
id: CPE-485
title: "Sync planner: block a diverged merge/rebase on a dirty tree (git can't run it)"
type: Defect
status: Done
priority: Medium
component: Backend
tags: [ready]
estimate: 30m
created: 2026-07-16
closed: 2026-07-16
epic: CPE-429
---

## Summary
The two-way mirror planner `repos::plan_sync` (CPE-438) currently, for a **diverged** history
(ahead>0 AND behind>0) with a **dirty** working tree, emits `[PullMerge, Push]` (or `PullRebase`) and
only *warns* about uncommitted changes. But `git merge`/`git rebase` **refuse to run** with
uncommitted changes ("cannot merge: You have unstaged changes"), so the plan describes steps that will
fail. The planner is meant to be safe-by-default and surface blockers, not hand back a doomed plan.

## Root cause
`plan_sync` adds a dirty warning up front, then plans merge/rebase regardless of `dirty`. Fast-forward
(behind-only) can often still run dirty, but a diverged merge/rebase cannot.

## Fix
When the history is diverged AND the tree is dirty, **block** with a clear message
("commit or stash your changes before reconciling a diverged history") instead of planning
merge/rebase steps. Behind-only fast-forward keeps its warn-but-plan behaviour (unchanged).

## Acceptance Criteria
- [x] Diverged + dirty → `blocked` set, `actions` empty (no doomed merge/rebase).
- [x] Diverged + clean → unchanged (merge/rebase per policy).
- [x] Behind-only + dirty → unchanged (fast-forward planned, dirty warning present).
- [x] New unit test for the dirty-diverged block; existing sync tests still pass; clippy clean.

## Notes
Found during Nightshift forge research (CPE-429). Pure-logic, headless-testable.

## Resolution
Added a `(true, true) if state.dirty` guard to `repos::plan_sync`: a diverged history on a dirty tree now
**blocks** with 'commit or stash your changes before reconciling a diverged history' and emits no
actions, instead of the doomed `[PullMerge/PullRebase, Push]` that git would reject. Behind-only
fast-forward keeps its warn-but-plan behaviour. New test `diverged_on_a_dirty_tree_is_blocked_not_a_
doomed_merge` (covers Merge + Rebase policies + the clean-diverged control). repos crate 29 tests pass;
clippy `--all-targets -D warnings` clean. API unchanged (SyncPlan shape same), so the host's forge_sync
consumer is unaffected. Nightshift loop 13.
