---
id: CPE-752
title: Space analyzer — reveal / delete actions from the treemap
type: feature
component: Frontend
priority: low
status: needs-prereq
created: 2026-07-19
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
- [ ] Reveal navigates to and selects the item; Delete sends it to the recycle bin with undo.
- [ ] The treemap/space totals update after a delete without a full manual rescan.
- [ ] Destructive delete confirms sensibly; theme-only menu styling.

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
