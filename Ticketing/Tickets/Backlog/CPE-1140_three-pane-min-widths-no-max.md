---
id: CPE-1140
title: "Three-pane layout: enforce sensible per-pane minimum widths, remove all maximum widths, keep the middle pane showing its columns"
type: bug
component: Frontend
priority: high
status: Backlog
tags: ready
created: 2026-07-29
epic: CPE-688
---

## Summary
User-reported layout issue with the main three-pane explorer (left sidebar · middle file list · right
preview/details). The current constraints (`src/App.svelte` ≈ lines 269-283) are:
- **Left (sidebar):** `SIDEBAR_MIN = 160`, **`SIDEBAR_MAX = 480`**, default 220.
- **Middle (file list):** grid track `1fr` — **no minimum width at all** (can be squeezed to nothing when the
  window is narrow and the side panes are wide).
- **Right (preview/details):** `RIGHT_MIN = 220`, **`RIGHT_MAX = 560`**, default 300.

Desired behaviour (from the user):
1. **Every pane has a reasonable minimum width** — left, middle, AND right. (Left/right mins already exist;
   the **middle has none** and must get one.)
2. **No pane has a maximum width** — remove `SIDEBAR_MAX` and `RIGHT_MAX` so the user can drag the side panes
   as wide as they like.
3. **The middle pane reasonably shows all its columns, and is never narrower than its leftmost column** — the
   middle's minimum width must be at least enough to display the file-list columns (Name/Size/Date/…), and in
   no case smaller than the leftmost **Name** column on its own.

## Design
- **Middle-pane minimum (the core fix).** Change the middle grid track from `1fr` to `minmax(<MID_MIN>, 1fr)`
  in both `gridCols` and `effectiveGridCols` (and the dual-pane variant's file columns) so the middle can grow
  to fill but never collapse below `<MID_MIN>`. Derive `<MID_MIN>` from the file-list column layout in
  `FileList.svelte` — the sum of the visible columns' minimum widths (Name + Size + Modified + any enabled
  extras), with an absolute floor of the **Name column's** own minimum width (requirement #3). Read the real
  column widths/`grid-template-columns` in `FileList.svelte` rather than hard-coding a guess; expose a shared
  constant if that's cleanest.
- **Remove maximums.** Delete `SIDEBAR_MAX` / `RIGHT_MAX` and the `clampWidth(..., MAX)` upper bounds in
  `onResize` (keep the lower clamps at the mins). The side panes may now grow unbounded — but the resize logic
  and grid must still guarantee the **middle never drops below `<MID_MIN>`**: when dragging a side pane wider
  would squeeze the middle below its min, stop the drag at that point (clamp the side pane so
  `window_width − other_panes − dividers − sidePane ≥ MID_MIN`), so the middle's floor wins over an unbounded
  side pane. i.e. the effective max of a side pane is dynamic (whatever leaves the middle at its min), not a
  fixed cap.
- **Narrow-window behaviour.** When the window itself is too narrow to satisfy all three mins + dividers,
  the middle keeps its min (never collapses below the Name column) and the layout may scroll / the side panes
  hold their mins — the middle's column visibility is the priority. Make sure the persisted
  sidebar/right widths (`settings.saveSidebarWidth`/`saveRightWidth`) are re-clamped on load to the new rules
  (no stale over-max value causes a broken first paint).
- Keep dual-pane mode (`effectiveGridCols`, CPE-677) consistent: its two `1fr` file columns should each honour
  a sensible min too (never collapse a file column below the Name column).

## Acceptance Criteria
- [ ] The middle file-list pane has a real minimum width and **cannot be squeezed below it** by widening the
      side panes or narrowing the window; that minimum is ≥ the leftmost **Name** column and large enough to
      show the standard columns.
- [ ] Neither the left nor the right pane has a fixed maximum width — both can be dragged arbitrarily wide,
      limited only dynamically by the middle pane's minimum (the side pane stops so the middle stays ≥ its min).
- [ ] Left and right panes retain sensible minimum widths (unchanged behaviour at the low end).
- [ ] Persisted pane widths load correctly under the new rules (no stale value from the old max causes a bad
      layout); a fresh session and a resized-then-reopened session both paint correctly.
- [ ] Dual-pane mode's file columns also honour a per-column minimum (no collapse below the Name column).
- [ ] `npm run check` passes; a component/logic test covers the width-clamp math (side-pane clamp respects the
      middle min; middle min ≥ Name column). GUI-verified on the real build (build → deploy → run): drag both
      side panes very wide → middle holds its column-showing min; shrink the window → middle never loses the
      Name column.

## Notes
- Added to the workshift per the user (2026-07-29). Filed under epic CPE-688 (explorer performance/UX polish);
  it's a layout-correctness fix to the core three-pane explorer.
- Touch points: `src/App.svelte` (the `SIDEBAR_*`/`RIGHT_*` consts, `gridCols`/`effectiveGridCols`,
  `onResize`/`clampWidth`, the load-time clamp) and `src/lib/components/FileList.svelte` (column min widths →
  the derived `MID_MIN`). Follow the busy-cursor/theme conventions; no new deps.
