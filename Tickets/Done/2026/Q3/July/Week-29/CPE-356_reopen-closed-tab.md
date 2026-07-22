---
id: CPE-356
title: "Reopen last closed tab (Ctrl+Shift+T)"
type: Feature
status: Done
closed: 2026-07-13
priority: Medium
component: Frontend
created: 2026-07-13
---

## Summary

A tabbed explorer should let you undo a tab close. Track recently-closed tabs' folders and add
**Ctrl+Shift+T** to reopen the most recent one in a fresh active tab — the browser-standard
gesture.

## Design (frontend)
- `tabs.ts`: pure `pushClosedTab(stack, path, cap)` (append, capped). Unit-tested.
- `App.svelte`: a `closedTabPaths` stack; `closeTab` records the closing tab's current folder;
  `reopenClosedTab()` pops it into a new active tab; `Ctrl+Shift+T` (checked before `Ctrl+T`).
- `shortcuts.ts`: document the new binding (the cheat sheet is transcribed from real bindings).

## Acceptance
- Close a tab, press Ctrl+Shift+T → a tab reopens at that folder. Repeats back through the
  stack; no-op when nothing was closed. `npm run check` + `npm test` green.

## Work Log
2026-07-13 — Filed during Nightshift (loop 2). Research: tabs, cut/copy/move, favorites etc.
all present; reopen-closed-tab is a common missing gesture. Implementing.

2026-07-13 — Implemented on branch `CPE-356-reopen-closed-tab`.
- `tabs.ts`: pure `pushClosedTab(stack, path, cap=10)` (+3 unit tests).
- `App.svelte`: `closedTabPaths` stack; `closeTab` records the closing tab's folder;
  `reopenClosedTab()` pops it into a new active tab; `Ctrl+Shift+T` handled before `Ctrl+T`.
- `shortcuts.ts`: added the binding to the cheat sheet.
- `npm run check` 0 errors; suite 308 pass; `npm run build` ok. Keyboard round-trip in the
  running app is a GUI eyeball; the stack logic + ordering are tested.
