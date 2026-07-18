---
id: CPE-613
title: "EPIC: File transfer & operations manager"
type: Task
status: Proposed
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
