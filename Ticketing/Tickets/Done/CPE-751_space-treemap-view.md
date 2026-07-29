---
id: CPE-751
title: Space analyzer — treemap/sunburst visualization with drill-down
type: feature
component: Frontend
priority: medium
status: Open
tags: needs-prereq
created: 2026-07-19
epic: CPE-706
estimate: 4h+
---

## Summary
Child of CPE-706 — the headline "what's eating my disk?" view. An interactive **treemap** (or sunburst)
of a folder's space, sized by each child's recursive size (from CPE-749), that you can **drill into**
(click a folder to make it the new root, breadcrumb back out) and that surfaces the largest files/folders.

## Scope
- A Space view/pane rendering `dir_children_sizes` as a treemap: rectangle area ∝ size, labelled, themed
  (light/dark), with hover tooltips (name + size + % of parent).
- Drill-down: click a folder tile to re-root the map on it; a breadcrumb to climb back; a running total.
- Largest-items surfacing (a side list of the biggest children).
- Rendering approach: **SVG/canvas, zero-dependency** (CSP forbids external libs); must stay responsive on
  a few thousand tiles (cap depth/tiles, aggregate the long tail into an "other" tile).
- Opt-in + cancellable (reuse CPE-749's cancel): no scanning until the user opens the view; leaving it
  stops any in-flight scan (fast-when-off, PURPOSE.md).

## Acceptance
- [ ] Opening Space on a folder shows a treemap sized by child recursive size, with tooltips.
- [ ] Clicking a folder drills in; the breadcrumb climbs back; largest items are listed.
- [ ] Zero external deps; responsive on large trees; scanning is opt-in and cancellable.

## Notes
Prereq: CPE-749 (`dir_children_sizes`). **Attended — GUI verification** (layout, drill interaction, theme).
Follow the dataviz skill for palette/legend/tooltip conventions.
