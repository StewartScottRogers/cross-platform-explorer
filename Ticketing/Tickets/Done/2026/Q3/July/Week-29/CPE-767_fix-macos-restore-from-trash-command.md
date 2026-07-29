---
id: CPE-767
title: Fix macOS backend compile break — restore_from_trash command missing on macOS
type: Bug
status: Done
priority: High
component: Backend
tags: ready
created: 2026-07-19
closed: 2026-07-19
estimate: 15m
---

## Summary
The backend fails to compile on **macOS** (`cargo check`): `cannot find macro
__cmd__restore_from_trash in this scope` at `src-tauri/src/lib.rs`. The `generate_handler![]` macro
references `restore_from_trash` unconditionally, but that `#[tauri::command]` is only defined under
`#[cfg(any(target_os = "windows", target_os = "linux"))]`. On macOS the `#[tauri::command]` attribute was
mistakenly placed on `restore_from_trash_impl` (the private helper), so no `restore_from_trash` command
exists there and the handler reference can't resolve. Linux and Windows compile fine — this is a
platform-specific break the local Windows-only build never sees.

## Environment
- OS: macОС (CI `macos-latest`) — Linux/Windows unaffected
- App version: 0.52.0
- Discovered: PR #15 CI (CPE-690), where the failing check was pre-existing on `main`

## Steps to Reproduce
1. `cargo check` (or CI Backend job) on macOS against `main` at/after the change that added
   `restore_from_trash`.

## Expected Behavior
Backend compiles on all three OSes; `restore_from_trash` is a real command on macOS that returns the
existing "not supported on this platform" error.

## Actual Behavior
macOS compile error: `cannot find macro __cmd__restore_from_trash in this scope` — build fails.

## Acceptance Criteria
- [x] macOS backend `cargo check` / clippy / test pass in CI.
- [x] Windows + Linux backend remain green (no regression).
- [x] `restore_from_trash` on macOS returns the same clear "not supported" `OpResult::err` as before.

## Resolution
Give macOS its own `#[tauri::command] async fn restore_from_trash` wrapper (mirroring the Windows/Linux
structure: async command → `spawn_blocking(restore_from_trash_impl)`), and demote the macOS
`restore_from_trash_impl` back to a plain private fn (drop the stray `#[tauri::command]`). Keeps every
filesystem command async + `spawn_blocking` per the async-all-commands convention.

## Work Log
2026-07-19 — Found via PR #15 CI (unrelated to CPE-690, which is frontend-only). Root cause: `#[tauri::command]`
sat on `restore_from_trash_impl` under the macOS cfg instead of on a `restore_from_trash` wrapper. Added the
macOS async command wrapper + plain impl. Windows/Linux paths unchanged. macOS verified by CI (no local Apple
toolchain).

2026-07-19 — **Closed.** PR #16 CI all green — `Backend (macos-latest)` passed (7m6s), the exact job that
was red; Windows/Linux backend + Frontend + all Sidecar jobs green, no regression. Squash-merged to main
(`be3b67b`). macOS build restored.

## Notes
Surfaced while merging CPE-690. Pre-existing on `main`, not introduced by that PR.
