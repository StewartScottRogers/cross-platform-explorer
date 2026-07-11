---
id: CPE-037
title: Editable address bar (Ctrl+L / Alt+D)
type: Feature
status: Open
priority: Medium
component: Frontend
estimate: 1-2h
created: 2026-07-11
closed:
---

## Summary

Ctrl+L or Alt+D (or clicking the empty part of the address bar) turns the breadcrumb into a text box
so a path can be typed or pasted.

## Acceptance Criteria

- [ ] Ctrl+L / Alt+D focuses and selects an editable path field
- [ ] Enter navigates; Escape reverts to breadcrumbs
- [ ] A nonexistent path shows a clear error and does not navigate
- [ ] Environment variables (e.g. %USERPROFILE%) are expanded
- [ ] Clicking a breadcrumb still navigates as before

## Resolution
## Work Log
## Notes
