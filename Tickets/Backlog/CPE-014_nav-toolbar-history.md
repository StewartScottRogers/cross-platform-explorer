---
id: CPE-014
title: Navigation toolbar — back / forward / up / refresh with history
type: Feature
status: Open
priority: High
component: Frontend
estimate: 1-2h
created: 2026-07-11
closed:
---

## Summary

Explorer's toolbar has back, forward, up, refresh, an address breadcrumb, and a search box. We only
have up/home. Add a real history stack.

## Acceptance Criteria

- [ ] Back/forward buttons driven by a history stack, disabled when unavailable
- [ ] Up navigates to parent; Refresh reloads the current directory
- [ ] Navigating to a new path truncates the forward history (standard behaviour)
- [ ] History logic is a pure, unit-tested module
- [ ] Address breadcrumb restyled to Win11 (chevron separators)

## Resolution
## Work Log
## Notes
