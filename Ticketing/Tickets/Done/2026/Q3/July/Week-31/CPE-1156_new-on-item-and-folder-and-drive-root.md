---
id: CPE-1156
title: "New ▸ must be reachable on item/folder right-click (create inside the folder) and at drive roots"
type: bug
component: Frontend
priority: high
status: Done
tags: ready
created: 2026-07-30
closed: 2026-07-30
---

## Summary
User-found (2026-07-30). Follow-on from CPE-1153/1154. "New" is only reachable from the empty-area menu, which
the user can't always hit:
- **Empty folder** → right-click → New ▸ shows. ✓
- **Folder with items** → the user naturally right-clicks an item, gets the **on-item** menu, which has **no
  New**, so New feels missing. ✗
- **Right-clicking a folder** should offer **New ▸**, creating the new item **inside that folder**. ✗ (missing)
- **Drive root** should offer the same New so items can be created at the **root of the drive**. Verify/fix. ✗

User's words: "If I right-click on a folder I should see the new button so that whatever is new will be
created in the folder. The drive should have the same new so that I can create items at the root of the drive."

## Current state
- `ContextMenu.svelte`: the **on-item** branch (`target === "item"`) has Open/Cut/Copy/Rename/Delete/… but
  **no New**. The empty-area branch has New ▸ (CPE-1153). Props available to the item branch: `folderSelected`
  (single dir selected), and App knows the clicked entry as `selectedEntries[0]`.
- `App.svelte`: `newFolder()` / `newFile()` are **hard-wired to `currentPath`** (`commands.createDir(currentPath,name)`
  / `createFile`), and both early-return `if (isHome || blockedInArchive())`. The `create_dir`/`create_file`
  backend commands already take `(path, name)`, so creating in a *different* folder needs **no backend change**
  — just pass a different path.

## Fix (frontend-only expected)
1. **Add New ▸ to the on-item menu** — reuse the CPE-1153 `Submenu` (Folder / Text file), same as the empty-area
   menu, so right-clicking any item still exposes New.
2. **Target the right folder.** Parameterize the create actions: `newFolder(targetDir = currentPath)` /
   `newFile(targetDir = currentPath)`.
   - Right-clicked item is a **single folder** → create **inside that folder** (`selectedEntries[0].path`).
     Since the new item lives in a folder that may not be the open one, pick a sensible UX and document it —
     recommend: **navigate into that folder, then create + inline-rename** (so the user sees + names it), OR
     create-in-place + a notice "Created X in <folder>". Worker decides; log the choice.
   - Right-clicked item is a **file** (or multi/none) → create in the **current folder** (`currentPath`), same
     as the empty-area menu.
