---
id: CPE-1084
title: "Batch execute runner — cpe_server::batch_execute (execute_plan real-fs)"
type: feature
component: Backend
priority: medium
status: Doing
tags: ready
created: 2026-07-26
epic: CPE-723
depends-on: CPE-1083
---

## Summary
Child of CPE-723 (Batch media operations). Tie CPE-940's plan to the CPE-1083 transform engine: read each
input file, transform it, write it to its planned output — **skip-on-error**, non-destructive. Backend-only,
`cargo test` (real tempdir round-trip) on the 3-OS matrix — no GUI, no user resource, no new deps. **Depends
on CPE-1083** (`batch_transform::apply_ops`) — dispatch after CPE-1083 merges. Reuses `batch_media::{BatchJob,
PlannedItem, plan}`.

## Design (buildable)
New module `crates/server/src/batch_execute.rs`, registered `pub mod batch_execute;` in
`crates/server/src/lib.rs` at a distinct anchor (e.g. **immediately after `pub mod batch_transform;`** once
CPE-1083 has landed). Read `batch_media.rs` for `PlannedItem` (its `output` path + the ops for that item —
check the exact shape) and `plan(job, inputs)`.

```rust
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BatchReport { pub written: usize, pub skipped: Vec<(String /*input*/, String /*why*/)> }
pub fn execute_plan(items: &[PlannedItem], job: &BatchJob) -> BatchReport;
```
For each `PlannedItem`: read the input bytes → `batch_transform::apply_ops(input, <that item's ops>)` → write
the result to `PlannedItem.output` (create the parent dir). **Each failing item (read error, non-image /
decode Err, write error) is skipped and recorded in `skipped`, NEVER fatal** (preserve the `list_dir`/
`revert_engine` skip-on-error ethos). Inputs are NOT modified (non-destructive — the plan already computes
collision-safe output paths). `written` counts successes.

## ⚠ Cross-OS + real-fs test
Use the plan's already-computed output paths (don't recompute path prefixes with `std::path`). Ext compares
lowercased. **Test with a process-unique tempdir** (mirror `revert_engine`/`snapshot_capture` tests: a
`std::env::temp_dir()` subdir + atomic counter + `remove_dir_all` cleanup) — no OS-permission tricks, so the
Windows CI leg stays green.

## Acceptance Criteria
- [ ] Real tempdir with 2 valid PNGs + one non-image `.txt`: run a Resize+Convert plan → 2 output files exist
      with the correct format + dims; the `.txt` is reported in `skipped` (not fatal); the input files are
      unmodified (non-destructive).
- [ ] A missing input path → skipped with a reason; empty plan → empty report (no panic).
- [ ] `cargo test -p cpe-server` green; clippy `--all-targets -- -D warnings` clean default AND
      `--features index`; no new deps.

## Work Log
2026-07-26 (workshift) — Filed by the Product Manager as the CPE-723 execution runner (ties CPE-940's plan to
the CPE-1083 engine). Held in Backlog: depends on CPE-1083 landing first. Real-fs but fully headless (tempdir).
