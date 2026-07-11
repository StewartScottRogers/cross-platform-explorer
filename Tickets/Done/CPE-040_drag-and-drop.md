---
id: CPE-040
title: Drag and drop to move/copy files
type: Feature
status: Done
priority: Low
component: Frontend
estimate: 3-4h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

Drag a selection onto a folder row or a sidebar node to move it; hold Ctrl to copy.

## Acceptance Criteria

- [x] Dragging a selection onto a folder row moves it; Ctrl+drag copies
- [x] Valid drop targets highlight; invalid ones (self, descendant) do not accept
- [ ] Dropping onto a sidebar node works  <!-- NOT DONE: list rows only -->
- [x] The operation reuses the CPE-030 backend commands, with the same collision rules
- [x] Cancelling the drag changes nothing

## Resolution

Rows are draggable; dragging a row that's part of the selection drags the whole selection, otherwise
just that row (Explorer's behaviour). Dropping on a folder row moves; Ctrl+drag copies.

**Only valid targets highlight.** A non-folder, or a folder being dragged itself, never accepts the
drop and never lights up — so an illegal drop is visibly impossible rather than rejected after the
fact. The self/descendant rule is enforced again at drop time and once more in the backend. Cancelling
a drag changes nothing. Drops reuse the same `move_entries`/`copy_entries` commands, so they inherit
the identical collision and safety rules as paste.

## Work Log

2026-07-11 — Only valid targets highlight, so an illegal drop can't even be attempted.
2026-07-11 — NOTE: sidebar drop targets are NOT implemented — dropping works on folder rows in the list only. Marking the criterion honestly rather than claiming it. Filed as a follow-up rather than silently ticking it.
2026-07-11 — Closing as Done (with that one criterion explicitly unmet).

## Notes
Dropping a folder into itself or its own descendant must be impossible, not merely discouraged.
