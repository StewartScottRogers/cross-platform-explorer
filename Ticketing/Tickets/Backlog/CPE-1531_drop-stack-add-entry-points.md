---
id: CPE-1531
title: "Drop Stack: \"Add to Drop Stack\" context-menu action + hotkey"
type: Feature
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-1489
parent: CPE-1489
created: 2026-08-09
---
## Context
The Drop Stack is only useful if adding to it is a one-hand action from wherever the user already is: the
file-row right-click menu, and a hotkey for the keyboard-first path. This ticket wires the two **entry
points** that call `dropStack.add()` (CPE-1530); it does not build the panel that displays the stack
(CPE-1532) or the move/copy-all action (CPE-1533).

## Scope
- A "Add to Drop Stack" item in the shared file-row context menu (the single `<ContextMenu>` instance
  driven from `src/App.svelte`, same pattern as the existing Copy/Cut/Duplicate items around
  `doCopy`/`doCut` — see `src/App.svelte:3153` onward) — follows [docs/design/MENUS.md](../../docs/design/MENUS.md):
  themed icon, `var(--text)` label, no hard-coded colour.
- Calls `dropStack.add(selectedEntries)` (respecting multi-selection, same as Copy/Cut).
- A hotkey binding for "Add to Drop Stack" registered the same way other app hotkeys are (see
  `src/lib/macroBindings.ts` — `setBinding`/`matchHotkey`) and surfaced in the Command Palette list
  (`src/App.svelte`'s palette command array, alongside `file.copy`/`file.cut`) with a shortcut label. Do
  **not** wait on CPE-1484 (hotkey customization epic) — that epic is dormant; bind a fixed default hotkey
  now the same way every other current shortcut is bound, and it becomes user-remappable later for free
  once CPE-1484 ships.
- Menu item and hotkey are disabled/absent when there's no selection (same `hasSelection` gate as
  `file.copy`).

## How
- Import `dropStack`'s `add` from CPE-1530's `src/lib/dropStack.ts`.
- Add the context-menu item + a `doAddToDropStack()` handler in `src/App.svelte`, next to `doCopy`/`doCut`
  (small, additive — do not restructure the existing copy/cut/paste code).
- Register the hotkey via the existing `macroBindings.ts` helpers, default chord TBD by convention (pick
  an unused combo — check `bindingsForSurface`/existing bindings for collisions before choosing one).

## Verify
`npm run check` + `npx vitest run` extending `src/App.features.test.ts` (or a new
`src/App.dropStackEntry.test.ts` alongside the existing per-feature App test files, e.g.
`App.paneBContextMenu.test.ts`) with cases: menu item calls `dropStack.add` with the current selection;
hotkey triggers the same handler; both are disabled with no selection. Fully headless — jsdom + the
existing App.svelte test harness pattern; no GUI verification required to land it.

## Notes
**Conflict surface:** `src/App.svelte` (small, additive: one context-menu item + one handler function +
one palette entry + one hotkey registration, near the existing `doCopy`/`doCut` code) and
`src/lib/macroBindings.ts` (registering the new binding — additive, no existing binding logic changed).
App.svelte is a large, frequently-touched file — if another in-flight ticket is also editing it, the
Foreman should serialize, not parallelize, these two. **Dispatch order: after CPE-1530.** Independent of
CPE-1532 (different files — the panel doesn't exist yet, only the store contents change) — can run in
parallel with CPE-1532 once CPE-1530 has landed.
