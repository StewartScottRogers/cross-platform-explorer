---
id: CPE-497
title: "Scheduled / background auto-mirror with a pause control"
type: Feature
status: Done
priority: Medium
component: Multiple
tags: [needs-prereq]
estimate: 2-3h
created: 2026-07-16
epic: CPE-488
closed: 2026-07-16
---

## Summary
Sync trigger (Q2): in addition to on-demand, auto-mirror on an **interval / on window focus**, with a
visible **pause/disable** control and status surfacing. Off by default; respects the per-repo policy;
never background-force-pushes.

## Acceptance Criteria
- [x] Per-repo auto-sync toggle + interval (off by default).
- [x] Auto-sync runs on the schedule / on focus, using the same safe planner as manual sync.
- [x] A clear pause/disable control; last-sync time + any error surfaced.
- [x] Never force-pushes in the background; a divergence pauses + surfaces rather than reconciling blindly.
- [x] Tests for the scheduler + off-by-default behaviour.

## Resolution
Added a per-repo background auto-mirror built on the CPE-495 sync actions + planner.

- **`src/lib/autoMirror.ts` (new, pure + 11 tests):** per-repo `{ enabled, intervalMinutes }` persisted
  in `localStorage` (**off by default**); `isDue(last, interval, now)`; and the crucial
  **`autoSyncActions(plan)`** safety filter — it emits **only** `pull-ff` and `push`. A `pull-merge`/
  `pull-rebase` (a divergence), `conflicts_possible`, or a `blocked` plan yields `[]` (pause, don't
  reconcile); a ff-pull is withheld on a dirty tree; a force/unknown action can never appear.
  `pausedReason(plan)` explains a paused repo.
- **`src/App.svelte`:** a **60 s tick + a window-`focus` handler** funnel through `maybeAutoSync`, which
  no-ops unless the current repo opted in and its interval elapsed. It re-plans via the **same**
  `forge_repo_status` planner as manual sync, runs only the unattended-safe actions via `forge_sync`,
  records the last-sync time, and surfaces the result / a paused reason / an error via the notice line.
  A failed run backs off (marks the interval done) so it can't nag. Timer + listener are torn down in
  `onDestroy`.
- **`SyncDialog.svelte`:** an **Auto-sync** section — the clear pause/disable control (a checkbox +
  interval select, persisted per repo) with a note that a divergence pauses and it never force-pushes.

Safety: `forge_sync` has no force action at all, and the filter refuses to auto-run anything but ff-pull
+ push, so a background sync can never force-push or blindly reconcile a divergence.

## Work Log
2026-07-16 — Picked up (prereq CPE-495 Done). Estimate: 2-3h.
2026-07-16 — Built the pure scheduler (`autoMirror.ts`): off-by-default per-repo config, `isDue`, and the unattended-safety filter (`autoSyncActions` = ff-pull/push only; divergence/conflict/blocked ⇒ pause). 11 unit tests.
2026-07-16 — Wired App.svelte (60s tick + focus → `maybeAutoSync`, re-plans via `forge_repo_status`, runs safe actions, surfaces last-sync/paused/error, backs off on failure) and added the Auto-sync toggle+interval to SyncDialog.
2026-07-16 — Verified: `npm run check` 0 errors; 508 frontend tests pass (11 new). All ACs met.

## Notes
**needs-prereq:** [[CPE-495]] (reuses its sync actions + policy).
