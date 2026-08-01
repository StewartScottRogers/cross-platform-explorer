---
id: CPE-1198
title: "Snapshot schedule rules store + pure due() policy + snapshot_run_due command (no timer)"
type: feature
component: Backend
priority: medium
status: Doing
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
- [ ] `cargo test -p cpe-server`: rule CRUD round-trips to disk; `due()` returns exactly the roots past their
      interval and excludes disabled ones; `snapshot_run_due` captures due roots + prunes to policy;
      deterministic with an injected `now`.
- [ ] clippy both modes clean; bindings regenerated (drift guard green).

## Work Log
- 2026-07-31 — Filed by Foreman (workshift, epic CPE-735). Backend batch (with 1196 + 1197-backend), one worker
  sequential. Consumes 1196's retention apply (soft). Prereq for CPE-1199.
