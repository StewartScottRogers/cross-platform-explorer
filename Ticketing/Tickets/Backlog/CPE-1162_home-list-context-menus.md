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
- [x] Right-clicking a row in **Recent / Favorites / Folders** opens a context menu with the actions above,
      adapted to file vs folder, targeting that row's real path.
- [x] Destructive **Delete (Recycle Bin)** + **Rename** act on the real file, clearly separated from the
      pointer-level **Remove from <view>** action.
- [x] Cross-view **Add to Favorites / Pin** (Recent/Folders) and **Remove from Favorites** (Favorites) work.
      (Pin is folder-only — pinning a file to the folder-oriented Quick access is a broken tile; see Work Log.)
- [x] Stale/missing targets: Open/Reveal/Rename/Delete disabled, Remove-from-list still works.
- [x] Blank Home background opens NO menu; drive tiles (CPE-1158) unchanged; row click/dblclick unchanged.
- [x] `npm run check` green; tests cover per-view dispatch + the file-vs-folder action set + stale-target
      disabling. A CDP-harness (CPE-1155) spec right-clicks a Home row and asserts the menu opens + STAYS
      (authored `gui-smoke/specs/home-item-menu.smoke.ts`; runs in CI — see Work Log for why it can't run
      in this worktree).
- [ ] **Shared**: machinery is view-agnostic and ready; Shared's menu is wired once its data source is chosen
      (Open Question) — split to a follow-up if Shared needs its own feature build. **Deferred** — Shared is
      an empty/disabled tab awaiting its data-source decision (now tracked as CPE-1163). The machinery is
      view-agnostic (`view` is a plain string end-to-end); Shared plugs in with one more `view` value.

## Work Log
- **2026-07-31 — Recent/Favorites/Folders landed (Shared deferred to CPE-1163).**
  - **`homeItemContext` pattern (mirrors CPE-1158 `driveContext`).** Home has no `<FileList>`/selection, so
    each `HomeView` row (`recent`/`favorites`/`folders`) now dispatches `homeItemContext {x,y,path,is_dir,view}`
    on `contextmenu`, with **both** `preventDefault()` **and** `stopPropagation()` (the stopPropagation is
    required per CPE-1157/1159 or the window-level dismisser self-closes the just-opened menu). `ExplorerPane`
    forwards it up even while `inHome` (distinct from `contextEmpty`, which stays suppressed on Home), and
    `App.onHomeItemContext` stores the target in dedicated `homeCtx*` state (independent of the FileList
    selection) and opens `ContextMenu` with the new `target: "home-item"` branch. The blank-Home guarantee is
    untouched — only real rows dispatch; `paneContext`'s `inHome` guard is unchanged.
  - **File-delete vs remove-from-list kept unmistakably distinct.** Two different verbs: `home-delete` trashes
    the real file (existing `delete_to_trash` + undo, then prunes the now-dead pointer) and sits in the
    destructive group; `home-remove` prunes only the list ENTRY (`removeRecent`/`unfavorite`/`removeRecentFolder`)
    and sits in a separate list-management group at the bottom with distinct wording ("Remove from Recent/
    Favorites/Recent folders", + "Clear all" on Recent). A separator divides the two groups. MENUS.md styling
    (theme vars, leading icons, never red text).
  - **Stale targets.** On menu-open a best-effort async existence check reuses `entries_for_paths` (the same
    stat-a-path command Home's preview uses); a missing target sets `homeStale`, which disables the on-disk
    rows (Open/Open-in-new-tab/Reveal/Copy/Copy-as-path/Rename/Delete/Properties and hides New ▸) while keeping
    "Remove from <view>" / "Clear all" enabled so a dead entry can still be pruned. A failed check is treated as
    not-stale (never wrongly disable a live entry over a hiccup); the on-disk action surfaces its own error.
  - **Rename.** Home has no inline editor, so `home-rename` navigates to the item's PARENT folder and hands off
    to the existing `pendingRenamePath` post-load inline-rename hook (same path a freshly-created item uses).
  - **Pin is folder-only.** Recent rows are files; Quick access pins folders, so pinning a file would create a
    broken tile. "Pin to Quick access" is therefore gated on `is_dir` (shown for Folders + any folder row);
    "Add to Favorites" is shown for all Recent/Folders rows. Logged as an intentional deviation from the
    ticket's flat "Recent/Folders → Pin".
  - **No backend changes / no specta regen.** Everything reuses existing App handlers + commands
    (`open`/`navigate`/`openRecent`, `revealItemInDir`, `stage`/clipboard, `move`/rename, `delete_to_trash`,
    `entries_for_paths`, `toggleFavorite`/`togglePin`, `newFolder`/`newFile` incl. CPE-1161 typed New ▸).
  - **i18n.** Added `ctx.copy`, `ctx.delete`, `home.clearAll`, `home.pinToQuickAccess` to all 12 complete
    locales (the CPE-539 coverage gate requires it); reused existing keys elsewhere (`ctx.open`,
    `ctx.openNewTab`, `ctx.reveal`, `ctx.copyAsPath`, `ctx.rename`, `ctx.properties`, `ctx.addFavorite`,
    `home.removeFrom*`, `cmd.new`, `ctx.folder`, `ctx.textFile`).
  - **Verification.** `npm run check` → 0 errors / 0 warnings. `npx vitest run` → 129 files / 1458 tests pass,
    including 12 new HomeView row-dispatch tests + 12 new `HomeItemContextMenu` branch tests (per-view remove
    label, file-vs-folder action set, delete-vs-remove distinctness, cross-view actions, stale disabling).
    Existing HomeView/ContextMenu/DriveContextMenu tests stay green.
  - **CDP harness.** Authored `gui-smoke/specs/home-item-menu.smoke.ts` (mirrors `drive-menu.smoke.ts`): a
    faithful CDP right-click on a real Home Folders row asserts the menu opens AND STAYS (self-close guard) and
    is the home-item variant. Could NOT run/typecheck it in this worktree — gui-smoke's `node_modules` is
    gitignored (not copied into git worktrees) and there is no built release binary + tauri-driver here; it
    runs in the CI gui-smoke job where both are present.
  - **Shared deferred.** Not built here — Shared is an empty/disabled tab with no backing data (Open Question).
    The machinery is view-agnostic so it plugs in once its data source is decided (CPE-1163).

## Notes
- Builds on CPE-1153 (submenu), CPE-1156 (New-in-target), CPE-1158 (Home drive `driveContext` pattern),
  CPE-1157/1159/1160 (self-close race). Cross-platform-agnostic.
