---
id: CPE-044
title: Undo a delete by restoring from the Recycle Bin
type: Feature
status: Open
priority: Medium
component: Multiple
estimate: 2h
created: 2026-07-11
closed:
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

- [ ] Backend `restore_from_trash` restores items to their original paths on Windows and Linux
- [ ] Backend exposes `can_restore_from_trash` so the UI knows whether the platform supports it
- [ ] Delete is pushed onto the undo stack ONLY on platforms where restore works
- [ ] On macOS, Ctrl+Z after a delete says restore isn't supported and points at the Trash — it does not silently no-op
- [ ] Restore refuses to overwrite a file that now occupies the original path
- [ ] Compiles and passes clippy on Linux, Windows AND macOS CI

## Resolution
## Work Log
## Notes
The point is not "make Ctrl+Z work everywhere". It is "never present an action that cannot happen".
