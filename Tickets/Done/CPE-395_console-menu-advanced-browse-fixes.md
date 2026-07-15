---
id: CPE-395
title: AI Console — fix garbled Manage-agents menu, dead Advanced toggle, Browse ignores Project folder
type: bug
priority: high
estimate: S
status: Done
created: 2026-07-14
closed: 2026-07-14
tags: [ui, ai-console, bug]
---

## Problem
Three user-reported defects in the AI Console launcher toolbar:

1. **Manage agents ▾ menu is garbled/misaligned** — the global `input { width:100% }`
   rule (for text fields) stretched the menu checkboxes to ~116px, shoving each label
   into a narrow wrapping column that overlapped the next item.
2. **Advanced ▾ has no effect** — `.toolbar .row { display:flex }` is more specific than
   the `[hidden]` attribute, so `#advanced-row` never collapsed; the toggle did nothing
   and the declutter (CPE-392) never actually happened.
3. **Browse opens at a fixed default**, not the current Project folder, and the two were
   not kept in sync.

## Fix
- Scope the menu checkboxes back to their natural size (`.menu-item input[type=checkbox]`)
  + `white-space:nowrap` on menu items.
- Add `.toolbar .row[hidden] { display:none }` so the Advanced row collapses.
- Thread the Project-folder value through pick-folder → host `set_directory`, opening the
  picker there when it exists (stale/typo path → OS default, no error), and write the
  choice back to the box.

## Verification
- Diagnosed + confirmed both CSS bugs live in Chromium (checkbox = 116px; advanced computed
  `display:flex` while `[hidden]`), re-verified fixed after the change.
- Added a headless harness test for the Browse ↔ Project-folder POST/writeback.
- Full vitest + ai-console build green; hot-swapped for a final visual check.

- [x] Menu renders cleanly
- [x] Advanced collapses/expands
- [x] Browse opens at Project folder + syncs both ways
