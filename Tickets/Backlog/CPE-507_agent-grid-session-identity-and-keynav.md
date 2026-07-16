---
id: CPE-507
title: "Agent Grid — per-pane session identity + keyboard navigation"
type: Feature
status: Open
priority: Medium
component: Sidecar
tags: [needs-prereq]
estimate: 2-3h
created: 2026-07-16
epic: CPE-501
---

## Summary
Make each grid tile self-identifying and keyboard-navigable ([[CPE-501]]). Each pane gets a small
header carrying the **session-identity chip** (colour+number from [[CPE-490]], matching the explorer's
left-pane Agents leaf and the tab chip), the agent/model label, and usage. Add a visible **focus ring**
and **keyboard navigation** between panes.

## Acceptance Criteria
- [ ] Each tile shows a header with the CPE-490 session chip (same colour+number), agent + short model,
      and the per-session usage/cost (hidden until reported), reusing `sessionChip` logic.
- [ ] The focused pane has a clear visual ring; focus follows click and keyboard nav.
- [ ] Keyboard navigation moves focus between panes (e.g. Ctrl/Alt+Arrow), without stealing keys the
      terminal needs; a documented shortcut appears in the Help/keys panel.
- [ ] An ended session's tile shows the ended state (strikethrough label, as tabs do) but stays until
      closed.
- [ ] Tests for the focus-move logic (next/prev/directional index math) in the jsdom harness.

## Notes
**needs-prereq:** [[CPE-506]] (the grid + focused-pane model). Carries the CPE-490 chip into grid slots
per CPE-501 scope.
