---
id: CPE-015
title: Command bar — New, clipboard actions, Sort, View, Filter, Details
type: Feature
status: Open
priority: Medium
component: Frontend
estimate: 2-3h
created: 2026-07-11
closed:
---

## Summary

Add the Win11 command bar: "New" button, icon actions (cut/copy/paste/rename/share/delete), and
Sort / View / Filter dropdowns plus a Details-pane toggle on the right.

## Acceptance Criteria

- [ ] Command bar renders with New + icon actions + Sort/View/Filter + Details toggle
- [ ] Sort menu changes the active sort (name/date/type/size, asc/desc) and the list re-sorts
- [ ] Details toggle shows/hides the right pane
- [ ] Actions that are not implemented are visibly disabled rather than silently doing nothing
- [ ] Buttons have tooltips and accessible labels

## Resolution
## Work Log
## Notes

Do NOT fake destructive actions (delete/paste). Disable what is not truly implemented.
