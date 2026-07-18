---
id: CPE-668
title: Backend same-volume detection for drag copy-vs-move
type: feature
component: Backend
priority: medium
status: Open
tags: ready
created: 2026-07-18
epic: CPE-661
estimate: 1h
---

## Summary
Foundation for CPE-661's OS-convention drag rule (same-volume = move, cross-volume = copy). Add
`same_volume(a, b) -> bool`: on Windows compare the volume (drive letter / `GetVolumePathName`), on Unix
compare `st_dev` of each path (or its parent when the path itself doesn't exist yet). Best-effort — on
any error, return false so the caller falls back to copy (the safe choice).

## Acceptance Criteria
- [ ] `#[tauri::command] same_volume(a, b)` returns true iff both paths resolve to the same volume/device.
- [ ] Windows + Unix implementations; unreadable/among-missing paths degrade to `false` (→ copy).
- [ ] cargo-tested (same path → true; a path vs a clearly different root handled); clippy clean both modes.

## Work Log
