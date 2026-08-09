---
id: CPE-1202
title: "Similar-images review + safe-cleanup UI (thumbnails, keeper-guarded move-to-bin)"
type: feature
component: Frontend
priority: medium
status: Done
tags: ready
created: 2026-08-01
epic: CPE-997
---

## Summary
Part of CPE-997 (after CPE-1201). A review UI for near-duplicate image groups with SAFE cleanup. Deletes user
files → must be recoverable + keeper-guarded.

## Build
- New `src/lib/components/SimilarImagesDialog.svelte` paralleling `DuplicatesDialog.svelte`: streaming append
  via `createChannel` + generation-token supersede (copy `DuplicatesDialog.svelte`'s `searchGen` pattern);
  groups rendered **side-by-side with thumbnails** (reuse the existing thumbnail path); per-image keeper
  selection.
- **SAFETY (mandatory):** cleanup uses `commands.deleteToTrash` (recoverable Bin, NEVER hard delete), guarded
  by `keepsOnePerGroup` so at least one copy per group always survives; **default select nothing** (no
  auto-select-all); consider a `checkpointCreate` before bulk removal. "Move to Bin" disabled when a selection
  would delete all copies of a group.
- Opener: context-menu item + palette entry (`tool.findSimilarImages`, parallel to `tool.findDuplicates`) +
  `App.svelte` mount. i18n keys; in-app docs section + `sectionDocs.ts` entry ([[maintain-in-app-docs-library]]).

## Acceptance Criteria
- [ ] gui-smoke (CPE-1203): two seeded near-duplicate images → scan renders exactly one group with both,
      thumbnails visible, keeper-selection toggles, "Move to Bin" disabled when it would delete all copies.
- [ ] `npm run check` + `npm test` green.

## Notes
- **USER-GATED / attended:** the destructive-cleanup interaction sign-off is attended per the epic — surface the
  safety flow (recoverable trash + keeper guard + no auto-select) to the user for confirmation.

## Work Log
- 2026-08-01 — Filed by Foreman (sprint, epic CPE-997). Depends on CPE-1201. opus (L, safety-sensitive).
- 2026-08-01 — Done (Worker, sprint). Built `src/lib/components/SimilarImagesDialog.svelte`, the perceptual
  complement of `DuplicatesDialog.svelte`. Streaming scan over an `ipc::Channel` via `rawInvoke(
  "find_similar_images_stream")` + `createChannel` (streaming opts out of the busy cursor and the typed
  `commands.*Stream` client can't accept a transport-agnostic `StreamChannel` — every streaming call site in
  the repo uses `rawInvoke`, so this matches convention). Generation-token supersede (`searchGen`) drops a
  stale in-flight scan's late batch. Groups render side-by-side with `ThumbnailImage` thumbnails; per-image
  keeper selection.
  **SAFETY DESIGN (all mandated rails):** (1) cleanup calls `commands.deleteToTrash` — recoverable Recycle
  Bin, NEVER a hard delete; (2) guarded by `keepsOnePerGroup` (reused from `duplicates.ts`) so at least one
  copy per group always survives; (3) selection starts EMPTY — no auto-select-all (a "Select extras" helper
  ticks all-but-first, which can never arm a delete-all); (4) "Move to Bin" is DISABLED whenever the current
  selection would remove every copy of any group; (5) a best-effort `commands.checkpointCreate` is taken
  before the bulk move (failure never blocks the already-recoverable trash move). **EMPTY-RESULT FIX
  (reviewer note):** image clustering is whole-set, so an empty scan streams NO batch — `loading` is cleared
  on the awaited stream resolution in `finally`, not solely on the first batch.
  Opener wiring mirrors `tool.findDuplicates`: `tool.findSimilarImages` command-palette entry + `App.svelte`
  mount + `find-similar-images` MenuBar Tools item + `onMenuSelect` case. i18n: 15 `sim.*` keys +
  `palette.findSimilarImages` + `mi.findSimilarImages` added to ALL 12 complete locales (coverage gate green).
  Docs: new `src/docs/18-similar-images.md` (auto-registered in DOCS; no new `Section` since the dialog is a
  tool overlay, not a sidebar view — same as DuplicatesDialog, which has no sectionDocs entry either).
  Tests: `SimilarImagesDialog.test.ts` (5 tests) — streaming append + generation-token supersede;
  empty-result clears loading; keeper-guard disables Move-to-Bin when the selection would delete all copies;
  `deleteToTrash` called only for the redundant (non-kept) path, after a checkpoint. `npm run check` 0
  errors; full vitest 1569 passed.
