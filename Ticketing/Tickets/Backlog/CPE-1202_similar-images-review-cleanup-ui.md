---
id: CPE-1202
title: "Similar-images review + safe-cleanup UI (thumbnails, keeper-guarded move-to-bin)"
type: feature
component: Frontend
priority: medium
status: Backlog
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
- 2026-08-01 — Filed by Foreman (workshift, epic CPE-997). Depends on CPE-1201. opus (L, safety-sensitive).
