---
id: CPE-1191
title: "Bind macros to menus, hotkeys, and the command palette (with dry-run confirm)"
type: feature
component: Frontend
priority: medium
status: Done
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
- 2026-07-31 — Filed by Foreman (sprint, epic CPE-739). Depends on CPE-1188 + CPE-1189; batch with 1189
  (both edit App.svelte). Watched-folder binding deferred (needs a CPE-734 rule engine that doesn't exist yet).
- 2026-07-31 — **Done.** Built `src/lib/macroBindings.ts` (+ `.test.ts`, 12 tests) mirroring
  `userCommands.ts`'s `commandsForSurface`/reorder pattern, but keyed by macro NAME (a macro's identity
  IS its name — `macro_save`/`macro_load`/`macro_delete` all key on it) rather than a generated id:
  `{name, surfaces: ("context"|"palette")[], hotkey}` + `normalizeHotkey`/`hotkeyFromEvent` (canonical
  `"Ctrl+Alt+K"` form; rejects a Shift-only or bare-letter combo so a macro hotkey can never collide with
  type-ahead-find or plain typing). Persisted via new `settings.saveMacroBindings`/`loadMacroBindings`
  (tolerant parse, same shape as `saveUserCommands`). The per-macro binding EDITOR (Menu/Palette
  checkboxes + hotkey field) lives in `MacrosDialog.svelte` (CPE-1189) — there was nowhere else for it to
  go, and without it a saved macro could never actually reach a surface.
  **Surfacing:** `ContextMenu.svelte` gained a `macros: string[]` prop — a "Run macro ▸" submenu (only
  when non-empty) next to Tags, dispatching `macro:<name>`; the command palette gets one row per
  palette-bound macro (`Run macro: <name>`). Both are filtered live against the CPE-1188 catalog (a
  binding for a since-deleted macro never shows a dead row). A user hotkey is checked LAST in App.svelte's
  `handleKeydown` — after every built-in `case`/`if` — so a macro can never shadow an existing binding.
  **Run flow:** new `MacroRunConfirm.svelte` (+ `.test.ts`, 5 tests) — dry-run via `commands.macroPlan`,
  render the flat `PlannedOp` preview, explicit Run click before `commands.macroRun`, then show the
  applied step count with an Undo button wired to `commands.macroUndo`. `App.svelte`'s `startMacro` is the
  one entry point (context menu / palette / hotkey all call it): loads the macro, and if
  `macroParams.extractAskLabels` finds any `{ask:label}` token, routes through `MacroParamPrompt` (CPE-1190)
  first — the resolved macro (params substituted) is what actually reaches the dry-run/run.
  gui-smoke `snap("macro-in-menu")` added (`gui-smoke/specs/macro-in-menu.smoke.ts`) — creates + binds a
  macro, real-right-clicks a file row, and asserts it appears in the "Run macro ▸" submenu; typechecks
  clean (`cd gui-smoke && npm run typecheck`), live run is CI. `npm run check`: 0 errors. `npm test`: 139
  files / 1556 tests green. No dependencies added.
