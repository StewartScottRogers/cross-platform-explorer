---
id: CPE-1084
title: "Batch execute runner — cpe_server::batch_execute (execute_plan real-fs)"
type: feature
component: Backend
priority: medium
status: Done
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
- [x] Real tempdir with 2 valid PNGs + one non-image `.txt`: run a Resize+Convert plan → 2 output files exist
      with the correct format + dims; the `.txt` is reported in `skipped` (not fatal); the input files are
      unmodified (non-destructive).
- [x] A missing input path → skipped with a reason; empty plan → empty report (no panic).
- [x] `cargo test -p cpe-server` green; clippy `--all-targets -- -D warnings` clean default AND
      `--features index`; no new deps.

## Work Log
2026-07-26 (workshift) — Filed by the Product Manager as the CPE-723 execution runner (ties CPE-940's plan to
the CPE-1083 engine). Held in Backlog: depends on CPE-1083 landing first. Real-fs but fully headless (tempdir).

2026-07-26 (workshift, Worker) — Built `crates/server/src/batch_execute.rs`. Registered
`pub mod batch_execute;` in `lib.rs` immediately after `pub mod batch_transform;` per the anchor.

**Assumption on the ambiguous "that item's ops" wording:** `PlannedItem` (in `batch_media.rs`) only
carries `{ input: String, output: String, summary: String }` — there is no per-item ops list, because a
`BatchJob` applies one ordered `Vec<MediaOp>` uniformly to every input in the batch (confirmed by reading
`batch_media::plan`, which loops the same `job.ops` over each input to derive its output path/summary).
So `execute_plan(items, job)` applies `job.ops` to every item via
`batch_transform::apply_ops(&input_bytes, &job.ops)` — there was nothing "per item" to select.

Logic: for each `PlannedItem`, `fs::read(input)` → `batch_transform::apply_ops` → `create_dir_all` the
output's parent (skipped when the output has no dir component) → `fs::write(output, ...)`. Any failure at
any step is caught and pushed to `BatchReport.skipped` as `(input, reason)`; `written` only increments on
full success. Never panics, never touches the input file.

Test tempdir mirrors `snapshot_capture.rs`'s `scratch()` helper: `std::env::temp_dir()` +
`cpe-batchexec-{tag}-{pid}-{atomic-counter}` + `create_dir_all`, cleaned up with `remove_dir_all` at the
end of each test. The 2 PNGs are built in-memory with `image::RgbImage::from_pixel(...).write_to(...,
ImageFormat::Png)` (same pattern as `batch_transform.rs`'s own tests) then written to disk with
`fs::write`; the plan is built via the real `batch_media::plan(&job, &inputs)` (not hand-built
`PlannedItem`s) so the test exercises the actual planner→executor handoff. Asserts: 2 outputs exist as
JPEG (`image::guess_format` + decode) and respect the 16px resize cap; the `.txt`'s planned output was
never created; all 3 inputs are byte-for-byte unchanged after the run. Separate tests cover a missing
input (skipped with a non-empty reason) and an empty item slice (default `BatchReport`, no panic).

Verify: `cargo test` in `crates/server` → 984 passed, 0 failed. `cargo clippy --all-targets -- -D
warnings` clean; `cargo clippy --all-targets --features index -- -D warnings` clean. No new dependencies
(only `std::fs`/`std::path`, `image` and `batch_media`/`batch_transform` already vendored). One clippy fix
needed mid-build: `&[missing.clone()]` → `std::slice::from_ref(&missing)`
(`cloned_ref_to_slice_refs`).

No blockers. PR opened against `main`.
