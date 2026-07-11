---
id: CPE-018
title: Home view — Quick access grid and Recent list
type: Feature
status: Open
priority: Medium
component: Frontend
estimate: 2-3h
created: 2026-07-11
closed:
---

## Summary

Explorer's Home shows a collapsible "Quick access" grid of pinned folders (icon, name, subtitle) and
a Recent/Favorites/Shared pill switcher over a recent-files list.

## Acceptance Criteria

- [ ] Home is a distinct view, shown at startup, reachable from the sidebar
- [ ] Quick access renders special folders as a two-column card grid
- [ ] Section is collapsible via its chevron
- [ ] Recent/Favorites/Shared pills render; unimplemented ones are disabled, not fake
- [ ] Clicking a Quick access card navigates to that folder

## Resolution
## Work Log
## Notes

Do not fabricate a "Recent files" list if the data isn't tracked — show an honest empty state.
