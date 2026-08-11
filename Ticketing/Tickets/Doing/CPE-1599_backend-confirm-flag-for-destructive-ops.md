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
