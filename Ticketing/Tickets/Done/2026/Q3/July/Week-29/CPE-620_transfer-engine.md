---
id: CPE-620
title: Backend transfer engine — streamed copy/move with progress + cancel
type: feature
component: Backend
priority: high
status: Done
tags: ready
estimate: 3-4h
created: 2026-07-18
closed: 2026-07-18
epic: CPE-613
---

## Summary
First child of the transfer-manager epic (CPE-613). A streamed copy/move engine with byte-level
progress, cancellation, and a per-batch conflict policy — the foundation the operations panel + wiring
build on. Pure core (`run_transfer`) is headlessly unit-tested; the async `start_transfer` command is a
thin thread wrapper that forwards progress as Tauri events.

## Acceptance Criteria
- [x] `run_transfer(id, sources, dest, kind, policy, cancel, emit)` copies/moves with byte progress,
      polls a cancel flag between chunks, and returns a `TransferReport` (transferred/skipped/failed/
      cancelled/errors). Streamed in 128 KiB chunks; symlinked dirs not descended (cycle-safe).
- [x] Conflict policy per batch: overwrite / skip / keep-both (auto-numbered via `unique_target`).
- [x] Move uses a same-volume rename fast path and deletes the source only after a fully-successful
      copy (never on partial failure); cross-volume falls back to stream-copy + delete.
- [x] `start_transfer` (async, thread) emits `transfer://progress` + `transfer://done`; `cancel_transfer`
      signals a live transfer via an id→flag registry. Both registered in `generate_handler!`.
- [x] Cargo tests cover copy+progress, the three conflict policies, move-removes-source, and cancel.
      Clippy clean (both feature modes). Byte assertions use file *content* lengths (portable).

## Resolution
Added the engine + `start_transfer`/`cancel_transfer` to `src-tauri/src/lib.rs` with 4 cargo tests.
Reuses `unique_target` + `is_self_or_descendant`. Frontend wiring (store, panel, conflict dialog,
routing copy/paste through it) follows in CPE-621+.

## Work Log
2026-07-18 (dayshift) — Built + verified the backend foundation. Decisions per the CPE-613 activation
(byte-level progress, window-scoped, core, cancel-only, overwrite/skip/keep-both).
