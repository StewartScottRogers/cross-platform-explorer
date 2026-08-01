---
id: CPE-1191
title: "Bind macros to menus, hotkeys, and the command palette (with dry-run confirm)"
type: feature
component: Frontend
priority: medium
status: Backlog
tags: ready
created: 2026-07-31
epic: CPE-739
---

## Summary
Part of CPE-739 (frontend phase, after CPE-1188 + CPE-1189). Make saved macros runnable from the UI, bound to
context menu / hotkeys / command palette, with a dry-run confirm before executing.

## Build
- New `src/lib/macroBindings.ts` (+ `.test.ts`) mirroring `userCommands.ts` (`commandsForSurface` pattern):
  macro name → surfaces + optional hotkey.
- Surface saved macros in `ContextMenu` + the command palette; register user hotkeys in `App.svelte`'s keydown
  handler; add a cheat-sheet entry in `src/lib/shortcuts.ts`.
- Run goes through a **dry-run preview + confirm** (reuse the `RunCommandConfirm`-style pattern) before
  executing via the CPE-1188 run command; show the multi-step result and offer undo.

## Acceptance Criteria
- [ ] `macroBindings.test.ts` covers surface/hotkey mapping.
- [ ] gui-smoke: a saved macro appears in the context menu/palette, `snap("macro-in-menu")`; `npm run check` +
      `npm test` green.

## Work Log
- 2026-07-31 — Filed by Foreman (workshift, epic CPE-739). Depends on CPE-1188 + CPE-1189; batch with 1189
  (both edit App.svelte). Watched-folder binding deferred (needs a CPE-734 rule engine that doesn't exist yet).
