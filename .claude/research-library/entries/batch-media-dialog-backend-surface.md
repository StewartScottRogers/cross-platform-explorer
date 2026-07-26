---
title: "What backend surface exists for a Batch-Media dialog (resize/convert/rotate over many files) and what must be built?"
date: 2026-07-26
tags: [batch-media, image-transform, tauri-command, streaming, ipc-channel, dialog, batch-rename, gui, cpe-723]
status: current
---

## Question
For an upcoming Batch-Media dialog (apply resize/convert/rotate/flip/rename/strip-metadata to many selected
image files, with progress), what backend already exists, what must be added, and what dialog pattern do we reuse?

## Finding (short)
The **pure engine is fully built and tested** but wired to **no `#[tauri::command]`, no specta binding, nothing
in `bindings.gen.ts`**. So GUI #2 is two tickets: (1) a small **backend enablement** ticket exposing plan +
streamed execute, then (2) the **frontend dialog** modelled on `BatchRenameDialog`.

## Backend building blocks (all in `crates/server/src/`, `pub mod` at lib.rs:111–121, NONE bound to a command)
- `batch_media.rs` (CPE-940) — planner, pure/no-I/O:
  - `enum MediaOp { Resize{max_px:u32}, Convert{to_ext:String}, Rotate{degrees:u16}, Flip{horizontal:bool}, Rename{template:String}, StripMetadata }` (`#[serde(tag="op", rename_all="snake_case")]`) — **order matters, ops apply left-to-right**.
  - `struct BatchJob { ops: Vec<MediaOp>, non_destructive: bool }` (default non_destructive=true).
  - `struct PlannedItem { input, output, summary }`; `fn validate(&BatchJob)->Result<(),String>`; `fn plan(&BatchJob,&[String])->Vec<PlannedItem>` (collision-safe non-destructive output paths + one-line summary per file — this IS the dialog's preview data).
- `batch_transform.rs` (CPE-1083) — `fn apply_ops(&[u8],&[MediaOp])->Result<Vec<u8>,String>`; decode-bomb guarded (20000px / 256MiB caps); encoders png/jpg/jpeg/gif/webp/bmp/tif/tiff only (heic/avif/psd/svg → graceful Err); StripMetadata bakes EXIF orientation into pixels first.
- `batch_execute.rs` (CPE-1084) — `struct BatchReport { written:usize, skipped:Vec<(String,String)> }`; `fn execute_plan(&[PlannedItem],&BatchJob)->BatchReport`; skip-on-error per file, never overwrites input. **Blocking, returns one report at the end — no progress hook.**

## The gap → backend enablement sub-ticket (blocks the GUI ticket)
No command calls plan/validate/execute; dialog can't compute the plan client-side (path/collision logic is Rust-only). Need:
1. `#[tauri::command] async fn batch_media_plan(job:BatchJob, inputs:Vec<String>)->Result<Vec<PlannedItem>,String>` (validate→plan), thin dispatcher like `move_exact` (lib.rs:1814-1819).
2. `#[tauri::command] async fn batch_media_execute_stream(items:Vec<PlannedItem>, job:BatchJob, on_result:tauri::ipc::Channel<Vec<OpResult>>)->Result<BatchReport,String>` — **exact shape of `apply_backup_plan_stream` (lib.rs:2005-2029)**. Requires adding an `execute_plan_walk(items,job,flush:impl FnMut(OpResult))` variant to `batch_execute.rs` (keep `execute_plan` for tests — STREAMING.md "one walker, both callers", like list_dir/list_dir_stream). Reuse existing `OpResult { path, ok, error }` (model.rs:33-37), don't invent a streamed type.
3. Register both in `generate_handler!` (~lib.rs:5906) + specta list (~lib.rs:6307), regen `bindings.gen.ts`.
4. Optional: `cancel_batch_media` flag (mirror `cancel_transfer`/`cancel_dir_stream`) so closing the dialog mid-run stops it.

## Reuse template: BatchRenameDialog
- `src/lib/components/BatchRenameDialog.svelte` + `src/lib/batchRename.ts`. Opened by `App.svelte:beginBatchRename()` (1559-1562, gates selectionCount≥2), rendered at App.svelte:3487-3493, dumb dialog: string props in, `apply`/`cancel` events out; reactive pure planner `$: items=...`; scrollable `from→to` preview with conflict marking; parent `applyBatchRename()` (1566-1592) calls the typed command + `reportResults` + undo push + `loadPath` refresh.
- Context menu: `ContextMenu.svelte:132-136` `{#if selectionCount>1}` batch-rename row; dispatched in App.svelte switch (~2243). Add "Batch media…" the same way (guard to all-image selections).
- Dialog styling convention (repo-wide, also in MetadataStudioDialog.svelte:219-223): `.dialog { background:var(--surface); border:1px solid var(--border-strong); border-radius:10px; box-shadow:0 20px 50px rgba(0,0,0,.25) }` — visible border, all colours from theme vars.

## Progress model tradeoff (decide at ticket-cut)
- **ipc::Channel + dialog-scoped progress bar** (recommended — dialog stays open, user watches; reuse TransferPanel `.bar/.fill` percent pattern inside the dialog; call via `rawInvoke` + `createChannel<OpResult[]>()` per BUSY-CURSOR/STREAMING). vs.
- **window-event + corner TransferPanel** (fire-and-forget, dialog closes, runs in background — like copy/move transfers, transfers.ts:86-87). Pick one; it changes both command shape and component structure.

## Dialog UX (v1, once commands exist)
Ordered **op pill list** (add-op dropdown → append; each pill shows param inline `Resize 1024px ✕`; reflow per tick-tacks rule) instead of BatchRename's single-mode radios; non-destructive checkbox (default on); debounced (~200ms) async `batchMediaPlan` preview of `input→output — summary` rows; inline `validate` error near Apply; Apply streams execute with live `X/Y done, Z failed` + progress bar; pre-filter non-image/unsupported extensions out of the input set with an "N files will be skipped" notice; no destination picker in v1 (planner writes non-destructive siblings); refresh listing after.

## Risks
Streaming-vs-event model choice; narrow format coverage (pre-filter heic/avif/psd/svg); debounce live-preview IPC; keep `execute_plan` as the cargo-test correctness path; consider mid-run cancel.
