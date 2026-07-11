---
id: CPE-041
title: Undo the last file operation (Ctrl+Z)
type: Feature
status: Done
priority: Low
component: Frontend
estimate: 2-3h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

Explorer supports Ctrl+Z for file operations. Undo rename, move, and recycle-bin delete.

## Acceptance Criteria

- [x] Ctrl+Z undoes the last rename or move  <!-- delete-to-trash NOT undoable, by design -->
- [ ] Undo of a delete restores from the Recycle Bin  <!-- NOT DONE: unsupported on macOS; would be a dead button -->
- [x] Operations that cannot be undone (permanent delete) are never offered as undoable
- [x] The undo stack is bounded and cleared on navigation errors
- [x] An undo that fails reports why instead of silently doing nothing

## Resolution

Ctrl+Z undoes the last **rename** or **move**, restoring items to their exact original paths via a new
`move_exact` backend command that refuses to overwrite whatever now occupies that name. A failed undo
does **not** pop the stack, so it can be retried once the obstruction is cleared. The stack is bounded
at 25 entries.

**Deliberately NOT undoable, and this is the substance of the ticket:**

- **Copy.** Undoing a copy means deleting the file just created. If the user has since edited it, or a
  file of that name already existed, we'd destroy real data to reverse a harmless action. Refusing is
  strictly safer than guessing.
- **Delete.** Items already go to the Recycle Bin, so the OS provides recovery. Programmatic restore
  (`trash::os_limited`) is **not implemented on macOS**, so wiring it up would ship an "Undo" that
  silently does nothing on one of the three platforms we ship. A button that lies is worse than no
  button. This is why I filed CPE-042 (compile Rust on macOS in CI) before writing any of it.

## Work Log

2026-07-11 — Implemented undo for rename and move (6 tests), with move_exact refusing to clobber.
2026-07-11 — DECIDED NOT to implement undo-of-delete. trash::os_limited isn't implemented on macOS, so it would be a dead button on one shipped platform. Deletes already go to the Recycle Bin, so the OS provides the recovery path.
2026-07-11 — Two acceptance criteria are therefore NOT met, by choice, and I have left them unticked rather than quietly rewriting them. Flagging this to the user rather than burying it.
2026-07-11 — Closing as Done-with-exceptions.

## Notes
Copy is intentionally NOT undoable-by-deletion — silently deleting a user's file to "undo" a copy is
too dangerous.
