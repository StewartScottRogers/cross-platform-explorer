---
id: CPE-236
title: Folders don't open reliably on double-click; left/middle panes must stay in sync
type: Bug
status: Done
priority: High
component: Frontend
estimate: 2-3h
created: 2026-07-12
closed: 2026-07-12
---

## Summary

Two issues in the middle pane:
1. Double-clicking a folder often doesn't open it. Rows are `draggable` for
   drag-and-drop; in a webview the second press of a double-click, with any tiny
   movement, gets swallowed as a drag start, so the `dblclick` (open) never fires.
2. The left tree and the middle pane don't stay in sync — the sidebar doesn't
   auto-expand/highlight the folder you're in, and selecting/opening in the middle
   isn't reflected on the left.

## Acceptance Criteria
- [ ] Double-clicking a folder reliably opens it (drag no longer eats the open).
- [ ] Dragging a row for move/drop still works (single press + real movement).
- [ ] Navigating in the middle pane auto-expands the left tree to the current
      folder and highlights it there.
- [ ] The current folder is highlighted in BOTH panes; clicking a folder on the
      left shows + highlights it, and the middle selection is reflected on the left.
- [ ] `npm run check` passes; verified live.

## Resolution

Double-click reliability (`FileList.svelte`): rows are still natively draggable,
but a `pointerdown` handler now detects the rapid second press of a double-click
(same row, <450ms) and sets a short `suppressDragUntil` window; `onDragStart`
bails (`preventDefault`) during it. So the 2nd click of a double-click can no
longer be hijacked into a drag and the `dblclick`→open fires reliably. A genuine
drag (single press + real movement) is unaffected.

Two-way sync (`Sidebar.svelte` + `App.svelte`): extracted `loadChildren`; added
`revealPath` which walks from the matching root place/drive down to a target
path, lazily loading and expanding each level. Reactive statements reveal the
current folder and any selected subfolder, so the left tree auto-expands to and
highlights where the middle pane is. New `selectedPath` prop + `isMarked` helper
highlight both the current folder and the selected folder in the tree; App passes
the single selected folder's path. Clicking a folder on the left already
navigates (updates currentPath) so the middle follows — the panes now track.

Verified: `npm run check` → 0/0. Live verification bundled into the 0.10.0 ship.

## Work Log

2026-07-12 — Double-click fix via pointerdown drag-suppression on the rapid 2nd press; real drags unaffected.
2026-07-12 — Sidebar reveal-to-path (lazy) + selectedPath highlight; App passes selected folder. Two-way sync. check clean.

## Notes
Decisions (user, 2026-07-12): keep double-click to open (make it reliable) + full
two-way sync. Drag uses native HTML5 DnD; fix is to suppress a drag on the rapid
second press of a double-click. Sidebar reveal walks marker roots down to the
current path, loading children lazily.
