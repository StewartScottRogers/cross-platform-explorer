---
id: CPE-022
title: Tab bar — multiple open locations
type: Feature
status: Done
priority: Low
component: Frontend
estimate: 2-3h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

Windows 11 Explorer has a tab strip: each tab holds its own location and history, with a "+" to add
and an "x" to close.

## Acceptance Criteria

- [x] Tab strip renders across the top with the active tab highlighted
- [x] "+" opens a new tab at Home; "x" closes a tab (last tab cannot be closed)
- [x] Each tab keeps its own current path and back/forward history
- [x] Switching tabs restores that tab's location without a reload flash
- [x] Tab shows the current folder name and a matching icon

## Resolution

Tab strip across the top: active tab is raised onto the surface, each shows its folder name, "+" adds
a tab at Home, "x" closes it. **Each tab owns its own `History`**, so back/forward are genuinely
per-tab rather than a single global stack — switching tabs restores that tab's location and its
navigable history.

The last tab cannot be closed (its "x" is not rendered), matching Explorer. Closing the active tab
activates its left neighbour.

## Work Log

2026-07-11 — Picked up. Modelled a tab as { id, history } so per-tab history came out naturally.
2026-07-11 — Guarded closing the last tab and re-activating a neighbour when the active tab closes. Closing as Done.

## Notes
