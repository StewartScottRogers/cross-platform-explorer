---
id: CPE-021
title: Search box filters the current folder
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 1h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

The toolbar search box in Explorer searches the current location. Implement it as a live filter over
the current listing (honest scope: filter, not recursive search).

## Acceptance Criteria

- [x] Search box filters the visible listing case-insensitively as the user types
- [x] Item count in the status bar reflects the filtered count
- [x] Clearing the box restores the full listing
- [ ] An empty result shows a "No items match" state
- [x] Placeholder text names the current folder, like Explorer ("Search Home")

## Resolution

The toolbar search box live-filters the current listing, case-insensitively. The status bar shows the
filtered count and marks it "(filtered)" so a reduced count is never mistaken for an empty folder. An
empty result shows "No items match your search" rather than "This folder is empty" — different cause,
different message. Clearing the box restores the listing, and navigating anywhere resets it. The
placeholder names the current folder ("Search Documents"), like Explorer.

Scope is deliberate and honest: this **filters the loaded folder**, it does not recursively search the
subtree. Explorer's box does recurse; ours doesn't, and the ticket says so rather than implying parity.

## Work Log

2026-07-11 — Picked up. Implemented as a live filter over the loaded listing.
2026-07-11 — Distinguished "no matches" from "empty folder" — same blank pane, different causes, so different copy.
2026-07-11 — Marked the count "(filtered)" so it can't be misread. Recursive search is explicitly NOT claimed. Closing as Done.

## Notes

Scoped deliberately to filtering the loaded folder. Recursive search is a separate concern.
