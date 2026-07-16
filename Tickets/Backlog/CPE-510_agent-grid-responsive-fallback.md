---
id: CPE-510
title: "Agent Grid — responsive narrow-window fallback"
type: Feature
status: Open
priority: Low
component: Sidecar
tags: [needs-prereq]
estimate: 1-2h
created: 2026-07-16
epic: CPE-501
---

## Summary
Keep the grid usable when the AI Console window is narrow ([[CPE-501]]). Below a width threshold the
grid degrades gracefully — tiles stop shrinking past a minimum and the layout collapses toward a single
column / falls back to the focused pane — rather than rendering unreadable slivers.

## Acceptance Criteria
- [ ] Tiles never shrink below a legible minimum width/height; below the threshold the grid collapses
      to a single column (or the focused pane) instead of many unreadable slivers.
- [ ] The Tabs⇄Grid toggle + per-pane headers stay reachable and their pills/chips **reflow** (never
      overflow) at narrow widths (tick-tack rule).
- [ ] No horizontal scroll of the whole window; any overflow is contained within a scrollable region.
- [ ] Tests for the columns-for-width breakpoint logic.

## Notes
**needs-prereq:** [[CPE-506]]. Closes the CPE-501 "narrow-window / responsive behaviour" open question.
Applies the standing tick-tack reflow rule.
