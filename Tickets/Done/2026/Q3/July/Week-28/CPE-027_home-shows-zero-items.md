---
id: CPE-027
title: Home reports "0 items" in the details pane and status bar
type: Defect
status: Done
priority: Low
component: Frontend
estimate: 15m
created: 2026-07-11
closed: 2026-07-11
---

## Summary

On Home, the details pane reads "Home (0 items)" and the status bar reads "0 items", because Home is
not a directory listing and `visible` is empty. The reference reads "Home (42 items)". Reporting zero
when the page is visibly full of Quick access entries is simply wrong.

## Acceptance Criteria

- [x] On Home, the item count reflects the Quick access entries shown
- [x] The status bar agrees with the details pane
- [x] Folder views are unaffected

## Resolution

Home is a sentinel view rather than a directory listing, so `visible` is empty there and both the
details pane and the status bar read "0 items" while the page was visibly full of Quick access cards.

Added a derived `itemCount` that, on Home, counts the places and drives actually rendered, and fed the
same value to both the details pane and the status bar so the two can never disagree. Folder views are
unchanged.

## Work Log

2026-07-11 — Spotted in the running app: "Home (0 items)" beside a screen full of cards.
2026-07-11 — Derived a single itemCount shared by the details pane and status bar. Closing as Done.

## Notes
