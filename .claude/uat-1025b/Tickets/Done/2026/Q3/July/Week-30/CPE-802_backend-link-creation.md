---
id: CPE-802
title: Backend link creation (symlink + hardlink)
type: feature
status: Done
priority: low
component: Backend
tags: ready
created: 2026-07-20
closed: 2026-07-20
epic: CPE-715
estimate: 1-2h
---

## Summary
Foundation for link forge (epic CPE-715). Async Tauri commands to create a symbolic link and a hardlink,
so the "New Link…" UI (CPE-803) is a thin call. Junctions and repair are a follow-up (CPE-804).

## Scope
- `create_symlink(target, link_path)` — unix `std::os::unix::fs::symlink`; Windows `symlink_dir` when the
  target is a directory else `symlink_file` (returns the OS error on failure so the UI can prompt for
  Developer Mode / elevation).
- `create_hard_link(target, link_path)` — `std::fs::hard_link` (cross-platform).
- Registered in `generate_handler!`; async + spawn_blocking.

## Acceptance Criteria
- [x] `create_hard_link` makes a working hardlink (same content); `create_symlink` makes a symlink to the target.
- [x] Failures return a clear OS error (esp. Windows symlink privilege); async (spawn_blocking).
- [x] cargo-tested (hardlink cross-platform; symlink on unix) on the CI matrix.

## Notes
Windows junctions deferred to CPE-804 (needs a reparse-point DeviceIoControl). Wired through lib/invoke by CPE-803.

## Resolution
Added `create_symlink(target, link_path)` (unix `symlink`; Windows `symlink_dir`/`symlink_file` with a
clear "needs Developer Mode/elevation" error) and `create_hard_link(target, link_path)` (`fs::hard_link`),
both async + spawn_blocking, registered in generate_handler. cargo tests: hardlink shares data
(cross-platform), symlink points at target (`#[cfg(unix)]` to avoid Windows-privilege CI flakiness; the
Windows path is compile-checked). clippy clean. Backend foundation for CPE-803 (New Link UI); junctions +
repair are CPE-804.

