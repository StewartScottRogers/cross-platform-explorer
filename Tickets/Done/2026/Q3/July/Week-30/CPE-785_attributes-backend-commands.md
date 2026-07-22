---
id: CPE-785
title: Backend commands for permissions / attributes / timestamps
type: feature
status: Open
priority: medium
component: Backend
tags: needs-prereq
created: 2026-07-20
closed: 2026-07-20
epic: CPE-710
estimate: 2-3h
---

## Summary
Backend for the attributes editor (epic CPE-710): async Tauri commands to edit the platform's real model —
POSIX `set_permissions(path, mode)` (chmod), Windows attribute toggles (hidden/read-only/system/archive),
and `set_file_times(path, modified?, accessed?)` — each returning the prior state so the UI can offer undo.

## Acceptance Criteria
- [x] POSIX chmod sets the mode; Windows toggles set/clear the attribute; timestamps are set.
- [x] Each command is async (spawn_blocking) and returns the prior value for undo; errors cleanly.
- [x] cargo-tested where feasible (mode round-trip on unix; attribute round-trip on windows) on the CI matrix.

## Notes
Prereq: CPE-784 (mode model). Take-ownership via the existing run_as_admin. Wired through lib/invoke.

## Work Log
2026-07-20 (02:4x MST) — Picked up (prereq CPE-784 done). Estimate 2-3h (kept). Slice it: dep-free commands
first (chmod + read-only, std-only), then the dep-gated remainder (Windows attrs + timestamps).

2026-07-20 — **Slice 1 landed (dep-free).** Added two async Tauri commands in `lib.rs`:
- `set_permissions(path, mode) -> u32` — POSIX chmod (`#[cfg(unix)]` via `PermissionsExt`; returns the
  prior low-9-bit mode for undo). Non-unix returns a clear error (Windows uses attribute toggles).
- `set_readonly(path, bool) -> bool` — cross-platform read-only toggle via std, returns the prior state.
Both registered in `generate_handler!`. cargo test `set_readonly_toggles_and_returns_prior` (all OSes) +
`set_permissions_chmods_and_returns_prior_mode` (`#[cfg(unix)]`, mac/linux CI) pass; clippy
`--all-targets -D warnings` clean. No new deps.

**Remaining (needs a dependency decision):** the other Windows attributes (hidden/system/archive) need a
Windows-API crate (`windows`/`winapi`), and `set_file_times` needs the `filetime` crate — neither is a
current dep, and the repo favors pure-Rust/no-system-libs (`filetime` is pure-Rust and fits; `windows` is a
larger, Windows-only binding). Deferred pending the user's call on adding them; ticket stays In Progress.

2026-07-20 — **Slice 2 landed (user approved adding `filetime` + `windows`).** Added
`set_file_times(path, modified_ms?, accessed_ms?)` (cross-platform via pure-Rust `filetime`; each optional,
returns prior `(modified, accessed)` epoch-ms for undo) and `set_file_attribute(path, attr, value)` for
Windows hidden/system/archive via the `windows` crate `GetFileAttributesW`/`SetFileAttributesW` (returns
prior; non-Windows errors). Both registered. Deps matched the versions already in the tree (`filetime`
0.2, `windows` 0.56 under the existing `cfg(windows)` target) so no duplicate crates. cargo tests
(`set_file_times` all-OS; `set_file_attribute` windows-only; chmod unix-only; readonly all-OS) pass on
Windows; clippy `--all-targets -D warnings` clean; the 3-OS CI matrix covers the rest. **CPE-785 complete.**

## Resolution
Four async Tauri commands in `lib.rs`, each returning the prior value for undo: `set_permissions` (POSIX
chmod, unix), `set_readonly` (cross-platform), `set_file_times` (via `filetime`), `set_file_attribute`
(Windows hidden/system/archive via the `windows` crate). Deps added match the versions Tauri already pulls
in. Backend for the CPE-786 editor. cargo-tested per platform + clippy clean; 3-OS CI verifies compile +
tests across Windows/macOS/Linux.

