---
id: CPE-1158
title: "Right-clicking a disk/drive gives no context menu — it should offer a folder-like menu"
type: bug
component: Frontend
priority: high
status: Backlog
tags: ready
created: 2026-07-30
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
- [ ] Right-clicking a disk/drive (Home tile AND sidebar row) opens a context menu.
- [ ] The menu offers Open + New ▸ (creating at the drive root) + Properties (of the drive root), following
      MENUS.md (theme vars, leading icons); New at the root actually creates there.
- [ ] The abstract Home background (not a drive) behaviour is unchanged — no accidental empty-area menu on
      blank Home space; only the drive tiles get the menu.
- [ ] `npm run check` green; a test covers the drive-tile menu dispatch + the New-at-drive-root wiring.

## Notes
- Frontend-only expected (create/list commands already accept an arbitrary path). Cross-platform-agnostic
  (drives on Windows; mount points elsewhere — keep labels sensible).
- Follow-up promised in CPE-1156 (drive-tile-on-Home menu) + the user's explicit request here.
