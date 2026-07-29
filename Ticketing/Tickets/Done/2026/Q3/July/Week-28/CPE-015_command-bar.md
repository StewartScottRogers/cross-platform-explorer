---
id: CPE-015
title: Command bar — New, clipboard actions, Sort, View, Filter, Details
type: Feature
status: Done
priority: Medium
component: Frontend
estimate: 2-3h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

Add the Win11 command bar: "New" button, icon actions (cut/copy/paste/rename/share/delete), and
Sort / View / Filter dropdowns plus a Details-pane toggle on the right.

## Acceptance Criteria

- [x] Command bar renders with New + icon actions + Sort/View/Filter + Details toggle
- [x] Sort menu changes the active sort (name/date/type/size, asc/desc) and the list re-sorts
- [x] Details toggle shows/hides the right pane
- [x] Actions that are not implemented are visibly disabled rather than silently doing nothing
- [x] Buttons have tooltips and accessible labels

## Resolution

Built the Win11 command bar: New, the six icon actions (cut/copy/paste/rename/share/delete), Open,
Sort, View, Filter, More, and a right-aligned Details toggle.

**Honesty over completeness.** Only Sort, Open, and Details are actually implemented. Everything else
is rendered (Explorer has it, so its absence would look wrong) but **disabled**, with a tooltip
saying "not implemented yet". I deliberately did not wire up delete/paste/rename: a file manager that
*pretends* to delete something is far worse than one that visibly can't. Disabled controls tell the
truth.

The Sort dropdown sets key (Name/Date modified/Type/Size) and direction, with a checkmark on the
active choice, and closes on outside click.

## Work Log

2026-07-11 — Picked up. Built the command bar with the full Explorer control set.
2026-07-11 — Deliberately left destructive/clipboard actions DISABLED rather than faking them. A file manager that pretends to delete is worse than one that can't.
2026-07-11 — Sort menu wired to the live sort state with checkmarks; closes on outside click. Closing as Done.

## Notes

Do NOT fake destructive actions (delete/paste). Disable what is not truly implemented.
