---
id: CPE-359
title: "Sort option: toggle 'folders first' vs. mixed"
type: Feature
status: Done
closed: 2026-07-14
priority: Low
component: Frontend
created: 2026-07-14
---

## Summary

The listing always sorts folders before files. Add a **Group folders first** toggle (in the
View menu) so a user can sort everything together alphabetically when they prefer. Persisted.

## Design (frontend)
- `sort.ts`: `compareEntries`/`sortEntries` gain a `foldersFirst = true` param; when false, the
  directories-first rule is skipped. Unit-tested (both modes).
- `App.svelte`: `foldersFirst` state (loaded/saved), threaded into the `visible` sort.
- `CommandBar.svelte`: a "Group folders first" checkbox in the View dropdown.
- `settings.ts`: persist `cpe.foldersFirst`.

## Acceptance
- Toggling off interleaves folders and files by the active sort key; on restores folders-first;
  the choice persists. `npm run check` + `npm test` green.

## Work Log
2026-07-14 — Filed during Nightshift (loop 5). A common file-manager preference we lacked.

2026-07-14 — Implemented on branch `CPE-359-folders-first-toggle`.
- `sort.ts`: `compareEntries`/`sortEntries` take `foldersFirst = true`; false skips the
  directories-first rule (+2 tests covering both modes).
- `settings.ts`: `cpe.foldersFirst`. `App.svelte`: state loaded/saved, threaded into `visible`.
- `CommandBar.svelte`: "Group folders first" checkbox in the View menu.
- `npm run check` 0 errors; suite 317 pass; `npm run build` ok.
