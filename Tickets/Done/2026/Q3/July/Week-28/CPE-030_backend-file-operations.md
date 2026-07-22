---
id: CPE-030
title: Backend — create folder, rename, delete to recycle bin, copy, move
type: Feature
status: Done
priority: Critical
component: Backend
estimate: 3-4h
created: 2026-07-11
closed: 2026-07-11
---

## Summary

The app is read-only. Add the mutating filesystem commands every file manager needs, with
**delete going to the Recycle Bin / Trash, not permanent deletion** — a file manager that
irreversibly destroys data on a mis-click is dangerous.

## Acceptance Criteria

- [x] `create_dir`, `rename_entry`, `delete_to_trash`, `copy_entries`, `move_entries` commands
- [x] Delete uses the OS recycle bin/trash (the `trash` crate), NOT `fs::remove_*`
- [x] Recursive directory copy is supported
- [x] Name collisions are reported, never silently overwritten
- [x] Every command returns a clear error string on failure; partial failures are reported per-item
- [x] Rust unit tests cover create/rename/copy/collision, run on Linux AND Windows CI

## Resolution

Added `create_dir`, `rename_entry`, `delete_to_trash`, `delete_permanent`, `copy_entries`,
`move_entries`, `move_exact`, `entry_info`, `dir_size` — with four safety rules enforced in the
backend, not just the UI:

1. **Delete goes to the Recycle Bin / Trash** (the `trash` crate), never `fs::remove_*`. Permanent
   delete is a separate command the UI only calls behind an explicit confirmation.
2. **Nothing is ever silently overwritten.** `create_dir` and `rename_entry` error on collision;
   paste auto-renames Explorer-style (`report - Copy.txt`, `report - Copy (2).txt`).
3. **A directory can never be copied or moved into itself or a descendant** — that recurses forever
   and shreds data. Checked via canonicalised paths, and it refuses when it cannot prove safety.
4. **Bulk operations report per-item results** instead of aborting on the first failure. If 9 of 10
   files copy and one is locked, the user is told exactly which one.

`move_entries` also falls back to copy-then-delete across filesystem boundaries (`fs::rename` fails
across volumes, e.g. C: -> Z:) — and only removes the source if the copy fully succeeded.

15 Rust unit tests, green on Linux, Windows and macOS CI.

## Work Log

2026-07-11 — Picked up. Designed around safety rules first, features second: a file manager that destroys data on a mis-click is worse than one that can't delete at all.
2026-07-11 — Trash-by-default; permanent delete is a separate, explicitly-confirmed command.
2026-07-11 — Added the self/descendant guard and the cross-volume move fallback (which never deletes the source on a failed copy).
2026-07-11 — 15 Rust tests. Green on all three OSes. Closing as Done.

## Notes
Uses the `trash` crate. Safe to add now that CI compiles Rust on both Linux and Windows (CPE-028).
Permanent delete (Shift+Del) is deliberately a separate, explicitly-confirmed path.
