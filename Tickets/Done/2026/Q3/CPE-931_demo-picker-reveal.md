---
id: CPE-931
title: Hide the demo dropdown until "Load demo", and place it in the revealed swarm area
type: feature
component: Sidecar
priority: medium
tags: ready
created: 2026-07-23
closed: 2026-07-23
status: Done
---

## Summary
The demo dropdown sat permanently in the Agent Deck toolbar. Per request: the dropdown should not appear
until the user clicks **Load demo**, and then it should live in the **freshly-revealed swarm area**, not the
toolbar.

- Removed `#demo-select` from the toolbar; the toolbar now just has **Load demo ▾**.
- Moved the dropdown into the swarm row inside a `#demo-picker` that's hidden by default.
- **Load demo** reveals the swarm row + the demo picker and loads the default demo; **Run swarm** reveals
  the row WITHOUT the picker.
- Changing the dropdown re-loads that demo's tasks (change handler), no re-click needed.

## Acceptance Criteria
- [x] No demo dropdown visible until "Load demo" is clicked.
- [x] After "Load demo", the dropdown appears in the revealed swarm area with the default demo loaded.
- [x] Run swarm reveals the task row without the demo picker. Browser-verified; harness + full suite (926).

## Work Log
- 2026-07-23 — Restructured the swarm controls; verified the reveal flow in a browser.
