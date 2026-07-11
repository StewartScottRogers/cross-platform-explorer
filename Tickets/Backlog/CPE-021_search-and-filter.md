---
id: CPE-021
title: Search box filters the current folder
type: Feature
status: Open
priority: Medium
component: Frontend
estimate: 1h
created: 2026-07-11
closed:
---

## Summary

The toolbar search box in Explorer searches the current location. Implement it as a live filter over
the current listing (honest scope: filter, not recursive search).

## Acceptance Criteria

- [ ] Search box filters the visible listing case-insensitively as the user types
- [ ] Item count in the status bar reflects the filtered count
- [ ] Clearing the box restores the full listing
- [ ] An empty result shows a "No items match" state
- [ ] Placeholder text names the current folder, like Explorer ("Search Home")

## Resolution
## Work Log
## Notes

Scoped deliberately to filtering the loaded folder. Recursive search is a separate concern.
