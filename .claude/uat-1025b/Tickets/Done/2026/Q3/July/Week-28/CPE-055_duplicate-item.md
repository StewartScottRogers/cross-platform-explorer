---
id: CPE-055
title: Duplicate selected item(s) with Ctrl+D and a context-menu entry
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 1h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

There is no way to duplicate a file/folder in place. Copy + paste into the same folder works but is two
steps. Add a Duplicate action (Ctrl/Cmd+D and a context-menu item) that copies the current selection
into the folder it already lives in. The backend `copy_entries` command already auto-renames on
collision (`unique_target`), so duplicating into the same directory yields a non-colliding copy with no
new backend code.

## Acceptance Criteria

- [ ] `Ctrl+D` (and `Cmd+D`) duplicates the selected item(s)
- [ ] A "Duplicate" entry appears in the item context menu with the Ctrl+D hint
- [ ] Duplicate invokes `copy_entries` with the selection's paths and `dest = currentPath`
- [ ] No-ops on the Home view and when nothing is selected
- [ ] Result (and any partial failure) is surfaced via the existing status notice
- [ ] Integration test drives Ctrl+D and asserts the backend call; `npm run check` clean; suite green

## Resolution

Added `doDuplicate()` (copies the selection into `currentPath` via `copy_entries`, which auto-renames),
a `Ctrl/Cmd+D` binding, a `"duplicate"` action case, and a "Duplicate" context-menu entry with the
Ctrl+D hint. No-ops on Home / empty selection; not pushed to undo (matches copy-paste policy). Added an
App integration test driving Ctrl+D and asserting `copy_entries` is called with the selection paths and
`dest = currentPath`. `npm run check` 0 errors; suite 133 passed; `vite build` clean. Committed on
branch, merged to `main`, pushed. Residual (visual: menu entry appearance) folds into CPE-053's
smoke-check.

## Work Log

2026-07-11 — Nightshift loop: research picked Duplicate as a clean, headlessly-testable feature that reuses the existing auto-renaming copy backend. Ctrl+D is free (only Alt+D is bound). Verified copy_entries + unique_target semantics.

## Notes

Like paste of a copy, a duplicate is intentionally NOT pushed onto the undo stack (undoing a copy means
deleting the new file — destructive to reverse a harmless act), matching the existing doPaste policy.
