---
id: CPE-1092
title: "Batch-media commands — plan/validate + streamed execute (GUI #2 enablement)"
type: feature
component: Backend
priority: high
status: Done
tags: ready
created: 2026-07-26
epic: CPE-723
---

## Summary
Backend enablement for the Batch-Media dialog (GUI #2). The pure engine is fully built and tested —
`batch_media` (planner, CPE-940), `batch_transform` (bytes→bytes, CPE-1083), `batch_execute` (fs runner,
CPE-1084) — but wired to **no `#[tauri::command]`, no specta binding, nothing in `bindings.gen.ts`**. Add the
thin dispatchers so the frontend can (a) compute a live plan/preview and (b) run the batch with streamed
progress. Backend only; the dialog is CPE-1093.

## Context (verified — file:line)
- `crates/server/src/batch_media.rs` — `enum MediaOp {Resize{max_px:u32}, Convert{to_ext:String},
  Rotate{degrees:u16}, Flip{horizontal:bool}, Rename{template:String}, StripMetadata}`,
  `struct BatchJob {ops:Vec<MediaOp>, non_destructive:bool}`, `struct PlannedItem {input,output,summary}`,
  `fn validate(&BatchJob)->Result<(),String>`, `fn plan(&BatchJob,&[String])->Vec<PlannedItem>`.
- `crates/server/src/batch_execute.rs` — `struct BatchReport {written:usize, skipped:Vec<(String,String)>}`,
  `fn execute_plan(&[PlannedItem],&BatchJob)->BatchReport` (blocking, skip-on-error, never overwrites input).
- `OpResult {path,ok,error}` at `crates/server/src/model.rs:33-37` — REUSE for streamed per-file outcomes.
- Command patterns to mirror: `move_exact` thin dispatcher (`src-tauri/src/lib.rs:1814-1819`, registered
  ~5906 + specta ~6307) and the streaming `apply_backup_plan_stream` (`src-tauri/src/lib.rs:2005-2029`) which
  batches `Vec<OpResult>` over a `tauri::ipc::Channel`.
- Conventions: async + spawn_blocking for any fs work ([[async-all-blocking-commands]]); STREAMING.md
  "one walker, both a collect and a streaming caller" (keep the sync `execute_plan` for tests).

## Design (buildable)
1. **`batch_media_plan`** — `#[tauri::command] async fn batch_media_plan(job: BatchJob, inputs: Vec<String>)
   -> Result<Vec<PlannedItem>, String>`: call `validate` (map `Err` string straight through), then `plan`.
   Pure/in-memory but keep it `async` for convention parity. Thin one-liner into `cpe_server::batch_media`.
2. **Streaming walker in `batch_execute.rs`** — add `pub fn execute_plan_walk(items, job, mut flush: impl
   FnMut(OpResult))` that runs the SAME per-file logic as `execute_plan` but calls `flush(OpResult{..})` after
   each file (ok → `{path:output, ok:true, error:None}`; skip → `{path, ok:false, error:Some(reason)}`), and
   returns the aggregate `BatchReport`. Refactor `execute_plan` to call `execute_plan_walk` with a
   no-op/collecting flush so there is ONE implementation (no logic drift). Existing `execute_plan` tests must
   still pass unchanged.
3. **`batch_media_execute_stream`** — `#[tauri::command] async fn batch_media_execute_stream(items:
   Vec<PlannedItem>, job: BatchJob, on_result: tauri::ipc::Channel<Vec<OpResult>>) -> Result<BatchReport,
   String>`: mirror `apply_backup_plan_stream` — run `execute_plan_walk` inside `spawn_blocking`, buffer
   `OpResult`s and `on_result.send(batch)` in chunks (e.g. 16, matching the backup stream), flush the tail,
   return the final `BatchReport`. (Cancellation is out of scope for v1 — note it for a follow-up.)
4. **Register + regen** — add both commands to `generate_handler![]` (near `move_exact`) AND the specta
   command list; regenerate `src/lib/bindings.gen.ts` via the committed export path
   (`cargo run --bin export_bindings --features "specta-bindings sidecar-platform"` from `src-tauri/`), so
   `batchMediaPlan`, `batchMediaExecuteStream`, `BatchJob`, `MediaOp`, `PlannedItem`, `BatchReport` (+ their
   nested types) appear. Ensure the `#[cfg_attr(feature="specta", derive(specta::Type))]` is present on the
   structs/enums that cross the boundary (add where missing, plain-derive parity).

## ⚠ Notes / guardrails
- No new deps. Async + spawn_blocking for the fs-touching stream. Keep `execute_plan` (sync) as the
  cargo-test correctness path — the walker is an addition, not a replacement.
- `on_result` is a raw transport channel — do NOT route it through the busy-cursor wrapper (the dialog shows
  its own progress). The command registration is normal.
- The drift-guard test (`typed_bindings_are_committed_and_routed_through_busy_cursor`) must pass with the
  regenerated bindings committed.

## Acceptance Criteria
- [ ] `batch_media_plan` returns `validate` errors as `Err(String)` and otherwise the planned items;
      `batch_media_execute_stream` streams `Vec<OpResult>` batches over the channel and returns the aggregate
      `BatchReport`. New tempdir round-trip test proves streamed outcomes match a direct `execute_plan` run
      (same written count + same skips), plus an empty-input case (no panic, empty report).
- [ ] `execute_plan` refactored to share `execute_plan_walk`; all existing batch_execute tests still green.
- [ ] Both commands in `generate_handler!` + specta list; `bindings.gen.ts` regenerated with the new
      bindings/types; drift-guard test passes; `npm run check` clean.
- [ ] `cargo test -p cpe-server` green; clippy `--all-targets -- -D warnings` clean (default AND
      `--features index`); `cargo build` (app) succeeds; no new deps.

## Work Log
2026-07-26 (workshift, GUI) — Filed by the Foreman as the backend enablement for GUI #2 (batch-media dialog),
from the Researcher brief filed in the Library
(`.claude/research-library/entries/batch-media-dialog-backend-surface.md`). Blocks CPE-1093 (the dialog).
