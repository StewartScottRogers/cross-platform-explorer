---
id: CPE-1198
title: "Snapshot schedule rules store + pure due() policy + snapshot_run_due command (no timer)"
type: feature
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-07-31
epic: CPE-735
---

## Summary
Part of CPE-735. The headless core of the periodic-snapshot scheduler, decoupled from any background thread
(the timer + UI land in CPE-1199). Fully unit-testable.

## Build
- New `crates/server/src/snapshot_schedule.rs`: a persisted per-folder schedule-rule store (folder path,
  interval, retention policy, enabled) reusing the `cpe_server::settings` persistence pattern; a **pure**
  `due(rules, now, last_run_times) -> Vec<root>` decision (past-interval, enabled only).
- Command `snapshot_run_due` that captures every due root (via the existing `checkpoint_create`) then applies
  CPE-1196's retention. Thin dispatchers in `lib.rs` + `bindings.gen.ts` regen. **No timer, no UI.**

## Acceptance Criteria
- [x] `cargo test -p cpe-server`: rule CRUD round-trips to disk; `due()` returns exactly the roots past their
      interval and excludes disabled ones; `snapshot_run_due` captures due roots + prunes to policy;
      deterministic with an injected `now`.
- [x] clippy both modes clean; bindings regenerated (drift guard green).

## Build (as landed)
- New `crates/server/src/snapshot_schedule.rs`: `ScheduleRule { root, interval_s, retention: RetentionPolicy,
  enabled }` persisted as a root-keyed `BTreeMap` catalog in `snapshot_schedule.json` under the app config
  dir — mirrors `column_config.rs`'s store pattern exactly (tolerant read, absent/corrupt → empty). CRUD:
  `list_rules`/`get_rule`/`set_rule`/`remove_rule`.
- Pure `due(rules, now, last_run_times) -> Vec<root>`: enabled-only, past-interval (or never-run) roots;
  `now`/`last_run_times` always caller-injected, no wall clock read.
- `snapshot_run_due(ctx, now, last_run_times)`: for each due root, `checkpoint_store::checkpoint_create`
  (labelled `"scheduled"`) then `checkpoint_store::checkpoint_prune_apply` (CPE-1196) with that root's rule
  policy. A root whose capture fails is skipped, not fatal to the batch. No timer, no UI — a single pass a
  caller invokes.
- Thin Tauri dispatchers `snapshot_schedule_list`/`snapshot_schedule_set`/`snapshot_schedule_remove`/
  `snapshot_run_due` in `src-tauri/src/lib.rs`, registered in `generate_handler!` + `collect_commands!`;
  `bindings.gen.ts` regenerated (`ScheduleRule`/`RunDueOutcome` types).

## Work Log
- 2026-07-31 — Filed by Foreman (sprint, epic CPE-735). Backend batch (with 1196 + 1197-backend), one worker
  sequential. Consumes 1196's retention apply (soft). Prereq for CPE-1199.
- 2026-07-31 — Implemented on branch `cpe-1196-1198-snapshot-backend`. `cargo test -p cpe-server` green
  (10 new `snapshot_schedule::` tests: rule CRUD round-trip + replace + corrupt-tolerant, `due()`
  disabled/never-run/interval-boundary/deterministic cases, `snapshot_run_due` captures only due+enabled
  roots, is deterministic under an injected `now`/last-run map, and applies CPE-1196's retention to each
  captured root). Clippy clean (default, `index`, `specta`). `npm run check` green. Moved to Done.
