---
id: CPE-1158
title: "Right-clicking a disk/drive gives no context menu — it should offer a folder-like menu"
type: bug
component: Frontend
priority: high
status: Done
tags: ready
created: 2026-07-30
closed: 2026-07-31
---

## Summary
User-found (2026-07-30). Right-clicking a **disk/drive** produces **no context menu at all**. It should offer
something like a folder's right-click menu (Open, New ▸ creating at the drive root, Properties, etc.). A drive
is conceptually a folder (its root), so it should behave like one.

## Current state
Disks appear as **tiles on the Home landing** (`HomeView.svelte`) and in the **sidebar Drives section**
(`Sidebar.svelte`). Per the CPE-1156 finding, `HomeView` dispatches only `navigate`/`select` (no
`rowContext`), and `ExplorerPane` suppresses context events while `inHome`, so a Home drive tile has no menu.
The sidebar drive rows should be checked too.

## Fix
- Give disk/drive items a right-click context menu. Reuse the existing `ContextMenu` where practical, or a
  focused drive menu, offering at minimum: **Open**, **New ▸** (Folder / Text file, created at the drive's
  **root path** — reuse CPE-1156's create-in-target-folder path), and **Properties** for the drive root. Copy
  as path / Open in Terminal / Pin are nice-to-haves if cheap.
- Wire it wherever drives are shown:
  - **Home drive tiles** (`HomeView.svelte`): add an `on:contextmenu` → dispatch a context event up through
    `ExplorerPane` (the `inHome` context suppression must be relaxed specifically for drive tiles so their
    menu works, WITHOUT re-enabling the general empty-area menu on the abstract Home background — keep those
    distinct). 
  - **Sidebar Drives** (`Sidebar.svelte`): if a drive row there lacks a menu, add the same.
- The menu's actions target the drive's **root path** (e.g. `Z:\`), so New creates at the root — consistent
  with CPE-1156.

## Acceptance Criteria
- [x] Right-clicking a disk/drive (Home tile AND sidebar row) opens a context menu.
- [x] The menu offers Open + New ▸ (creating at the drive root) + Properties (of the drive root), following
      MENUS.md (theme vars, leading icons); New at the root actually creates there.
- [x] The abstract Home background (not a drive) behaviour is unchanged — no accidental empty-area menu on
      blank Home space; only the drive tiles get the menu.
- [x] `npm run check` green; a test covers the drive-tile menu dispatch + the New-at-drive-root wiring.

## Notes
- Frontend-only expected (create/list commands already accept an arbitrary path). Cross-platform-agnostic
  (drives on Windows; mount points elsewhere — keep labels sensible).
- Follow-up promised in CPE-1156 (drive-tile-on-Home menu) + the user's explicit request here.

## Work Log
- 2026-07-31 — Implemented (frontend-only). Branch `cpe-1158-disk-context-menu`.
- **How the drive menu was scoped.** Added a third `target: "drive"` branch to `ContextMenu.svelte` (a
  FOCUSED drive menu, not the full on-item menu). It offers only what makes sense for a whole volume:
  **Open** (`drive-open` → navigate into root), **New ▸ Folder / Text file** (`drive-new-folder` /
  `drive-new-file` → create AT the root), **Copy as path** (`drive-copy-path`), **Open in Terminal**
  (`drive-terminal`), and **Properties** (`drive-properties` → root). The volume-nonsensical actions
  (Rename / Delete / Cut / Copy / Duplicate / Pin / Favorite / Compress / …) are deliberately NOT rendered
  — the drive branch simply doesn't emit them, rather than the on-item menu hiding them behind a flag, so
  there's zero risk of a stray destructive item on a drive. All items follow MENUS.md (leading `Icon`s,
  `var(--text)` colour, no red).
- **New at the drive ROOT.** `App.svelte` keeps the clicked drive's root path + display name in
  `driveCtxPath` / `driveCtxName` (set by `onDriveContext`), independent of any FileList selection — this
  is essential because on Home there is no selection to piggy-back on. The `drive-new-*` actions call the
  existing `newFolder(driveCtxPath)` / `newFile(driveCtxPath)` (CPE-1156's create-in-target path); its
  `createNewItem` already treats a real drive root as an ordinary path (only the ` home` sentinel is
  blocked), navigates into the root, and inline-renames the new item. `drive-properties` synthesizes a
  root folder entry exactly like `openFolderProperties`; `drive-terminal` calls `commands.openTerminal`
  directly (no `isHome` guard, since the menu is reachable from Home and a root is terminal-worthy).
- **Home tile vs Home background kept distinct (guarantee).** Only DRIVE tiles get a menu: `HomeView`
  tags each Quick-access card with `isDrive`, and its `on:contextmenu` dispatches a NEW `driveContext`
  event (with the root path + name) ONLY for drives — place/pin tiles `return` early and fall through to
  the window-level native-menu suppressor (no menu, as before). `ExplorerPane` forwards `driveContext`
  straight up even while `inHome`; this is a SEPARATE channel from `contextEmpty`/`paneContext`, which
  stay suppressed on Home (`paneContext` still `return`s early when `inHome`). So a right-click on blank
  Home space still opens NO menu — the empty-area menu was not re-enabled anywhere.
- **Sidebar coverage.** `Sidebar.svelte` drive rows previously had no menu; added the same
  `on:contextmenu` → `driveContext` (gated on the existing `isDrive` computed), so both surfaces open the
  identical focused drive menu via `App.onDriveContext`.
- **Verification.** `npm run check` → 0 errors / 0 warnings. New test `DriveContextMenu.test.ts` (6 tests)
  covers: Home drive tile dispatches `driveContext` with the root path/name; a place tile does NOT; and
  the ContextMenu drive branch renders + dispatches `drive-open` / `drive-new-folder` / `drive-new-file` /
  `drive-copy-path` / `drive-terminal` / `drive-properties`, and omits Rename/Delete/Duplicate. Full
  suite: 128 files / 1437 tests passing (no regressions to CPE-1153/1154/1156). Diff is frontend-only (no
  `.rs` / `Cargo` / bindings).
