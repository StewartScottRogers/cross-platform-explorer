---
id: CPE-748
title: Menu items should have leading icons, aligned in a column
type: feature
component: Frontend
priority: medium
status: Open
tags: big-design
created: 2026-07-19
estimate: 3-4h
---

## Summary
Popup menus (right-click context menus + dropdowns) are text-only today, while toolbar buttons, sidebar
tree nodes, and leaves all carry icons. Give **menu items leading icons**, reusing the shared `Icon` glyphs,
**aligned in a fixed-width icon column** so labels line up (a blank slot where an item genuinely has no
icon). Where a menu action mirrors something with an icon elsewhere (a toolbar button, a file/place glyph),
reuse that same icon for consistency.

## Why
Consistent iconography across the whole UI — buttons, nodes, leaves, **and menus** — makes actions faster
to scan and the app feel finished. (User request, 2026-07-19.)

## Rough scope (areas, not child tickets)
- Extend the shared menu system ([docs/design/MENUS.md](../../docs/design/MENUS.md) / `menu-render`) so a
  menu item can carry an optional `icon` in a **fixed leading icon column**; items without one still align.
- Add icons to the existing context menus and dropdowns (file row context menu, sidebar context menus,
  tab menu, the Application menu, any others), reusing existing `Icon` names; add glyphs only where none fit.
- Keep the menu design standard: icon + text both `var(--text)` / `currentColor` — theme-variable colours,
  never a hard-coded or "destructive-red" colour; identical light/dark, cross-platform.
- Handle separators, submenus, checkmarks/toggles, and keyboard-shortcut hints coexisting with the icon
  column without misalignment.

## Open questions (resolve at pickup — best-guess if unattended)
- Icon column width + gap (match the sidebar node indent/`Icon size` for visual continuity).
- Do checkable items put the check in the icon column or a separate trailing marker? (lean: trailing, so the
  leading column stays purely category/action icons.)
- Any menu items that should intentionally stay icon-less (and thus render a blank slot).

## Definition of Done
- The shared menu component supports an aligned leading icon column; items line up with or without an icon.
- Every existing popup menu shows icons, consistent with the toolbar/sidebar glyphs and the menu standard.
- No colour regressions (theme-variable only, no red text); light/dark parity; a GUI pass confirms alignment.

## Notes
Related: CPE-747 (gave the Documents toolbar button a book icon — same iconography-consistency push).
Follows the menu design standard and the shared `menu-render` system. Marked `big-design` because the
alignment/column model + submenu/checkmark coexistence is the design work.
