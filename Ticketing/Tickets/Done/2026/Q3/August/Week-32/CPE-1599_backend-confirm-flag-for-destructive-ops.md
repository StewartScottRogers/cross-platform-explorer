---
id: CPE-1599
title: "Defence in depth: require an explicit confirmed flag in the backend for irreversible in-place operations"
type: Task
status: Backlog
priority: Medium
component: Backend
tags: [ready]
created: 2026-08-10
closed:
---

## Why
Raised by the independent reviewer on CPE-1590 (PR #805), which added a confirmation gate before Batch
Media overwrites a user's originals in place. That gate works and no in-app path bypasses it today — the
reviewer traced every caller — but it is a **pure frontend state invariant** (`showOverwriteConfirm` in
`BatchMediaDialog.svelte`). The engine has no idea a confirmation was ever required:

- `batch_media_execute_stream` (`src-tauri/src/lib.rs`) and `execute_plan_walk`
  (`crates/server/src/batch_execute.rs`) take **no "confirmed" flag of any kind** and will happily execute
  an in-place-overwriting plan from any caller.
- A devtools call, a future automation/agent feature, a macro system extension, or a new UI surface that
  invokes the command directly would skip the gate entirely with **zero server-side objection**.

This is not a new weakness introduced by CPE-1590 — it matches an existing repo-wide pattern (`shred_paths`
/ `secure_shred::shred_paths`, irreversible secure delete, has no backend confirm flag either). But
in-place overwrite is uniquely unrecoverable: unlike delete, there is no trash fallback at all.

## Goal
Move the safety invariant from "the UI remembered to ask" to "the engine refuses unless told explicitly".

## Scope
- Add an explicit confirmation flag to the batch job contract — e.g. `BatchJob.confirmed_overwrite: bool`
  — and make `execute_plan_walk` **refuse** any plan where `plan()` resolves `output == input` unless it is
  set. A refusal must be a clean, specific error, not a panic.
- Thread it through the Tauri command, regenerate `src/lib/bindings.gen.ts`, and set it from the existing
  confirm panel (the only place that should ever set it).
- Do the same review for `shred_paths` and decide, with a one-line rationale in the work log, whether it
  gets the same treatment now or a follow-up ticket.
- Tests: an in-place plan without the flag is refused by the engine; with the flag it proceeds; a
  non-destructive plan is unaffected either way.

## Related, from the same review
`checkpoint_create` currently calls `snapshot_capture::capture` with `CaptureBudget::UNLIMITED`, so a
successful checkpoint really does capture every file. A comment at `crates/server/src/checkpoint_store.rs`
notes a later ticket may thread a real budget cap through the same signature — if that lands, the Batch
Media call site (which discards `CheckpointCreated.skipped` without inspecting it) would start silently
omitting oversize files from "recovery". Whoever adds that budget must make this call site inspect
`skipped` and surface it.

## Notes
Conflict surface: `crates/server/src/batch_execute.rs`, the batch job DTO in `crates/server/src/model.rs`,
`src-tauri/src/lib.rs`, `src/lib/bindings.gen.ts`, `BatchMediaDialog.svelte`. Model: sonnet.

## Work Log
2026-08-10 21:34 USMST — Implemented the engine-side gate:
- `BatchJob.confirmed_overwrite: bool` (`crates/server/src/batch_media.rs`, `#[serde(default)]` so it's
  optional on the wire, defaults `false` via `BatchJob::new`).
- `crates/server/src/batch_execute.rs`: `execute_plan_walk`/`execute_plan` now return
  `Result<BatchReport, String>` and refuse up front (nothing read, nothing written, `flush` never called)
  when any planned item has `output == input` and `confirmed_overwrite` is unset — a specific, named-count
  error, not a panic or silent skip. New `any_in_place` helper. 3 new tests: refuses unconfirmed in-place,
  proceeds once confirmed, non-destructive plan unaffected by the flag either way.
- `src-tauri/src/lib.rs`'s `batch_media_execute_stream` propagates the `Result` straight through to the
  frontend (no behavior change needed there — it was already `Result`-returning).
- Frontend: `src/lib/batchMedia.ts` gained `confirmOverwriteJob()`, the **single named seam** allowed to
  set the flag true; `BatchMediaDialog.svelte`'s `apply()` calls it only when reached via the confirm
  panel's own "Overwrite N files" button (`needsOverwriteConfirm` true), never anywhere else. `opsToJob`
  always builds `confirmed_overwrite: false`.
- Also closed the `checkpoint_create` `.skipped`-discarded gap flagged in "Related": the Batch Media
  pre-overwrite checkpoint call site now inspects `CheckpointCreated.skipped` and, if non-empty, feeds the
  folder into the SAME `checkpointFailures` warning path CPE-1590 built for an outright checkpoint
  failure (dialog panel + `App.svelte`'s `noticeCheckpointFailures`), reworded to "didn't fully cover"
  since a partial-skip checkpoint did partially succeed — no new panel/notice function added.

**`shred_paths` decision: follow-up ticket, not this PR.** Rationale: the two gaps are only superficially
identical. `batch_media`'s danger is *conditional* — `plan()` can resolve `output == input` for some op
combos and not others, so a caller (including the confirm panel itself) has to inspect the concrete plan
to know if a given run is destructive; that's exactly the kind of fact a backend invariant should pin down
independently of the caller's own bookkeeping. `shred_paths` has no such fork: **every** call is
unconditionally, 100% destructive by definition — there is no "safe plan" a caller could mistake for a
dangerous one. Adding a `confirmed: bool` there is a smaller mechanical change (no plan-inspection needed,
just thread a bool to the one function), but it's still its own reviewed change — this PR's conflict
surface (per the ticket Notes) doesn't list `secure_shred.rs`/`ShredConfirmDialog.svelte`, and bundling an
unrelated command's signature change into an already-large batch-media diff isn't worth the risk of
scope creep. Filed CPE-1611 to track it.

