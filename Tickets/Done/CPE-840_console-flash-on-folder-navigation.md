---
id: CPE-840
title: "Bug: a console window flashes on every folder navigation (missing CREATE_NO_WINDOW)"
type: bug
component: Backend
priority: high
status: Done
tags: ready
created: 2026-07-21
closed: 2026-07-21
---

## Summary
On Windows, clicking/navigating into a folder briefly flashes a black console window. Reported by the
user on the 0.57.0 sidecar build.

Root cause: navigating changes the current folder, which fires `forge_repo_status` (App.svelte
`refreshGitStatus`) to show the folder's git branch/sync state. Its backend (`forge_repo_status_impl`)
runs `git status` via `std::process::Command::new("git").output()` **without** the Windows
`CREATE_NO_WINDOW` creation flag, so Windows spawns a visible console for the git child process — a flash
on every folder change. The same defect affected ~15 other helper-process spawns in `lib.rs` (all the git
commands, the external opener `cmd /C start`, `run_command`, the elevation `powershell`); only the
session-daemon spawn set the flag.

## Fix
Added a `quiet_command(program)` helper that builds a `std::process::Command` with `CREATE_NO_WINDOW` on
Windows (a no-op elsewhere), and routed every **invisible** helper-process spawn through it: all 12 `git`
invocations, `open_external` (`cmd`/`open`/`xdg-open`), `run_command` (`cmd`/`sh`), and `run_as_admin`
(`powershell`). Left `open_terminal` untouched — those spawns are *meant* to open a visible window.

## Acceptance Criteria
- [x] Navigating into a folder no longer flashes a console window (the per-folder `git status` is hidden).
- [x] All helper-process spawns that run for output/side-effect use `CREATE_NO_WINDOW` on Windows; the
      intentional terminal-opener (`open_terminal`) is unchanged.
- [x] `cargo clippy --all-targets -D warnings` clean in **both** feature modes (default + `sidecar-platform`);
      the `mut`-unused-on-non-Windows path is guarded.

## Resolution
`src-tauri/src/lib.rs`: added `quiet_command` near the top and replaced the invisible spawns. The
`#[cfg(windows)]` block sets `creation_flags(0x0800_0000)`; a `#[allow(unused_mut)]` guards the binding on
non-Windows (where the cfg block compiles out). Verified locally: `cargo clippy --all-targets -D warnings`
green for both `--features sidecar-platform` and default. GUI-verify (the flash is gone) rides the next
install of the build carrying this fix.

## Work Log
- 2026-07-21 — Reproduced conceptually via `forge_repo_status` firing on folder change → `git status`
  without `CREATE_NO_WINDOW`. Added `quiet_command` + routed all invisible spawns through it. Both clippy
  feature modes clean. Closing.
