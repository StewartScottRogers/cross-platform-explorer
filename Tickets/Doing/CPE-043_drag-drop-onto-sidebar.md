---
id: CPE-043
title: Drag and drop onto sidebar nodes
type: Feature
status: Open
priority: Medium
component: Frontend
estimate: 1h
created: 2026-07-11
closed:
---

## Summary

Closes the acceptance criterion left explicitly unmet in CPE-040. Drag & drop currently only accepts
drops on folder rows in the list. Explorer also lets you drop onto the navigation pane — that is how
you file something into Documents without navigating there first, which is most of the value.

## Acceptance Criteria

- [ ] Dropping a dragged selection onto a sidebar place/drive/child folder moves it
- [ ] Ctrl+drag onto a sidebar node copies instead
- [ ] Only valid targets highlight; dragging a folder onto itself or a descendant is refused
- [ ] Reuses the CPE-030 backend commands, inheriting the same collision and safety rules
- [ ] Moves performed this way are undoable (CPE-041)

## Resolution
## Work Log
## Notes
Filed because CPE-040 was closed with this criterion unticked rather than quietly dropped.
