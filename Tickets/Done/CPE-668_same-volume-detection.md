---
id: CPE-668
title: Backend same-volume detection for drag copy-vs-move
type: feature
component: Backend
priority: medium
status: Done
tags: ready
created: 2026-07-18
closed: 2026-07-18
epic: CPE-661
estimate: 1h
---

## Summary
Foundation for CPE-661's OS-convention drag rule (same-volume = move, cross-volume = copy). Add
`same_volume(a, b) -> bool`: on Windows compare the volume (drive letter / `GetVolumePathName`), on Unix
compare `st_dev` of each path (or its parent when the path itself doesn't exist yet). Best-effort — on
any error, return false so the caller falls back to copy (the safe choice).

## Acceptance Criteria
- [x] `#[tauri::command] same_volume(a, b)` returns true iff both paths resolve to the same volume/device.
- [x] Windows + Unix implementations; unreadable/among-missing paths degrade to `false` (→ copy).
- [x] cargo-tested (same path → true; a path vs a clearly different root handled); clippy clean both modes.

## Work Log
2026-07-18 (nightshift) — Picked up as first CPE-661 child. Estimate 1h.

## Resolution
Added `#[tauri::command] same_volume(a, b) -> bool` (src-tauri/src/lib.rs). On Windows it compares a
pure-string volume root — drive (`C:`) or UNC share (`\server\share`), case-insensitive — via
`windows_volume_root` (kept always-compiled + unit-tested on every OS, `#[cfg_attr(not(windows),
allow(dead_code))]`). On Unix it compares `st_dev`, falling back to the parent folder's device when a
path doesn't exist yet. Best-effort: any uncertainty returns `false` so the caller copies (never loses
the source). 2 cargo tests (drive/UNC parsing across OS; same-volume for sibling paths); 123 backend
tests pass; clippy clean both feature modes. Foundation for CPE-669's OS-convention copy/move rule.
