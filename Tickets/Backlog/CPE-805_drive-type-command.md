---
id: CPE-805
title: Backend drive_type classification command
type: feature
status: Open
priority: low
component: Backend
tags: ready
created: 2026-07-20
closed:
epic: CPE-716
estimate: 1h
---

## Summary
Foundation for the drive bay (epic CPE-716): a backend command classifying a drive's type, so the sidebar
(CPE-806) can badge removable/network/optical drives. Additive — doesn't change `list_drives`.

## Scope
- `drive_type(path) -> String` returning one of `fixed` / `removable` / `network` / `cdrom` / `ram` /
  `unknown`. Windows via `GetDriveTypeW` (windows crate, already a dep); unix returns `fixed` (best-effort
  for now — richer classification is a follow-up).
- Async + spawn_blocking; registered in `generate_handler!`.

## Acceptance Criteria
- [ ] Windows classifies the drive containing a path (system drive → `fixed`); unix returns a stable value.
- [ ] Async; registered; unknown/edge inputs return `unknown` rather than erroring.
- [ ] cargo-tested (the system drive classifies as expected on Windows CI; unix returns its fallback).

## Notes
Uses the `windows` crate `Win32_Storage_FileSystem::GetDriveTypeW`. Foundation for CPE-806.
