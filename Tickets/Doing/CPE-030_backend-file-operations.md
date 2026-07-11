---
id: CPE-030
title: Backend — create folder, rename, delete to recycle bin, copy, move
type: Feature
status: Open
priority: Critical
component: Backend
estimate: 3-4h
created: 2026-07-11
closed:
---

## Summary

The app is read-only. Add the mutating filesystem commands every file manager needs, with
**delete going to the Recycle Bin / Trash, not permanent deletion** — a file manager that
irreversibly destroys data on a mis-click is dangerous.

## Acceptance Criteria

- [ ] `create_dir`, `rename_entry`, `delete_to_trash`, `copy_entries`, `move_entries` commands
- [ ] Delete uses the OS recycle bin/trash (the `trash` crate), NOT `fs::remove_*`
- [ ] Recursive directory copy is supported
- [ ] Name collisions are reported, never silently overwritten
- [ ] Every command returns a clear error string on failure; partial failures are reported per-item
- [ ] Rust unit tests cover create/rename/copy/collision, run on Linux AND Windows CI

## Resolution
## Work Log
## Notes
Uses the `trash` crate. Safe to add now that CI compiles Rust on both Linux and Windows (CPE-028).
Permanent delete (Shift+Del) is deliberately a separate, explicitly-confirmed path.
