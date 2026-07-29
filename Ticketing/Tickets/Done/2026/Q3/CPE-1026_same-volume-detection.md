---
id: CPE-1026
title: Same-volume detection (pure-ish model)
type: feature
component: Backend
priority: medium
tags: ready
epic: CPE-661
created: 2026-07-25
closed: 2026-07-25
status: Done
---

## Summary
Foundation for Universal Drag-and-Drop (CPE-661): decide whether two paths live on the **same volume**, so a
drag-drop can follow the OS convention (same volume → move, different volume → copy). New module
`crates/server/src/volume.rs`, declared `pub mod volume;` in `crates/server/src/lib.rs`.

`same_volume(a: &str, b: &str) -> bool`:
- **Unix:** compare `std::os::unix::fs::MetadataExt::dev()` of the two paths (same `st_dev` → same volume).
  If either `metadata()` fails, return `false` (can't confirm → treat as different, the safe copy default).
- **Windows:** compare the volume by path prefix — take each path's `std::path::Component::Prefix` (drive
  letter / UNC share), compare case-insensitively; equal prefix → same volume. (v1 limitation: distinct
  mount points on one drive read as same-volume; note it in the doc comment — a `GetVolumePathNameW`
  refinement is a later slice, deliberately no new dep now.)

## Acceptance Criteria
- [ ] `same_volume` returns true for two paths on the same drive/volume, false across drives; missing paths
      (unix) → false.
- [ ] Windows arm compares drive/UNC prefix case-insensitively (`C:\a` vs `c:\b` → true; `C:\a` vs `D:\b` →
      false) with no new dependency.
- [ ] Pure `std` only (no new deps); clippy clean both feature modes; unit tests for both cfg branches
      (unix via a tempdir vs `/`; windows via drive-letter prefixes).

## Notes
Do NOT touch `crates/server/src/links.rs` (a sibling worker is editing it). Only `volume.rs` + the one
`lib.rs` module line + this ticket's Work Log.

## Work Log
- 2026-07-25: Implemented `crates/server/src/volume.rs` (`same_volume(a, b) -> bool`): Unix arm compares
  `MetadataExt::dev()` via `metadata()`, missing-path → `false`; Windows arm compares each path's
  `Component::Prefix` case-insensitively (drive letter / UNC share), no filesystem access, no new deps.
  Declared `pub mod volume;` in `crates/server/src/lib.rs`. 4 unit tests (Windows cfg branch: same-drive
  case-insensitive, cross-drive false, UNC-share prefix compare, no-prefix/relative path never confirmed
  same-volume) all pass; `cargo clippy --all-targets` and `--all-targets --all-features` both clean
  (`-D warnings`). PR opened against `main`.
