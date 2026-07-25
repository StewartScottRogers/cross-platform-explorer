---
id: CPE-255
title: Batch rename (find & replace across a selection)
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 2h
created: 2026-07-13
---

## Summary

Renaming is one file at a time (F2). Add a **Rename…** action for a
multi-selection that opens a small dialog to find & replace text across all the
selected names, with a live before/after preview, then applies the change in one
undoable step. The name transform is a pure, unit-tested module; the apply path
reuses the existing `move_exact` command (same-dir rename), so nothing new is
needed on the backend.

## Acceptance Criteria

- [ ] `Rename…` appears in the context menu when 2+ items are selected.
- [ ] Dialog has Find / Replace fields, a case-sensitive toggle, and a live
      preview showing each old → new name (unchanged ones dimmed).
- [ ] Applying renames all changed items in one `move_exact` call and pushes a
      single undoable move; collisions are reported, not silently dropped.
- [ ] Pure transform module `batchRename.ts` with unit tests (replace-all,
      case-insensitive, no-op when find is empty, regex specials escaped).
- [ ] `npm run check` and the full vitest suite pass.

## Resolution

Added a pure, unit-tested `batchRename.ts` (`planFindReplace` — replace-all,
case toggle, regex-escaped literal find, intra-batch collision flagging) and a
`BatchRenameDialog.svelte` with Find/Replace fields, a case-sensitive toggle,
and a live old→new preview (unchanged dimmed, collisions in red, Rename disabled
on conflict/no-change). **Rename…** appears in the context menu for 2+ selected
items; applying issues one `move_exact` within the folder and pushes a single
undoable "rename" step. No backend change — reuses `move_exact`, which already
guards against clobbering.

## Work Log
2026-07-13 — Filed and picked up during Nightshift.
2026-07-13 — Implemented transform module + dialog + wiring. Verified: vitest 249
pass (8 new batchRename tests), npm run check clean (0/0), production build ok.
Landed on branch cpe-255-batch-rename.
