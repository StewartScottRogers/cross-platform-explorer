---
id: CPE-917
title: Checkpoint restore-plan diff engine (revert compute core)
type: feature
component: Backend
priority: low
tags: ready
epic: CPE-732
created: 2026-07-22
closed: 2026-07-22
status: Done
---

## Summary
First headless slice of checkpoint & rollback (CPE-732). `cpe_server::restore_plan` diffs a checkpoint
`Snapshot` (path → content-addressed `FileState{hash,size}`) against the current tree state and computes the
minimal revert ops:
- `plan_restore(checkpoint, current) -> Vec<RestoreAction{path, op}>` — `RestoreOp::{Create (restore
  deleted), Overwrite (revert modified), Delete (remove created-after)}`; unchanged files (same hash) are
  skipped; sorted by path. Doc-notes the execution order (parents-first writes, deepest-first deletes).
- `revert_one(path, …)` — single-mutation cherry-revert from a timeline entry.
- `summarize_plan(plan, checkpoint) -> RestorePlanSummary{creates, overwrites, deletes, bytes_written}` for
  the confirm dialog.

The revert engine executes the plan (respecting the skip-unreadable rule); the restore UI previews it.

## Acceptance Criteria
- [x] Create/Overwrite/Delete detection; identical trees → empty plan; cherry-revert of one path.
- [x] Plan summary counts + bytes-written from the checkpoint. 3 unit tests; clippy `-D warnings` clean.

## Work Log
- 2026-07-22 — The pure compute core of rollback. The snapshot capture/store (content-addressed, deduped),
  the revert engine, and the checkpoint-marker + restore UI are the remainder of CPE-732.
