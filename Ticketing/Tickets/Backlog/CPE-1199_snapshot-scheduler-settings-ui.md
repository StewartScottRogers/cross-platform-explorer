---
id: CPE-1199
title: "Background interval scheduler + Settings UI for per-folder snapshots (opt-in)"
type: feature
component: Multiple
priority: medium
status: Backlog
tags: ready
created: 2026-07-31
epic: CPE-735
---

## Summary
Part of CPE-735 (final phase, after CPE-1196 + CPE-1198). The actual background timer + the Settings UI. This
establishes the app's first generic periodic-task scheduler (only a `notify` watcher exists today). Needs a
Reviewer pass focused on the prune no-double-release / no-data-loss invariant + off-means-off.

## Build
- Spawn a background interval task in `run()` (`src-tauri/src/lib.rs`) that periodically calls
  `snapshot_run_due` (CPE-1198). **Off-means-off:** no watched folders ⇒ zero timer cost/CPU/disk.
- A **Settings** panel to add/remove watched folders, set interval + retention, enable/disable — inline
  instant controls, NOT a launch-time consent modal ([[avoid-modal-permission-popups]],
  [[prefer-inline-instant-controls]]). Path fields offer a native Browse picker ([[path-inputs-need-picker]]).

## Acceptance Criteria
- [ ] Headless test of enable→due→capture with a fast/injected interval; idle cost provably zero with no rules.
- [ ] gui-smoke screenshot of the Settings panel; `npm run check` + `npm test` + `cargo test` green.
- [ ] Reviewer confirms the auto-prune preserves the manifest-first / no-double-release invariant.

## Work Log
- 2026-07-31 — Filed by Foreman (workshift, epic CPE-735). Final phase; hard dep on CPE-1198 + CPE-1196.
  Model tier opus (new scheduler infra). Natural point for attended GUI/visual verify.
