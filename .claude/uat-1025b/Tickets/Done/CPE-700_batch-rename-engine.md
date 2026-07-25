---
id: CPE-700
title: Batch-rename engine + validation (pure src/lib/rename.ts)
type: feature
component: Frontend
priority: medium
status: Done
tags: ready
created: 2026-07-18
epic: CPE-699
estimate: 2-3h
---

## Summary
Child of CPE-699. The pure, headless core of batch rename: a composable **recipe** of operations applied
in order to a list of filenames, producing the old→new mapping plus preview/validation metadata. No
filesystem access, no UI — just `names[] + recipe → results[]`, mirroring the shape of `src/lib/search.ts`.
This is the land-tonight slice; the GUI (CPE-702) is a thin shell over it.

## Scope
- `src/lib/rename.ts`:
  - Operation types: find/replace (literal + regex, first/all, case-insensitive), case transform
    (lower/upper/title/sentence), insert (prefix/suffix/at index), remove (range / by substring),
    trim+collapse whitespace, sequential numbering (start/step/pad width/position), extension
    change/add/strip. Each op carries a **scope**: name-only / ext-only / whole filename.
  - `applyRecipe(names: string[], recipe: RenameRecipe): RenameResult[]` where
    `RenameResult = { from, to, changed }`. Operations compose in order; numbering counts over the input
    list order.
  - `validate(results, opts)` → per-result flags: `collision` (two targets equal, or a target equals an
    untouched sibling in the input set), `noop` (to === from), `invalid` (empty, illegal char per-OS,
    Windows reserved name). Pure over the name list — no FS.
- `src/lib/rename.test.ts`: each operation, scope handling, numbering padding/position, extension edge
  cases, collision detection (dup targets + collision with an untouched sibling), no-op, invalid names
  (illegal chars, `CON`/`NUL`), and a couple of composed recipes.

## Acceptance Criteria
- [x] `applyRecipe` transforms names per each op and composes ops in order; numbering pads/positions
      correctly.
- [x] `validate` flags collisions (dup targets AND collisions with untouched siblings), no-ops, and
      invalid/reserved names.
- [x] Pure (no FS/UI); `npm run check` + full JS suite green.

## Work Log
2026-07-18 22:30 USMST (nightshift) — Built `src/lib/rename.ts`: `RenameOp` union (replace [literal +
regex, first/all, case-insensitive], case, insert, remove-range, trim, number, extension), each with a
`Scope` (name/ext/full); `applyRecipe(names, recipe)` composes ops in order (numbering counts over input
order) → `{from,to,changed}[]`; `validate` flags collisions (dup targets, case-insensitive on win;
collisions with untouched `existing` siblings), no-ops, and invalid names (empty, control/illegal chars,
trailing dot/space, Windows-reserved CON/NUL/COM#/LPT#); `previewRename` zips the two for the GUI.
`rename.test.ts`: 18 tests (every op, scope, numbering pad/pos, extension edge cases, collision cases,
platform-specific validity, composed recipe). `npm run check` 0/0; full JS suite 726 pass. Pure/tree-
shakeable — plain explorer pays nothing. No Rust touched (clippy/cargo unaffected). Next: CPE-701 backend
`rename_many` (headless ordering/cycle logic), then CPE-702 GUI panel (attended).

## Reverted (2026-07-19)
Removed as a **duplicate**. This engine (`src/lib/rename.ts`) duplicated the pre-existing, shipped
`src/lib/batchRename.ts` used by `BatchRenameDialog.svelte`. Filed during the nightshift on the false
premise that the app only renamed one file at a time (frontend feature not checked). Per the user's
decision, `src/lib/rename.ts` + `rename.test.ts` were deleted. Superseded by the existing feature; see
CPE-702 (Duplicate) and the batch-rename-exists memory.
