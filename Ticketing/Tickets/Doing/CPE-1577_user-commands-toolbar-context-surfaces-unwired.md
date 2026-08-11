---
id: CPE-1577
title: "Bug: User-defined commands Toolbar/Context surfaces save but don't render (only Palette wired)"
type: Bug
status: Backlog
priority: Medium
component: Frontend
epic: CPE-711
tags: [ready]
created: 2026-08-10
closed:
---

## Why / found by
Surfaced while documenting User Commands for CPE-1575 (docs audit CPE-1569). Verified in
`UserCommandsDialog.svelte` + the command-surface wiring: a user command's **Toolbar** and **Context**
surface checkboxes persist but nothing consumes them — only the **Palette** surface is actually wired to
render/run the command. Worse, a freshly-created command **defaults to Context-only**, so it is invisible
everywhere (toolbar AND context menu AND palette) unless the user also ticks Palette. Feature epic CPE-711
(user-defined commands) shipped the dialog + palette path but left the other two surfaces inert.

## Repro
1. Define a user command; leave the default surface (Context) selected.
2. Right-click a file → the command does not appear in the context menu; it's not in the toolbar either.
3. Only if "Palette" is ticked does it show up (in the command palette).

## Fix (scope when picked up)
- Wire the **Context** surface: user commands flagged for context render in `ContextMenu.svelte` (per MENUS.md —
  theme colors, Icon glyph) and run with the same `{path}/{name}/{dir}/{ext}/{stem}` templating + confirm-before-shell.
- Wire the **Toolbar** surface similarly (or, if toolbar hosting is out of scope, hide the checkbox until supported —
  don't offer a control that does nothing).
- Reconsider the **default surface** for a new command so it isn't invisible out of the box (e.g. default Palette on,
  or require ≥1 surface).
- Tests for each surface actually rendering + running the command.

## Notes
Confirm current behavior in `UserCommandsDialog.svelte`, `RunCommandConfirm.svelte`, and the palette/context wiring
before implementing. Update the CPE-1575 `organizing-user-commands.md` docs page once fixed (it currently documents the
gotcha honestly). Model: sonnet.

## Work Log — 2026-08-10

Branch `cpe-1577-1584-command-surfaces`, batched with CPE-1584 (same hot frontend files). PR #797
(https://github.com/StewartScottRogers/cross-platform-explorer/pull/797).

- **Context surface**: wired. `ContextMenu.svelte` gained a `userCommands` prop rendering a "Run
  command ▸" submenu (same shape as the existing "Run macro ▸" submenu) for `target === "item"`;
  dispatches `uc:<id>`, which `App.svelte`'s `runAction` routes to the existing
  `openRunCommand` → `RunCommandConfirm` flow.
- **Toolbar-surface decision: WIRED, not hidden.** `CommandBar.svelte` already dispatches every
  action through the same `on:action` event `runAction` consumes (see its Command Palette launcher
  button), so adding one button per Toolbar-bound command was the same shape of change as Context,
  not genuinely out of scope. Each bound command gets its own always-visible toolbar button
  (`title="{name} (user command)"`), same `uc:<id>` dispatch. No submenu grouping on this surface —
  documented as a limit in `organizing-user-commands.md` (bind sparingly if you have many commands).
- **Default surface fixed**: a new command now defaults to `["context", "palette"]` (was
  `["context"]` alone) — `DEFAULT_COMMAND_SURFACES` in `userCommands.ts`, shared by the add form's
  initial state and its "every surface unchecked" save-time fallback — so a freshly-created command
  is reachable immediately by right-click and by search, never invisible out of the box.
- Docs (`organizing-user-commands.md`) rewritten to describe all three working surfaces.
- Tests: `ContextMenu.test.ts` / `CommandBar.test.ts` render+dispatch specs per surface, plus a new
  `App.userCommandSurfaces.test.ts` integration spec proving Context/Toolbar reach the same confirm
  dialog as Palette with the selection correctly resolved (including a real `run_command` invocation).
- Verified locally: `npm run check` (0 errors/warnings) and `npx vitest run` (268 files / 3169 tests,
  all green). Did not watch CI on the PR — that's the Foreman's pass.