3. **Drive roots.** Verify New works when `currentPath` is a drive root (e.g. `Z:\`) — a drive root is not
   `isHome`, so it should already; if anything gates it out, fix. If drives are shown as **items** (Home /
   sidebar / a drive listing), right-clicking a drive → New ▸ should create at that **drive's root path**
   (reuse the "create inside the clicked folder" path with the drive root as the target). Keep the `isHome`
   guard only for the abstract Home landing itself, not for real drive-root paths.
4. Keep the empty-area New, the CPE-1153 submenus, CPE-1154 native-menu suppression, and the two-step nothing
   else regressed.

## Acceptance Criteria
- [x] Right-clicking a **file item** shows **New ▸**; choosing Folder/Text file creates it in the **current
      folder** and inline-renames it.
- [x] Right-clicking a **folder item** shows **New ▸**; choosing Folder/Text file creates the new item
      **inside that folder** (verified it lands in the subfolder, not the current one), with a sensible,
      documented UX (navigate-in — see Work Log).
- [x] In a folder that **has one or more items**, New is reachable (via the item menu and/or the blank area) —
      the reported "can't find New when there's an item" case is resolved.
- [x] At a **drive root**, New creates an item at the drive root (empty-area menu works; on-item inside a
      drive root works). **Partial:** a drive shown as a *tile on the Home landing* has no context menu at all
      today (`HomeView` never dispatches `rowContext`; `ExplorerPane` suppresses menus when `inHome`), so
      right-clicking a Home drive tile → New is a documented follow-up — see Work Log.
- [x] Empty-folder New, CPE-1153 submenus, and CPE-1154 native-menu suppression all still work; `npm run check`
      green; tests updated/added (component tests: New-from-item dispatches the folder-target action for a
      folder item vs. current-folder for a file item; existing suite green).

## Notes
- No backend change expected (`create_dir`/`create_file` already accept an arbitrary parent path).
- Mirrors Windows-Explorer intuition the user is reaching for; keep cross-platform-agnostic. Menu items follow
  MENUS.md (theme vars, leading icons).

## Work Log
- 2026-07-30 — Implemented frontend-only (3 files: `ContextMenu.svelte`, `App.svelte`, `ContextMenu.test.ts`);
  no backend/bindings touched (`create_dir`/`create_file` already take `(path, name)`).
- **On-item New ▸** — added the CPE-1153 `Submenu` (Folder / Text file, leading icons + chevron, theme vars) to
  the `target === "item"` branch, mirroring the empty-area menu, placed as its own separated group after the
  Open block. The submenu dispatches conditionally on `folderSelected`:
  - single **folder** selected → `new-folder-in` / `new-file-in`;
  - **file** / multi / none → `new-folder` / `new-file` (same as empty-area).
  The `Ctrl+Shift+N` hint is shown only on the current-folder (`!folderSelected`) variant, since the shortcut
  always creates in the current folder.
- **Create parameterized** — `newFolder`/`newFile` now take an optional `targetDir` (default `currentPath`) and
  delegate to a shared `createNewItem(kind, targetDir)`. `runAction` maps `new-folder-in`/`new-file-in` to
  `newFolder(selectedEntries[0].path)` / `newFile(...)` when `selectedEntries[0].is_dir`. The palette,
  `Ctrl+Shift+N`, and the empty-area menu all still call `newFolder()`/`newFile()` with no arg → `currentPath`,
  so their behaviour is unchanged.
- **UX choice for "create inside a folder": navigate-in + inline-rename** (the ticket's recommended option).
  We create the item first, then `setHistory(...)` + a **fresh** `loadPath(targetDir, false, false)` (NOT the
  cache — a cached listing wouldn't contain the just-created item, so the pending inline-rename wouldn't fire).
  The user lands inside the target folder with the new item selected and in rename mode. Dedup (`(2)`
  auto-number) is computed against the **target** folder's real contents: the in-view `entries` when creating
  in place, or a fresh `commands.listDir(targetDir)` when creating inside an un-opened folder. This avoids any
  reliance on reactive `currentPath`/`entries` timing after the create.
- **Guard fix for drive roots** — changed the early-return from `if (isHome ...)` to `if (targetDir === HOME ...)`.
  `isHome` is only ever true for the abstract Home landing; a drive root (e.g. `Z:\`) is an ordinary path, so
  New at a drive root (empty-area, or right-clicking a subfolder there) is not blocked. The abstract Home
  landing still has no New (no context menu opens there at all).
- **Drive-as-Home-tile finding (follow-up):** drives/pins on the Home landing are rendered by `HomeView`, which
  dispatches only `navigate`/`select` (no `rowContext`), and `ExplorerPane` returns early on context events when
  `inHome`. So Home tiles have no right-click menu today — wiring New onto a Home drive tile needs a menu added
  to `HomeView` first, which is a separate change. Noted as a follow-up; the navigated-in drive-root case is
  fully covered.
- **Verification:** `npm run check` → 0 errors / 0 warnings. `npx vitest run` → 127 files, **1431 passed**
  (added 2 item-menu New tests to `ContextMenu.test.ts`; 15/15 there). `git diff --name-only origin/main...` →
  only the 3 frontend files (no `.rs`/`Cargo`/`bindings.gen.ts`).
