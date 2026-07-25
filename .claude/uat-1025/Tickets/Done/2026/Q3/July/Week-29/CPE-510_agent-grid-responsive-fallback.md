---
id: CPE-510
title: "Agent Grid — responsive narrow-window fallback"
type: Feature
status: Done
priority: Low
component: Sidecar
tags: [needs-prereq]
estimate: 1-2h
created: 2026-07-16
epic: CPE-501
closed: 2026-07-16
---

## Summary
Keep the grid usable when the AI Console window is narrow ([[CPE-501]]). Below a width threshold the
grid degrades gracefully — tiles stop shrinking past a minimum and the layout collapses toward a single
column / falls back to the focused pane — rather than rendering unreadable slivers.

## Acceptance Criteria
- [x] Tiles never shrink below a legible minimum width/height; below the threshold the grid collapses
      to a single column (or the focused pane) instead of many unreadable slivers.
- [x] The Tabs⇄Grid toggle + per-pane headers stay reachable and their pills/chips **reflow** (never
      overflow) at narrow widths (tick-tack rule).
- [x] No horizontal scroll of the whole window; any overflow is contained within a scrollable region.
- [x] Tests for the columns-for-width breakpoint logic.

## Resolution
Made the grid responsive to a narrow AI Console window, in `launcher.html`.

- **Width-aware columns:** a pure, tested `colsForWidth(n, width, minTileW)` never uses more columns
  than the near-square ideal **and** never so many that a tile drops below `MIN_TILE_W = 320px` — so a
  narrow window collapses toward a single column instead of unreadable slivers. `applyView` feeds it
  `#terms.clientWidth`; `width <= 0` (headless) falls back to the ideal, so the existing grid tests are
  unaffected. A **window `resize`** listener recomputes columns live.
- **No horizontal window scroll:** `#terms.grid-view` uses `minmax(0,1fr)` columns (tiles shrink inside
  the width) with `overflow-x: hidden` + `overflow-y: auto`, and rows flow via `grid-auto-rows:
  minmax(160px,1fr)` — many panes on a narrow window scroll **vertically**, contained, never sideways.
- **Reflowing controls:** the pane header keeps the chip `flex:0 0 auto` + label ellipsis (nowrap, no
  overflow) and the tab strip retains `overflow-x:auto` — chips/labels never spill (tick-tack rule).

Tests: `colsForWidth` breakpoints (wide→ideal, narrow→fewer, very-narrow→1, zero-width→ideal). 48
launcher + 523 frontend tests pass; `npm run check` clean.

## Work Log
2026-07-16 — Picked up (dayshift; prereq CPE-506). Estimate: 1-2h.
2026-07-16 — Added pure colsForWidth + width-aware columns in applyView, a resize listener, minmax columns + auto-rows + contained vertical scroll (no horizontal). 2 new jsdom tests.
2026-07-16 — Verified: 48 launcher + 523 frontend tests pass; `npm run check` clean. Applied the standing tick-tack reflow rule to the pane header. All ACs met.

## Notes
**needs-prereq:** [[CPE-506]]. Closes the CPE-501 "narrow-window / responsive behaviour" open question.
Applies the standing tick-tack reflow rule.
