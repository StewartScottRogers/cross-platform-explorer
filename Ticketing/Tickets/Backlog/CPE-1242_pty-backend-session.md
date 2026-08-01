---
id: CPE-1242
title: "PTY backend session (spawn/stream/write/resize/close) for the terminal dock"
type: Task
priority: Medium
component: Multiple
tags: [ready]
estimate: 3h
created: 2026-08-01
epic: CPE-714
closed:
---

## Context
The terminal dock's tab model (`crates/server/src/terminal_tabs.rs`, CPE-947) is built but there's no
embedded PTY in the app. `sidecar/ai-console/src/pty.rs` already has a working `PtySession`
(spawn/master/resize/read/write) on `portable-pty` 0.8 (an existing workspace dep). Build the PTY
backend for the MAIN app by mirroring that proven pattern — no new backend dependency.

## GREP FIRST
- `sidecar/ai-console/src/pty.rs` — the `PtySession`/`PtyLaunch` pattern (spawn via `native_pty_system`
  + `CommandBuilder`, master read/write, `resize(PtySize)`, child kill on drop). MIRROR it.
- `crates/server/src/terminal_tabs.rs` — the dock/tab model to drive.
- How other streamed commands use `tauri::ipc::Channel` (STREAMING.md; e.g. `thumbnails_stream`,
  `list_dir_stream`) + the cancel/registry pattern.
- `sidecar/ai-console/Cargo.toml` (`portable-pty = "0.8"`) — add the same to the crate that hosts the
  new module (prefer `src-tauri` for the PTY host, or cpe-server if it fits the seam — decide + justify;
  reuse the SAME version, no version drift).

## Build
- A PTY session type (mirror pty.rs): spawn a shell (`$SHELL`/`ComSpec`/cmd.exe/powershell/bash per OS)
  at a given cwd, expose write(bytes), resize(rows,cols), and a reader that streams output.
- Thin Tauri commands (async + spawn_blocking per CPE-760/761 for the blocking read loop): open a PTY
  session (returns an id; streams output over an `ipc::Channel`), write input, resize, close. A
  session registry keyed by id (like the transfer/thumb cancel registries), removed on close, so a
  closed panel leaves NO live PTY/background cost.
- Wire the `terminal_tabs` `TerminalDock` model to track sessions (open/close/activate/set_cwd).
- If a specta::Type struct is added, regen `bindings.gen.ts`.

## Acceptance criteria
- Open a PTY at a cwd → run a command → its output streams back; write input reaches the shell; resize
  works; close terminates the child + frees the PTY (no leak).
- No PTY/background cost when no session is open.
- REAL cargo tests (mirror pty.rs's own tests): spawn a trivial command (e.g. `echo`/`cmd /c echo`),
  read its output, write to a shell + read the echo, resize, and confirm close reaps the child.
  `cargo test -p cpe-server` (or the hosting crate) + `cd src-tauri && cargo test` + clippy both modes.
- "os error 225" on cargo test = Defender quarantine (not a failure) — note it.

## Notes
Prereq for CPE-1243 (xterm.js UI). Reuse portable-pty (existing dep) — do NOT add a different PTY crate.
