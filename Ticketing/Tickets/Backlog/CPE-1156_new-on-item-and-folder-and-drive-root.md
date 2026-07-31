---
id: CPE-1156
title: "New ▸ must be reachable on item/folder right-click (create inside the folder) and at drive roots"
type: bug
component: Frontend
priority: high
status: Backlog
tags: ready
created: 2026-07-30
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
- [ ] Right-clicking a **file item** shows **New ▸**; choosing Folder/Text file creates it in the **current
      folder** and inline-renames it.
- [ ] Right-clicking a **folder item** shows **New ▸**; choosing Folder/Text file creates the new item
      **inside that folder** (verified it lands in the subfolder, not the current one), with a sensible,
      documented UX (navigate-in or notice).
- [ ] In a folder that **has one or more items**, New is reachable (via the item menu and/or the blank area) —
      the reported "can't find New when there's an item" case is resolved.
- [ ] At a **drive root**, New creates an item at the drive root (empty-area menu; and on-item if a drive is
      shown as an item).
- [ ] Empty-folder New, CPE-1153 submenus, and CPE-1154 native-menu suppression all still work; `npm run check`
      green; tests updated/added (a component test that New-from-item dispatches with the folder-target for a
      folder item vs. current-folder for a file item; keep existing green).

## Notes
- No backend change expected (`create_dir`/`create_file` already accept an arbitrary parent path).
- Mirrors Windows-Explorer intuition the user is reaching for; keep cross-platform-agnostic. Menu items follow
  MENUS.md (theme vars, leading icons).
