---
id: CPE-1153
title: "Right-click context menu: bring it to Windows 11 Explorer parity (New ▸ / View ▸ / Sort by ▸ / Undo / Properties)"
type: feature
component: Frontend
priority: medium
status: Backlog
tags: ready
created: 2026-07-30
---

## Summary
User-requested (2026-07-30). Surfaced while testing checkpoints: the user reached for a Windows 11-style
right-click menu to create a folder and found ours thinner than modern Explorer. Bring the file-area context
menu up to **Windows 11 Explorer parity**, adding the entries a Windows user expects — so common actions
(especially creating things and changing view/sort) are discoverable where they reach for them.

## Current state (`src/lib/components/ContextMenu.svelte`)
The empty-area (background) branch already has: **New folder** (Ctrl+Shift+N), **New file**, **Paste**,
Select all / Invert / By-pattern, **Refresh** (F5), Open in Terminal + Work-on-folder (when available),
Reveal in OS, Docs. The on-item branch has Open/Cut/Copy/Rename/Delete/Copy-path/Extract/Compress/Pin/
Favorite/Tags/Reveal/Properties/etc. The menu is currently **flat** (no submenus).

## Gaps vs. Windows 11 Explorer (empty-area / background menu)
- **View ▸** submenu — icon sizes / list / details / tiles / content (we have view modes via the toolbar,
  but not in the context menu).
- **Sort by ▸** submenu — Name / Date modified / Type / Size, plus Ascending/Descending (and, if we support
  it, Group by ▸). We have sortable columns but no context-menu sort.
- **New ▸** submenu — Folder, (Shortcut), then a small set of common file types (Text Document, etc.),
  replacing/[supplementing] the flat "New folder" / "New file" with the familiar submenu.
- **Undo** (Ctrl+Z) for the last file op (rename/move/delete) — if/when an undo stack exists; otherwise note
  it as out of scope and file separately.
- **Properties** on the background (folder-level properties for the current directory) — currently only on
  the item menu.

## Acceptance Criteria
- [ ] The empty-area context menu gains **View ▸** and **Sort by ▸** submenus that drive the same view/sort
      state the toolbar does (single source of truth — no divergent state), with the current selection
      checkmarked.
- [ ] A **New ▸** submenu offering at minimum **Folder** + **Text file** (extend to a couple more common
      types if cheap), wired to the existing `new-folder` / `new-file` actions.
- [ ] Background **Properties** (properties for the current folder) is available.
- [ ] **Undo last action** is included IF an undo stack exists; if not, this AC is dropped with a note (and a
      separate undo-stack ticket filed) rather than faked.
- [ ] Submenus follow the menu standard **[docs/design/MENUS.md](../../../docs/design/MENUS.md)**: item text is
      always `var(--text)` (never hard-coded / never red), theme-var colours only, identical light/dark,
      and every item has a **leading icon aligned in a column** (per the menu-items-need-icons convention).
      Submenus open on hover/right-arrow, close on Escape/left-arrow, keyboard-navigable, and reflow/clamp to
      stay on-screen near a window edge.
- [ ] The existing flat entries + the on-item menu keep working; `ContextMenu.test.ts` is extended to cover
      the new submenus (open/select/keyboard) and stays green; `npm run check` clean.

## Notes / decisions to make
- **Submenu is a new pattern** for `ContextMenu.svelte` (today it's flat) — the nested/flyout submenu is the
  main design piece; do it once, reuse for View/Sort/New. Consider extracting a small `<Submenu>` helper.
- **Home-screen caveat (worth a look):** the reported friction was partly that "New folder" is disabled on
  the **Home/landing** view (you can only create inside a real folder). A Windows-parity menu won't create a
  folder on an abstract Home either — but consider whether right-clicking Home should at least offer useful
  actions (e.g. New tab, pin/add a location) instead of feeling dead. Decide + note; may spin a follow-up.
- Keep it **cross-platform** (this is a general explorer, not Windows-only) — mirror Windows 11's *shape*,
  but the labels/behaviour must read sensibly on macOS/Linux too; don't hard-code Windows-only concepts.
- Respect the [[prefer-inline-instant-controls]] and menu conventions; no modal popups for these.
