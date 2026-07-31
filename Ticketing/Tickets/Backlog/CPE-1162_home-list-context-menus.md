---
id: CPE-1162
title: "Right-click context menus for the Home lists (Recent / Favorites / Folders / Shared)"
type: feature
component: Frontend
priority: high
status: Backlog
tags: ready
created: 2026-07-31
---

## Summary
User-requested (2026-07-31, confirmed via Q&A). The Home landing's segmented list — **Recent / Favorites /
Folders / Shared** (`HomeView.svelte`) — has no right-click menu today (rows only respond to click/dblclick +
a tiny × button; Home suppresses context menus except CPE-1158 drive tiles). Give each row a context menu with
the normal file/folder actions, adapted to the view's peculiarities.

## Confirmed decisions (user, 2026-07-31)
1. **Include destructive file ops** — the menu offers Delete (to Recycle Bin) + Rename acting on the ACTUAL
   file/folder on disk, like a normal listing — IN ADDITION to the pointer-level "Remove from this list".
2. **Include Shared** — but Shared has NO data/implementation today (disabled, empty tab). Its data source
   must be DEFINED first (see Open Question). Build the menu machinery view-agnostic so Shared plugs in once
   it has content.
3. **Include cross-view actions** — Recent/Folders rows get "Add to Favorites" / "Pin to Quick Access";
   Favorites rows get "Remove from Favorites".

## Behaviour per view (each row passes path + type + source-view)
Standard, adapted to file vs folder (Favorites carry `is_dir`; Recent = files; Folders = folders):
- **Open** (file → openFile; folder → navigate) · **Open in new tab** (folder) · **Open containing folder /
  Reveal in OS** · **Copy** · **Copy as path** · **Rename…** (real file) · **Delete** (real file → Recycle
  Bin, the app's existing trash path) · **Properties** · for a folder: **New ▸** (create inside).
- **View-native pointer action** (also on the existing × button): **Remove from Recent** / **Remove from
  Favorites** / **Remove from Recent Folders**, + **Clear all** where it exists. Labelled so it's clearly
  removing the ENTRY, not the file.
- **Cross-view:** Recent/Folders → **Add to Favorites** + **Pin to Quick Access**; Favorites → **Remove from
  Favorites**.

## Peculiarities to handle
- **Pointers can be stale** (target moved/deleted): if the path no longer exists, DISABLE Open/Reveal/Copy/
  Rename/Delete but KEEP "Remove from …" enabled (pruning dead entries). Best-effort existence check.
- **Pointer-remove vs file-delete are DISTINCT**: "Remove from Recent" prunes the list entry; "Delete" trashes
  the real file. Both present (per decision #1) but must read unmistakably differently (wording + grouping;
  Delete in the destructive group, Remove-from-list in the list-management group).
- **Home has no FileList selection** — mirror CPE-1158's `driveContext`: each row dispatches a context event
  carrying `{x,y,path,is_dir,view}`; `ExplorerPane` forwards it even while `inHome`; App builds the menu +
  routes actions to that path. The blank Home background stays **menu-less** (do not re-enable the empty-area
  menu on Home).
- **Reuse `ContextMenu`** via a new `target` (e.g. `"home-item"`) or by synthesizing a DirEntry + a
  `homeView`/removable flag, so the right actions + the correct "Remove from <view>" show. Follow MENUS.md
  (theme vars, leading icons); reuse existing App handlers (openFile/navigate/reveal/rename/delete-to-trash/
  properties/removeRecent/unfavorite/removeRecentFolder/favorite/pin).
- **Must stopPropagation** on the row `contextmenu` handler (per CPE-1157/1159 — or rely on CPE-1160's
  hardening if that lands first) so the menu doesn't self-close.

## Open question (Shared) — needs a one-line decision to fully land Shared
"Shared" is an empty, disabled tab with no backing data. To attach a menu it must first LIST something.
Candidates: (a) network / mapped / SMB shares + mounts; (b) folders the user has shared out; (c) "shared with
me" (cloud). Until chosen, the ticket lands Recent/Favorites/Folders + the reusable machinery; Shared's menu
follows once its content is defined (tracked here). Do NOT silently invent a whole Shared feature.

## Acceptance Criteria
- [ ] Right-clicking a row in **Recent / Favorites / Folders** opens a context menu with the actions above,
      adapted to file vs folder, targeting that row's real path.
- [ ] Destructive **Delete (Recycle Bin)** + **Rename** act on the real file, clearly separated from the
      pointer-level **Remove from <view>** action.
- [ ] Cross-view **Add to Favorites / Pin** (Recent/Folders) and **Remove from Favorites** (Favorites) work.
- [ ] Stale/missing targets: Open/Reveal/Rename/Delete disabled, Remove-from-list still works.
- [ ] Blank Home background opens NO menu; drive tiles (CPE-1158) unchanged; row click/dblclick unchanged.
- [ ] `npm run check` green; tests cover per-view dispatch + the file-vs-folder action set + stale-target
      disabling. A CDP-harness (CPE-1155) test right-clicks a Home row and asserts the menu opens + STAYS.
- [ ] **Shared**: machinery is view-agnostic and ready; Shared's menu is wired once its data source is chosen
      (Open Question) — split to a follow-up if Shared needs its own feature build.

## Notes
- Builds on CPE-1153 (submenu), CPE-1156 (New-in-target), CPE-1158 (Home drive `driveContext` pattern),
  CPE-1157/1159/1160 (self-close race). Cross-platform-agnostic.
