---
id: CPE-1325
title: Metadata Studio — best-effort checkpoint before save
type: feature
component: Frontend
priority: medium
tags: ready
created: 2026-08-05
epic: CPE-725
estimate: 1-2h
status: Done
---

## Summary
`MetadataStudioDialog.svelte` mutates the target file(s) in place via `metadataWrite` — there is no undo
if a bad edit is saved. `SimilarImagesDialog.svelte` already takes a best-effort `checkpointCreate` of the
target folder before a destructive bulk move; Metadata Studio's save path should do the same so a bad
metadata edit stays recoverable.

## Acceptance Criteria
- [x] Before `metadataWrite` runs, `save()` takes a best-effort `commands.checkpointCreate` covering the
      target file(s)' folder.
- [x] Best-effort, non-blocking: a checkpoint failure does NOT abort the save — it's logged
      (`console.error`) and the write proceeds. A subtle status suffix ("(checkpoint saved)") is shown on
      the existing "Saved" notice when the checkpoint succeeded — no modal/blocking prompt either way.
- [x] Applies to both single-file and `applyToAll`/batch writes: the checkpoint is taken ONCE before the
      batch begins, not per-field or per-file.
- [x] No new backend command, no new dependency — reuses `commands.checkpointCreate` exactly as
      `SimilarImagesDialog` already calls it.
- [x] New i18n string (`studio.checkpointed`) added to all 12 locales in `src/lib/i18n.ts`.
- [x] New vitest coverage (`MetadataStudioDialog.test.ts`) asserting: the checkpoint is attempted before
      `metadataWrite`, a rejected checkpoint does not block the write, and a batch save checkpoints exactly
      once (not per-file).
- [x] `npm run check` clean; `npx vitest run src/lib/components/MetadataStudioDialog` green; full
      `npx vitest run` green (181 files / 2007 tests).

## Work Log
2026-08-05 (sprint) — Implemented. `save()` in `src/lib/components/MetadataStudioDialog.svelte` now
calls `commands.checkpointCreate(parentDir(primary.path), "Before metadata edit")` once before the write
loop, wrapped in try/catch so a rejection only logs and proceeds. Success is surfaced via a translated
suffix appended to the existing save notice. Added `studio.checkpointed` to all 12 locales and a new test
file with 3 cases (single-file ordering, rejected-checkpoint non-blocking, batch-once). Frontend-only, no
Rust/bindings changes.
