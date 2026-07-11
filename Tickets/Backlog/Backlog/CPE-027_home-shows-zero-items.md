---
id: CPE-027
title: Home reports "0 items" in the details pane and status bar
type: Defect
status: Open
priority: Low
component: Frontend
estimate: 15m
created: 2026-07-11
closed:
---

## Summary

On Home, the details pane reads "Home (0 items)" and the status bar reads "0 items", because Home is
not a directory listing and `visible` is empty. The reference reads "Home (42 items)". Reporting zero
when the page is visibly full of Quick access entries is simply wrong.

## Acceptance Criteria

- [ ] On Home, the item count reflects the Quick access entries shown
- [ ] The status bar agrees with the details pane
- [ ] Folder views are unaffected

## Resolution
## Work Log
## Notes
