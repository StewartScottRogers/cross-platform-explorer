---
id: CPE-358
title: "File-type filter (enable the disabled Filter button)"
type: Feature
status: Done
closed: 2026-07-14
priority: Medium
component: Frontend
created: 2026-07-14
---

## Summary

The CommandBar's Filter button has been a disabled stub. Enable it as a **file-type filter**:
a dropdown (All / Folders / Images / Documents / Audio & video / Code / Archives) that narrows
the listing to that category — complementing the name search. Reuses `categoryOf`.

## Design (frontend)
- `filetypes.ts`: `FILE_FILTERS` groups + pure `matchesFileFilter(entry, key)`. Unit-tested.
- `App.svelte`: `fileFilter` state folded into the `visible` derivation (after the search/hidden
  filter, before sort). Shown in the status bar as active when not "all".
- `CommandBar.svelte`: the Filter button becomes a dropdown (like Sort/View) with a check on the
  active group; dispatches `filter`.

## Assumptions (Nightshift — user asleep)
- The filter is a session mode (not reset on navigate) so "show only images" works across
  folders; the active category is visible on the button. "All" clears it.

## Acceptance
- Picking a category narrows the list to it; "All" restores everything; works alongside search.
- `npm run check` + `npm test` green.

## Work Log
2026-07-14 — Filed during Nightshift (loop 4). The last disabled explorer stub worth enabling.

2026-07-14 — Implemented on branch `CPE-358-type-filter`.
- `filetypes.ts`: `FILE_FILTERS` groups + pure `matchesFileFilter(entry, key)` (+4 tests).
- `App.svelte`: `fileFilter` state folded into `visible` (after search/hidden, before sort).
- `CommandBar.svelte`: the once-disabled Filter button is now a dropdown (All / Folders /
  Images / Documents / Audio & video / Code / Archives) with a check on the active group and an
  accent tint (`.cmd.on`) when a filter is active.
- `npm run check` 0 errors; suite 315 pass; `npm run build` ok. Predicate + grouping tested;
  the dropdown is a GUI eyeball.
