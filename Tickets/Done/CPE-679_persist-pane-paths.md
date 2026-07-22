---
id: CPE-679
title: Persist each pane's path/history across sessions
type: feature
component: Frontend
priority: low
status: Done
tags: ready
created: 2026-07-18
closed: 2026-07-22
epic: CPE-617
estimate: 1-2h
---

## Summary
Child of CPE-617. Remember each pane's path (and history) across restarts, reusing the existing tab/settings
persistence, so a dual-pane layout comes back where the user left it. Prereq: CPE-677.

## Acceptance Criteria
- [x] Each pane's path is restored on launch when dual-pane was last active — pane A via the existing
      `restoreLastSession()` (tabs + history), pane B via `navigateB(paneBPath)` in `onMount` (CPE-677
      already persists `paneBPath`).
- [x] Reuses the existing persistence layer (`settings` / `paneBPath`); `npm run check` 0/0.

## Work Log
- 2026-07-22 (nightshift) — Pane A path+history already restored by `restoreLastSession` (existing tab
  session persistence). Added the pane-B half: after session restore, when `dualPane` is on, `await tick()`
  then `navigateB(paneBPath || homePath)` so the split returns where the user left it. Pane B has no
  per-pane history stack in v1 (forward-only nav per CPE-677 scoping) — a pane-B history is a future
  refinement alongside CPE-678. `npm run check` clean.
