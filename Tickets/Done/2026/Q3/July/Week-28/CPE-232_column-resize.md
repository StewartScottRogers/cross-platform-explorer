---
id: CPE-232
title: File-list columns resize unreasonably — Name balloons, right column clipped
type: Defect
status: Done
priority: Medium
component: Frontend
estimate: 30m
created: 2026-07-12
closed: 2026-07-12
---

## Summary

In the details view, the columns grid is `1fr 170px 150px 100px` (Name flexes,
metadata fixed). Two bad behaviours result when the window/pane is resized:
- **Wide:** the Name column (1fr) balloons, pushing Date/Type/Size far to the
  right with a large empty gap — hard to associate a row's name with its metadata.
- **Narrow:** the fixed 420px of metadata columns don't shrink, so they overflow
  the pane and the rightmost (Size) column is clipped (the pane is `overflow-y`
  only), which is confusing.

Fix: cap the Name column, let the metadata columns shrink within bounds, and add
a trailing flexible spacer so surplus width becomes empty space on the right.
Columns then stay left-packed and no wider than needed.

## Acceptance Criteria

- [ ] On a wide window, columns do not stretch full-width; surplus space is empty
      on the right, columns stay left-packed.
- [ ] The Name column no longer balloons; long names still truncate with ellipsis.
- [ ] On a narrow pane, metadata columns shrink instead of clipping the Size
      column off-screen.
- [ ] Header and rows stay perfectly aligned (same grid template).
- [ ] `npm run check` passes; verified visually by resizing the running build.

## Resolution

Replaced the `.columns` / `.row` grid template (`1fr 170px 150px 100px`) with a
shared `--filelist-cols` var: `minmax(120px,640px) minmax(96px,168px)
minmax(84px,148px) minmax(60px,96px) 1fr`. Name is capped so it can't balloon;
metadata columns shrink within bounds so a narrow pane doesn't clip Size; the
trailing `1fr` soaks up surplus width as empty space on the right. Header and rows
share the same var so they stay aligned.

Verified live on installed **0.9.1** at a 2200px-wide window (scratchpad
`25-columns-wide-091.png`): Name capped ~640px, Date/Type/Size packed immediately
to its right, large empty area on the right — columns left-packed and no wider
than needed, matching the request. `npm run check` clean; shipped in 0.9.1.

## Work Log

2026-07-12 — Diagnosed: Name=1fr balloons on wide windows; fixed metadata columns clip Size on narrow panes.
2026-07-12 — Capped Name + shrinkable metadata + trailing spacer via shared --filelist-cols. Shipped in 0.9.1.
2026-07-12 — Verified live on 0.9.1 at 2200px wide (screenshot). Closed.

## Notes

Grid lives in `src/app.css` (`.columns` + `.row`, kept identical). Header is 4
buttons, rows are 4 cells; the trailing spacer is a 5th, empty grid track.
