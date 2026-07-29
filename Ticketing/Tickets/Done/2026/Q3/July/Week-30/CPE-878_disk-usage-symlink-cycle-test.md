---
id: CPE-878
title: Regression test that disk-usage scan never follows symlinked dirs (anti-cycle)
type: chore
component: Server
priority: low
tags: ready
epic: CPE-706
created: 2026-07-21
closed: 2026-07-21
status: Done
---

## Summary
`disk_usage::dir_size_walk` must not follow symlinked directories (CPE-611) — otherwise a symlink pointing
back up its own tree sends the recursive (rayon) walk into an infinite loop: a hard production hang. That
guarantee had **no test**, so a refactor that dropped the guard would still pass every existing test while
re-introducing the hang.

Added a regression test that builds a symlink cycle (`loop -> parent`) plus a 500 KB file reachable only
*through* the symlink, and asserts the scan terminates and never re-counts the through-symlink bytes — in
both `dir_size` and `dir_children_sizes`. Sizes are asserted as a range, not an exact total, because a
symlink's own reported length is 0 on Windows but the target-path string length on Linux/macOS (the
cross-platform fs-byte-count trap). No behavior change; test-only.

## Acceptance Criteria
- [x] A symlink cycle does not hang the scan and its target bytes are never counted (verified end-to-end).
- [x] Portable on the 3-OS matrix — no exact byte-count assertions.
- [x] Skips cleanly on unprivileged Windows (symlink creation gated).
- [x] `cargo test` + `cargo clippy --all-targets -D warnings` green in `cpe-server`.

## Work Log
- 2026-07-21 (autonomous) — Audited `disk_usage.rs`: the anti-cycle guarantee holds via two mechanisms
  (`DirEntry::metadata()` not traversing symlinks, plus the explicit `entry_is_symlink` guard). Locked it
  in with a cycle + through-symlink big file. First cut asserted exact bytes and failed on the metadata
  semantics; rewrote as a portable range. 3/3 disk_usage tests pass; clippy clean.
