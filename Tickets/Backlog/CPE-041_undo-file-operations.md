---
id: CPE-041
title: Undo the last file operation (Ctrl+Z)
type: Feature
status: Open
priority: Low
component: Frontend
estimate: 2-3h
created: 2026-07-11
closed:
---

## Summary

Explorer supports Ctrl+Z for file operations. Undo rename, move, and recycle-bin delete.

## Acceptance Criteria

- [ ] Ctrl+Z undoes the last rename, move, or delete-to-trash
- [ ] Undo of a delete restores from the Recycle Bin
- [ ] Operations that cannot be undone (permanent delete) are never offered as undoable
- [ ] The undo stack is bounded and cleared on navigation errors
- [ ] An undo that fails reports why instead of silently doing nothing

## Resolution
## Work Log
## Notes
Copy is intentionally NOT undoable-by-deletion — silently deleting a user's file to "undo" a copy is
too dangerous.
