---
id: CPE-752
title: Space analyzer — reveal / delete actions from the treemap
type: feature
component: Frontend
priority: low
status: Done
created: 2026-07-19
closed: 2026-07-19
tags: needs-prereq
epic: CPE-706
estimate: 2-3h
---

## Summary
Child of CPE-706. Make the treemap actionable: from a tile (or the largest-items list), **reveal** the
item in the explorer or **delete** it (to the recycle bin, via the existing delete path + undo), then
refresh the map so freed space is reflected. Closes the loop from "find the big thing" to "remove it".

## Scope
- Context action / buttons on a treemap tile + largest-items row: "Reveal in explorer" (navigate + select)
  and "Delete" (reuse the recycle-bin delete + undo; confirm for large/many).
- After a delete, re-scan the affected subtree (or decrement cached sizes) so the map updates.
- Respect the menu design standard for any context menu (theme-only colours, never red text; MENUS.md),
  with leading icons (CPE-748 convention).

## Acceptance
- [x] Reveal navigates to and selects the item; Delete sends it to the recycle bin with undo.
      *(GUI-verified on a throwaway folder: Reveal `big6.dat` → explorer at the folder with it selected;
      Delete `big5.dat` → removed; Ctrl+Z → `restore_from_trash` → it's back.)*
- [x] The treemap/space totals update after a delete without a full manual rescan.
      *(GUI-verified: after deleting `big5.dat` the largest-items list refreshed 11→10 automatically via the
      `refreshToken` re-scan.)*
- [x] Destructive delete confirms sensibly; theme-only menu styling.
      *(GUI-verified confirm: "Delete to Recycle Bin? … will be moved …", Cancel/Delete. Action buttons use
      the neutral `.lg-action` style + trash icon — no red, per the theme-only convention.)*

## Notes
Prereq: CPE-751 (the treemap surface). Reuses the existing delete-to-recycle + undo (CPE-033/044).

## Work Log
2026-07-19 (nightshift, 23:31 MST) — Picked up (prereq CPE-751 is Done). Estimate: 2-3h (kept). Plan:
slice it — Reveal first (non-destructive, reuses App's proven `revealFileInApp`), then Delete + map-refresh
(destructive → verify against a throwaway folder only). Surface: the "Largest" items list rows (cleaner
than treemap-tile context menus).

2026-07-19 (nightshift, 23:35 MST) — **Slice 1 (Reveal) implemented.** `DiskSpaceView` largest-items rows
gain a hover/focus "Reveal in explorer" action (forward icon) that dispatches `reveal(path)`; App wires
`on:reveal={(e) => { spacePath = null; revealFileInApp(e.detail); }}` — the same navigate-to-parent+select
helper the content-search and file-search reveal already use (proven). Headless green: `npm run check` 0/0;
suite **748 pass**. Live GUI drive was blocked tonight by CDP/dev-app address-bar flakiness (the app
reloaded clean per the vite log, but address-edit wouldn't trigger via synthetic events after ~4 tries) —
NOT merged. Will do the combined reveal+delete GUI verify on a freshly-relaunched dev app in the Delete
slice. Remaining: Slice 2 (Delete + map refresh + confirm).

2026-07-19 (nightshift, 23:44 MST) — **Slice 2 (Delete + refresh) implemented + both slices GUI-verified.**
- `DiskSpaceView`: added a "Delete to Recycle Bin" action per largest-items row (trash icon) →
  dispatches `delete({path,name})`. Added `refreshToken` prop; a reactive bump busts `cache[cur]` and
  re-scans so freed space shows.
- `App`: `spaceDelete(item)` — confirm → `delete_to_trash` + `reportResults` + push the same
  `kind:"delete"` undo entry the file-list delete uses (guarded by `canRestoreTrash`) → bump
  `spaceRefresh`. Kept separate from `doDelete` so the file-list delete path is untouched.
- GUI-verified on a fresh dev app + a throwaway folder (`cpe-del-test`): Reveal selects the item in the
  explorer; Delete `big5.dat` → confirm dialog → recycled (gone from folder) → list auto-refreshed 11→10;
  Ctrl+Z → `restore_from_trash` → `big5.dat` restored (folder back to 11). `npm run check` 0/0; suite 748.
- Note: the earlier "reveal GUI blocked" was a stale HMR-instance quirk; a clean dev relaunch drove
  cleanly. The undo "didn't restore" scare was a timing artifact (restore scans the whole recycle bin,
  >1s) — it did restore.

## Resolution
Made the Space analyzer actionable (CPE-752, epic CPE-706). Each "Largest" list row gains hover/focus
**Reveal** and **Delete** actions. Reveal dispatches `reveal(path)` → App's proven `revealFileInApp`
(navigate to parent + select). Delete dispatches `delete({path,name})` → App's `spaceDelete`: a confirm,
then `delete_to_trash` + an undoable `kind:"delete"` entry (reusing the file-list delete/undo mechanism,
CPE-033/044), then a `refreshToken` bump that makes `DiskSpaceView` bust its cache and re-scan so the map
reflects the freed space. Kept `doDelete` untouched (separate `spaceDelete`) to avoid regressing the hot
file-list delete path. Files: `src/lib/components/DiskSpaceView.svelte`, `src/App.svelte`. Verified:
`npm run check` 0/0; suite 748 pass; full GUI drive over CDP on a throwaway folder (reveal, delete,
auto-refresh, undo-restore all confirmed).
