---
id: CPE-1164
title: "Make the Command Palette discoverable — toolbar button + menu + context-menu entry (not just Ctrl+Shift+P)"
type: feature
component: Frontend
priority: high
status: Backlog
tags: ready
created: 2026-07-31
---

## Summary
User-requested (2026-07-31): the Command Palette is powerful but **only reachable via the `Ctrl+Shift+P`
shortcut** — there's no visible affordance, so a user who doesn't know the shortcut can't find it (the user hit
exactly this looking for "checkpoint"). Add discoverable entry points.

## Current state
`src/App.svelte`: `paletteOpen` is toggled only by the `Ctrl+Shift+P` keydown (~line 2569). The palette
(`CommandPalette.svelte`) already aggregates all the app's commands. There is a top menu bar (File / Tools /
Application / Language), a main toolbar/CommandBar (New / Cut / Copy / … / Sort / View), and the empty-area +
item context menus.

## Fix — add several discoverable entry points, all opening the palette
1. **Toolbar button** — a Command-Palette button on the main toolbar (a command/⌘ or "search-commands" style
   icon) with a tooltip that names it AND shows the `Ctrl+Shift+P` shortcut.
2. **Top-menu entry** — add "Command Palette   Ctrl+Shift+P" to the appropriate top menu (Tools or File),
   following the app's menu convention.
3. **Context-menu entry** — add "Command Palette" to the empty-area context menu (and optionally the item
   menu) so a right-click surfaces it too.
All simply set `paletteOpen = true` (reuse the existing open path). Keep the shortcut working.

## Acceptance Criteria
- [ ] A visible toolbar button opens the Command Palette; its tooltip shows the `Ctrl+Shift+P` shortcut.
- [ ] A top-menu item (Tools/File) opens it, labelled with the shortcut.
- [ ] The empty-area context menu has a "Command Palette" entry that opens it.
- [ ] `Ctrl+Shift+P` still works; the palette is unchanged otherwise. MENUS.md-compliant (theme vars, leading
      icon); i18n keys added to all COMPLETE_LOCALES (CPE-481 gate); `npm run check` green; a test covers at
      least the toolbar/menu dispatch → `paletteOpen`.

## Notes
- Pure frontend (reuses the existing palette). Cross-platform-agnostic. Follows the MENUS.md + menu-items-need-
  icons + tabs conventions already established this session.
