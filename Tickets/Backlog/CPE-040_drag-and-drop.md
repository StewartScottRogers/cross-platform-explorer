---
id: CPE-040
title: Drag and drop to move/copy files
type: Feature
status: Open
priority: Low
component: Frontend
estimate: 3-4h
created: 2026-07-11
closed:
---

## Summary

Drag a selection onto a folder row or a sidebar node to move it; hold Ctrl to copy.

## Acceptance Criteria

- [ ] Dragging a selection onto a folder row moves it; Ctrl+drag copies
- [ ] Valid drop targets highlight; invalid ones (self, descendant) do not accept
- [ ] Dropping onto a sidebar node works
- [ ] The operation reuses the CPE-030 backend commands, with the same collision rules
- [ ] Cancelling the drag changes nothing

## Resolution
## Work Log
## Notes
Dropping a folder into itself or its own descendant must be impossible, not merely discouraged.
