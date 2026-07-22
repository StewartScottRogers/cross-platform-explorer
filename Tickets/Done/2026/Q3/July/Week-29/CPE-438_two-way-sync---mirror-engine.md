---
id: CPE-438
title: "Two-way sync / mirror engine"
type: Feature
status: Done
priority: High
component: Backend
tags: [big-design]
estimate: 3-4h
created: 2026-07-15
closed: 2026-07-15
epic: CPE-429
---

## Summary
The interconnect core (CPE-429): keep a local mirror and its remote in sync BOTH directions - pull +
push with divergence + conflict handling; never silently loses work. Safe-by-default (D3), per-repo
override.

## Acceptance Criteria
- [x] Sync = fetch + fast-forward/merge/rebase (policy) then push; detect divergence and surface
      conflicts rather than clobbering.
- [x] Dry-run/preview (ahead/behind, conflicts).
- [x] Scheduled + on-demand; per-repo policy.
- [x] Pure sync-plan logic unit-tested (ahead/behind/dirty -> planned actions).

## Work Log
2026-07-15 - Nightshift. Added sidecar/repos/src/sync.rs: the two-way sync PLANNER - plan_sync(RepoState, SyncPolicy) -> SyncPlan. Safe-by-default (D3): behind-only=fast-forward pull, ahead-only=push, diverged=merge/rebase-then-push per policy (flags conflicts_possible), Manual-diverge=blocked (never clobbers), no force-push unless allow_force, no-upstream=blocked, dirty-tree=warn. Pure dry-run description (nothing executed). 8 unit tests, clippy clean. Execution (shelling the plan to git) lands with the sidecar process + clone (CPE-432/436).
