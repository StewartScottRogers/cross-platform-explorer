---
id: CPE-805
title: Backend drive_type classification command
type: feature
status: Done
priority: low
component: Backend
tags: ready
created: 2026-07-20
closed: 2026-07-20
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
- [x] Windows classifies the drive containing a path (system drive → `fixed`); unix returns a stable value.
- [x] Async; registered; unknown/edge inputs return `unknown` rather than erroring.
- [x] cargo-tested (the system drive classifies as expected on Windows CI; unix returns its fallback).

## Notes
Uses the `windows` crate `Win32_Storage_FileSystem::GetDriveTypeW`. Foundation for CPE-806.

## Work Log
- 2026-07-20 — Picked up. Estimate: 1h. Plan: add async `drive_type` + `drive_type_impl` (Windows
  `GetDriveTypeW`, unix `fixed` fallback), register, cargo-test, clippy.
- 2026-07-20 — Implemented `drive_type` (async, spawn_blocking) + `drive_type_impl`. Windows derives the
  `C:\` root from a drive-letter path then calls `GetDriveTypeW`.
- 2026-07-20 — Compile fix: the windows-crate 0.56 `DRIVE_*` constants aren't exported in our enabled
  feature set (unresolved imports). `GetDriveTypeW` returns a raw `u32`, so I match the stable documented
  DRIVE_* numeric values directly (2=removable, 3=fixed, 4=remote, 5=cdrom, 6=ram) with a comment — avoids
  the const-import issue and needs no extra crate features.
- 2026-07-20 — `cargo test --no-default-features drive_type` green; `cargo clippy --no-default-features
  --all-targets -- -D warnings` clean.

## Resolution
Added `drive_type(path) -> Result<String, String>` to `src-tauri/src/lib.rs`: an async command
(`spawn_blocking`) registered in `generate_handler!`, delegating to a `drive_type_impl`. On Windows it
derives the drive root (`C:\`) from a drive-letter path and calls `GetDriveTypeW`, mapping the raw return
to `fixed`/`removable`/`network`/`cdrom`/`ram`/`unknown`; on non-Windows it returns `fixed` as a
best-effort fallback (richer unix classification is a follow-up). Two cargo tests cover it — Windows
classifies the cwd's system drive as `fixed`, unix returns its fallback for `/`.

Tradeoff: matched the DRIVE_* return values by their stable numeric constants rather than importing named
`windows`-crate constants, because those constants aren't in our enabled feature set; the numbers are
fixed Win32 API values and are documented inline. Additive — `list_drives` is untouched. Foundation for
the drive-bay sidebar badging (CPE-806).
