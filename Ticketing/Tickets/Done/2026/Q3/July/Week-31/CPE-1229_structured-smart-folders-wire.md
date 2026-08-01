---
id: CPE-1229
title: "Wire structured smart folders end-to-end: Save search + sidebar surface + open-evaluator"
type: Task
priority: Medium
component: frontend
tags: [ready]
estimate: 3h
created: 2026-08-01
epic: CPE-978
closed:
prereq: CPE-1228
---

## Context
With the saved-search store (CPE-1228) in place, wire the structured `SavedSearch` (multi-`Condition`)
into the UI, mirroring the existing tag-only smart-folder wiring in `App.svelte`/`Sidebar.svelte`/
`SmartFolderMenu.svelte` but driven by `evaluateSavedSearch` instead of the tag path.

## Acceptance criteria
- **Save search…**: a discoverable affordance (from the search bar / results area, and/or the command
  palette) that captures the CURRENT structured search (its `Condition`s + match mode) into a named
  `SavedSearch` via the CPE-1228 store. This is the primary DoD flow ("save the search you just ran").
- **Sidebar surface**: saved structured searches appear as places in the sidebar (alongside or unified
  with the tag-only smart folders), each clickable, with a right-click rename/delete menu.
- **Open-evaluator**: opening one runs `evaluateSavedSearch(entries, search, now)` across the tree
  (reuse the index / `entriesForPaths` / the existing smart-folder open path) and shows current matches,
  cutting across the physical tree — a `smartOverride`-style result view like the tag path.
- Reuse the existing `Condition` matcher + evaluator — NO parallel matching logic.
- `npm run check` clean; REAL tests for the save-capture + open-evaluate wiring. Menu text theme-var
  only (MENUS.md); any pills reflow; dialogs (if any) get the visible border (`--dialog-border`).

## Notes
Prereq CPE-1228. GUI verify (save->open flow) is user-gated for a live smoke; cover headlessly with
component tests + a gui-smoke render pin where feasible (QA-Architect follow-up ok).
