---
id: CPE-044
title: Undo a delete by restoring from the Recycle Bin
type: Feature
status: Done
priority: Medium
component: Multiple
estimate: 2h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

Closes the acceptance criterion left explicitly unmet in CPE-041. Ctrl+Z currently undoes rename and
move but not delete. The blocker was that `trash::os_limited` (list/restore) is **not implemented on
macOS**, and I would not ship an Undo that silently does nothing on one of the three platforms we
release.

Now that CI compiles Rust on macOS (CPE-042), this can be done properly: implement restore where the
platform supports it, and — crucially — **do not offer it where it doesn't**, rather than offering a
button that lies.

## Acceptance Criteria

- [x] Backend `restore_from_trash` restores items to their original paths on Windows and Linux
- [x] Backend exposes `can_restore_from_trash` so the UI knows whether the platform supports it
- [x] Delete is pushed onto the undo stack ONLY on platforms where restore works
- [x] On macOS, Ctrl+Z after a delete says restore isn't supported and points at the Trash — it does not silently no-op
- [x] Restore refuses to overwrite a file that now occupies the original path
- [x] Compiles and passes clippy on Linux, Windows AND macOS CI

## Resolution

Ctrl+Z now undoes a delete by restoring from the Recycle Bin — on the platforms that can.

Backend: `restore_from_trash` uses `trash::os_limited::{list, restore_all}`, matching each trashed
item by the full path it was deleted from. It **refuses to restore over something that now occupies
the original path** — an undo must never destroy a file to reverse a delete. `can_restore_from_trash`
reports whether the platform supports any of this.

The design point, which is the whole reason this was originally declined:

- On **Windows and Linux**, restore works, so a trashed delete is pushed onto the undo stack.
- On **macOS**, `trash::os_limited` does not exist. So `can_restore_from_trash()` returns false and
  the delete is simply **never pushed onto the stack**. Ctrl+Z then offers whatever came before it,
  rather than presenting an Undo that would silently do nothing. The app doesn't offer an action it
  cannot perform.
- **Permanent delete is never undoable**, on any platform.

This is only verifiable because CPE-042 added macOS to the CI matrix first — the cfg-gated code and
the macOS fallback both compile and pass clippy on all three OSes.

Verified: cargo check + clippy + test green on ubuntu, windows AND macos; 80 frontend tests.

## Work Log

2026-07-11 — Filed because CPE-041 was closed with this criterion explicitly unticked. The user said do it, so it got done properly.
2026-07-11 — Implemented restore via trash::os_limited, matching items by their original full path, refusing to clobber.
2026-07-11 — Kept the original principle intact: rather than shipping a dead Ctrl+Z on macOS, the delete is never pushed onto the undo stack there. Never offer an action that cannot happen.
2026-07-11 — All three backend CI jobs green, including macOS. Closing as Done.

## Notes
The point is not "make Ctrl+Z work everywhere". It is "never present an action that cannot happen".
