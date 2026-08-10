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
