---
id: CPE-1187
title: "Macro executor + undo model (headless) with scope guard"
type: feature
component: Backend
priority: medium
status: Doing
tags: ready
created: 2026-07-31
epic: CPE-739
---

## Summary
Part of CPE-739. The macro model (`action_macro.rs` `plan()`), library (`macro_library.rs`), and store
(`macro_store.rs`) exist, but **nothing runs a macro** — `plan()` emits `PlannedOp`s that are never applied.
Build the pure resolution + inverse (undo) + scope-guard layer so a multi-step macro run is reversible and
can't escape the working root. (Actual disk writes happen in the CPE-1188 command.)

## Build
- New `crates/server/src/macro_run.rs` (+ `mod` line in `crates/server/src/lib.rs`): given `(ActionMacro,
  inputs)` or a `Vec<PlannedOp>`, produce a **resolved, collision-safe** ordered op list plus a per-op
  **inverse** record so the run is reversible; and a **scope check** (reuse `op_plan::within_root`-style logic)
  rejecting any resolved dest outside the working root. Pure logic — no disk writes here.
- Deterministic collision-safe naming; imported macros never auto-run (that gate lives in the command/UI).

## Acceptance Criteria
- [ ] `cargo test -p cpe-server`: inverse of a rename/move/tag/convert sequence round-trips (apply-then-invert
      restores state, tested at the resolution level); out-of-root dest rejected; collision-safe naming
      deterministic.
- [ ] `cargo clippy --all-targets -D warnings` clean (both feature modes).

## Work Log
- 2026-07-31 — Filed by Foreman (workshift, epic CPE-739). Backend phase; built with CPE-1188 by one worker.
