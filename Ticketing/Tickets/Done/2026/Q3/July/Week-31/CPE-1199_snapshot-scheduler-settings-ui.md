---
id: CPE-1199
title: "Background interval scheduler + Settings UI for per-folder snapshots (opt-in)"
type: feature
component: Multiple
priority: medium
status: Done
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
- 2026-07-31 — Filed by Foreman (sprint, epic CPE-735). Final phase; hard dep on CPE-1198 + CPE-1196.
  Model tier opus (new scheduler infra). Natural point for attended GUI/visual verify.
- 2026-08-01 — Implemented (Worker, sprint). **Background timer** (`src-tauri/src/lib.rs`): a single
  dedicated `cpe-snapshot-scheduler` thread wakes every 60s (`SNAPSHOT_SCHEDULE_TICK`) and runs
  `snapshot_schedule_tick` — a pure-over-injected-`ServerCtx`+clock helper that **early-returns before
  any capture/disk write when no rule is enabled** (off-means-off, verified by construction), else calls
  `snapshot_run_due` (CPE-1198, captures + retention-prunes), recording each captured root's run time in
  an in-memory last-run map so an already-run rule waits its full interval instead of re-capturing every
  wake. Spawned in `setup()`, never joined (doesn't block startup), errors swallowed (can't crash the
  app), desktop-only. **Settings UI**: new `ScheduledSnapshots.svelte` section in `SettingsDialog.svelte`
  — lists rules, add/remove a watched folder (native Browse picker, [[path-inputs-need-picker]]), set
  interval (value + minutes/hours/days) + retention (hourly/daily/weekly/monthly kept), enable/pause —
  all inline instant controls via `commands.snapshotScheduleList/Set/Remove`, no launch-time consent
  modal ([[avoid-modal-permission-popups]], [[prefer-inline-instant-controls]]). No specta struct
  touched ⇒ no bindings regen. Docs: extended `src/docs/16-checkpoints.md` with a Scheduled snapshots
  section. **Tests**: 3 backend (`lib.rs`) incl. the zero-idle-cost assertion (no rules / all-disabled ⇒
  no capture, no bookkeeping change) + the enable→due→capture→hold-off-within-interval path; 5 frontend
  component tests (add builds the right rule, Browse fills path via the picker, remove, enable toggle);
  a gui-smoke spec `snapshot-schedule-settings.smoke.ts` opening Settings + `snap`. Verify: cargo test
  --lib 80 ok, clippy clean both feature modes, npm run check 0 errors, npm test 1561 ok, gui-smoke
  typecheck clean. → Done (PR to main).
