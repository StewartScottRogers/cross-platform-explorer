---
id: CPE-360
title: "Select by pattern (glob) from the context menu"
type: Feature
status: Done
closed: 2026-07-14
priority: Low
component: Frontend
created: 2026-07-14
---

## Summary

Add **Select by pattern…** to the empty-area context menu: a small dialog takes a glob
(`*.log`, `IMG_????.jpg`) and selects every visible entry whose name matches — a power-user
complement to the existing "Select all .ext".

## Design (frontend)
- `glob.ts`: pure `matchesGlob(name, pattern)` (supports `*` and `?`, case-insensitive,
  literal-safe). Unit-tested.
- `PatternSelectDialog.svelte`: a minimal input dialog (Enter/Select, Esc/backdrop cancel).
- `App.svelte`: `select-pattern` command opens it; on submit, `selection = selectIndices` of the
  matching visible rows.
- `ContextMenu.svelte`: "Select by pattern…" in the empty-area menu.

## Acceptance
- Entering `*.md` selects the markdown files in view; `?` matches one char; case-insensitive;
  empty pattern selects nothing. `npm run check` + `npm test` green.

## Work Log
2026-07-14 — Filed during Nightshift (loop 6).

2026-07-14 — Implemented on branch `CPE-360-select-by-pattern`.
- `glob.ts`: pure `matchesGlob` / `globToRegExp` (`*`, `?`, case-insensitive, literal-safe).
  5 unit tests.
- `PatternSelectDialog.svelte`: minimal input dialog (Enter/Select, Esc/backdrop cancel).
- `App.svelte`: `select-pattern` command + `selectByPattern` (selects matching visible rows,
  reports the count).
- `ContextMenu.svelte`: "Select by pattern…" in the empty-area menu.
- `npm run check` 0 errors; suite 322 pass; `npm run build` ok.
