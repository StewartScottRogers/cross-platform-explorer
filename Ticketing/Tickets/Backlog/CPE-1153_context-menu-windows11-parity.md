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
- [x] The empty-area context menu gains **View ▸** and **Sort by ▸** submenus that drive the same view/sort
      state the toolbar does (single source of truth — no divergent state), with the current selection
      checkmarked.
- [x] A **New ▸** submenu offering at minimum **Folder** + **Text file** (extend to a couple more common
      types if cheap), wired to the existing `new-folder` / `new-file` actions.
- [x] Background **Properties** (properties for the current folder) is available.
- [x] **Undo last action** is included IF an undo stack exists; if not, this AC is dropped with a note (and a
      separate undo-stack ticket filed) rather than faked.
- [x] Submenus follow the menu standard **[docs/design/MENUS.md](../../../docs/design/MENUS.md)**: item text is
      always `var(--text)` (never hard-coded / never red), theme-var colours only, identical light/dark,
      and every item has a **leading icon aligned in a column** (per the menu-items-need-icons convention).
      Submenus open on hover/right-arrow, close on Escape/left-arrow, keyboard-navigable, and reflow/clamp to
      stay on-screen near a window edge.
- [x] The existing flat entries + the on-item menu keep working; `ContextMenu.test.ts` is extended to cover
      the new submenus (open/select/keyboard) and stays green; `npm run check` clean.

## Work Log

**2026-07-30 — implemented (frontend-only), branch `cpe-1153-context-menu-parity`.**

Brought the empty-area (background) right-click menu to Windows 11 Explorer parity. On-item branch left
untouched, as scoped.

- **Submenu pattern (the core new piece):** new reusable `src/lib/components/Submenu.svelte` — a parent
  `.row` with a trailing chevron plus a flyout panel styled to the MENUS.md `.ctx` container spec. Opens on
  hover AND on Right-arrow / Enter / Space / Down-arrow; closes on Escape / Left-arrow (returning focus to
  the parent, `stopPropagation` so it closes just the submenu, not the whole menu); arrow keys move focus
  between items; clamps on-screen by flipping to `right:100%` when the rightward flyout would overflow the
  viewport (the menu is `position:fixed`). Slotted items keep ContextMenu's scoped `.row` styling; the
  parent row + flyout container are styled locally (Svelte scoping) but to the identical MENUS.md tokens.
  Used three times: New / View / Sort.
- **New ▸** — Folder (→ `new-folder`, keeps the Ctrl+Shift+N hint) and Text file (→ `new-file`). Dropped
  extra file types (Shortcut, etc.): creating them cheaply needs new backend actions, out of scope here —
  noted as a possible follow-up. Item labels use new `ctx.folder` / `ctx.textFile` keys so they read
  "Folder"/"Text file" under the "New" parent rather than the redundant "New folder".
- **View ▸** — Details / List / Large icons / Gallery, checkmarked via `aria-checked` + a trailing accent
  ✓. Selecting dispatches `view:<mode>`; `runAction` sets the SAME `view` state the toolbar/CommandBar drive
  and calls `settings.saveView` exactly as the existing paths do — no divergent state.
- **Sort by ▸** — Name / Date modified / Type / Size + Ascending / Descending, both key and direction
  checkmarked. Dispatches `sort:<key>` / `sortdir:<dir>` → sets the SAME `sortKey`/`sortDir` the column
  headers use, persisting via `settings.saveSortKey`/`saveSortDir`.
- **Undo** — an undo stack already exists (`src/lib/undo.ts`; `undo()` in App.svelte, wired to Ctrl+Z). Added
  a row near Paste (Ctrl+Z hint), gated on `canUndo(undoStack)`, labelled with `peekLabel` when present
  (e.g. "Undo Rename to a.txt"). AC satisfied — no separate ticket needed.
- **Properties (this folder)** — new `openFolderProperties()` synthesizes a folder `DirEntry` for
  `currentPath` and feeds the existing `PropertiesDialog` (which re-fetches real info from the backend by
  path). Gated to real folders (`canTerminal`); skipped on the abstract Home view.
- **i18n:** reused the already-fully-translated `cmd.*` / `view.*` / `sort.*` namespaces for the submenu
  labels (zero translation burden). Added only `ctx.folder` / `ctx.textFile` / `ctx.undo` to all 12
  `COMPLETE_LOCALES` so the CPE-481 coverage gate stays green.
- **Home-screen caveat (from the ticket notes):** left as-is for now — the background menu on Home still
  shows New ▸ etc. but create/properties are naturally gated off an abstract Home. A richer Home menu
  (New tab / add-location) is a reasonable follow-up, not done here.

**Verification:** `npm run check` → 0 errors / 0 warnings. `npx vitest run
src/lib/components/ContextMenu.test.ts` → 13 passed (6 pre-existing + 7 new). Full `npx vitest run` → 126
files / 1424 tests passed. No Rust/`bindings.gen.ts` changes (frontend-only), so no specta regen. Left in
Backlog per instructions.

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
