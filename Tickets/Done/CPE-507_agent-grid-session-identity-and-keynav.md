---
id: CPE-507
title: "Agent Grid — per-pane session identity + keyboard navigation"
type: Feature
status: Done
priority: Medium
component: Sidecar
tags: [needs-prereq]
estimate: 2-3h
created: 2026-07-16
epic: CPE-501
closed: 2026-07-16
---

## Summary
Make each grid tile self-identifying and keyboard-navigable ([[CPE-501]]). Each pane gets a small
header carrying the **session-identity chip** (colour+number from [[CPE-490]], matching the explorer's
left-pane Agents leaf and the tab chip), the agent/model label, and usage. Add a visible **focus ring**
and **keyboard navigation** between panes.

## Acceptance Criteria
- [x] Each tile shows a header with the CPE-490 session chip (same colour+number), agent + short model,
      and the per-session usage/cost (hidden until reported), reusing `sessionChip` logic.
- [x] The focused pane has a clear visual ring; focus follows click and keyboard nav.
- [x] Keyboard navigation moves focus between panes (e.g. Ctrl/Alt+Arrow), without stealing keys the
      terminal needs; a documented shortcut appears in the Help/keys panel.
- [x] An ended session's tile shows the ended state (strikethrough label, as tabs do) but stays until
      closed.
- [x] Tests for the focus-move logic (next/prev/directional index math) in the jsdom harness.

## Resolution
Made each grid tile self-identifying and keyboard-navigable in `launcher.html`.

- **Pane header** (`.pane-head`, shown only in grid view): the CPE-490 session chip (same
  `sessionColor`/`sessionNum` as the tab + left-pane Agents leaf), the session label, and the
  provider-reported usage (mirrored from `applyUsage`, hidden until reported). A 22px top strip is
  reserved via a `#terms.grid-view` offset on the term host/scrollbar — no DOM restructure.
- **Focus ring + click-to-focus:** `focusPane(id)` sets `activeId` + the `.focused` ring + active-tab
  highlight + terminal focus **without** relaying out; a tile `mousedown` calls it in grid view.
- **Keyboard nav:** `nextPaneId(ids, current, dir, cols)` (pure, row-major, edge-clamped) drives
  **Ctrl+Alt+Arrow** in `onKey`, gated to grid view so it never steals an arrow key the terminal needs
  in tabs mode. Documented in the Help panel ("See several agents at once — Grid").
- **Ended state:** the ws `onclose` now also marks the pane `.ended` → the header label strikes through
  (mirrors the tab), and the tile stays until closed.

Tests (jsdom): `nextPaneId` directional math + edge clamps + unknown-id; each tile gets a `.pane-head`
with the right chip number + label; `focusPane` moves the ring + active highlight. 39 launcher + 514
frontend tests pass; `npm run check` clean.

## Work Log
2026-07-16 — Picked up (dayshift; prereq CPE-506 merged). Estimate: 2-3h.
2026-07-16 — Added the pane header (chip+label+usage) via a grid-view top strip, focusPane + click-to-focus, nextPaneId + Ctrl+Alt+Arrow keynav, ended-pane styling, and Help docs. 3 new jsdom tests.
2026-07-16 — Verified: 39 launcher + 514 frontend tests pass; `npm run check` clean. All ACs met.

## Notes
**needs-prereq:** [[CPE-506]] (the grid + focused-pane model). Carries the CPE-490 chip into grid slots
per CPE-501 scope.
