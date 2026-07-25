---
id: CPE-223
title: Rename the left pane to "Navigation Pane"
type: Task
status: Done
priority: Low
component: Frontend
estimate: 15m
created: 2026-07-12
closed: 2026-07-12
---

## Summary

Standardise the left column's name to "Navigation Pane" (matches Windows Explorer; pairs with the
File List Pane and Preview Pane renames). Rename the `sidebar` wrapper class to `navigation-pane`,
label the region, and update the resizer's accessible label + the reserved-class guard.

## Acceptance Criteria

- [ ] Left-pane wrapper class is `navigation-pane` (Sidebar.svelte + app.css); `sidebar-sep` -> `navigation-pane-sep`
- [ ] The pane has role/aria-label "Navigation"
- [ ] The left resizer's aria-label becomes "Resize navigation pane" (+ tests updated)
- [ ] Reserved-class guard updated
- [ ] `npm run check` clean; suite green; build clean

## Notes

Deliberately NOT renamed (out of scope / would be harmful): the persisted width key `cpe.sidebarWidth`
and its `sidebarWidth`/`SIDEBAR_*` state — renaming the localStorage key would discard users' saved
pane widths. The component file stays `Sidebar.svelte` (consistent with keeping FileList.svelte).

## Resolution

*(Agent writes this when closing — do not fill in)*

## Work Log

2026-07-12 — Requested; took the "Navigation Pane" recommendation.

## Resolution

Renamed .sidebar -> .navigation-pane and .sidebar-sep -> .navigation-pane-sep (Sidebar.svelte + app.css), added role/aria-label "Navigation", changed the left resizer label to "Resize navigation pane" (+ tests), and updated the reserved-class guard. Kept cpe.sidebarWidth / sidebarWidth internals and the Sidebar.svelte filename as noted. check + suite (200) green; build clean.
