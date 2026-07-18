---
id: CPE-613
title: "EPIC: File transfer & operations manager"
type: Task
status: In Progress
priority: High
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

## Goal

Turn copy/cut/paste/move/delete from fire-and-forget calls into a **managed operation queue** with
visible progress, conflict resolution, and cancellation — the single biggest gap between this app and a
"real" file manager. Today a large copy blocks with only the busy cursor; a name collision is handled
per-item without a batch choice; there's no way to see, pause, or cancel an in-flight transfer.

## Why

Moving/copying big trees is the core job of a file explorer. Users need to *see* what's happening
(bytes, files, ETA), decide **once** how to handle collisions ("replace all / skip all / keep both"),
and cancel or background a long transfer while they keep browsing. This is also where data-loss bugs
hide, so a deliberate engine with tests is safer than ad-hoc calls.

## Rough scope (areas, not child tickets)

- A backend transfer engine: streamed copy/move with per-operation progress events, cancellation
  tokens, and resilient skip-on-error (preserve the `list_dir` ethos).
- An operations panel/drawer: live queue of running + queued + finished ops, each with progress, a
  cancel button, and an error summary; collapses out of the way when idle.
- Batch conflict resolution: one dialog offering replace / skip / keep-both / rename, with
  "apply to all remaining", surfaced *before* the bytes move.
- Move-vs-copy correctness across volumes (rename within a volume, copy+delete across); verify vs
  same-tree guards (already have `copy_dir_all` descendant protection — reuse it).
- Undo integration: a completed transfer is undoable via the existing undo stack where feasible.

## Open questions (resolve at activation)

- Progress granularity: per-file vs byte-level? (byte-level needs streamed copy + periodic events.)
- Do we background transfers across tab/window changes, or scope to the window?
- How much of this lives in the sidecar-platform vs core? (Core, likely — it's fundamental.)
- Pause/resume: nice-to-have or in-scope for v1?

## Definition of Done

- Copying/moving a large tree shows live progress and can be cancelled mid-flight without corruption.
- A batch of collisions is resolved with a single choice ("apply to all").
- Errors on individual items don't abort the whole operation; they're reported at the end.
- The plain explorer stays fast/small/predictable when no transfer is running (the panel is idle-hidden).
- Engine has headless tests (progress accounting, cancellation, collision decisions, cross-volume move).

## Decisions (2026-07-18, activated in dayshift — user away, best-guess defaults logged)
- **Progress granularity:** byte-level (streamed copy in fixed chunks, periodic progress events). Gives
  a real progress bar + ETA; the streaming loop also gives a natural cancellation checkpoint.
- **Scope of a transfer:** window-scoped for v1 (a running transfer belongs to the app session; not
  persisted across restart). Simpler + safe.
- **Where it lives:** core (`src-tauri/src/lib.rs`) — file transfer is fundamental, not a Mega-Feature
  sidecar.
- **Pause/resume:** OUT of v1. Cancel only (a cancel flag checked in the copy loop).
- **Conflict policy v1:** overwrite / skip / keep-both (auto-numbered "name (2)"), chosen per-batch
  ("apply to all"). Per-item rename can come later.

## Child tickets (created just-in-time as each is worked, in dayshift)
1. CPE-620 — Backend transfer engine: streamed copy/move core + conflict policy + cancel flag, cargo-tested.
2. CPE-621 — Async `start_transfer`/`cancel_transfer` commands emitting `transfer://progress` events.
3. CPE-622 — Frontend transfer store consuming the progress events (pure, testable).
4. CPE-623 — Operations panel UI: live queue with progress bars + cancel button (idle-hidden).
5. CPE-624 — Batch conflict dialog ("apply to all remaining").
6. CPE-625 — Route copy/cut/paste + move through the engine.
7. CPE-626 — Undo integration for a completed transfer.
8. CPE-627 — Docs: in-app docs + design note.
