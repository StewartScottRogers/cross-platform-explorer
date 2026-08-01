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
closed: 2026-08-01
status: Done
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

## Work Log
2026-08-01 (workshift) — **Done.** Built `src-tauri/src/pty.rs`: `PtySession`/`PtyLaunch` mirror
`sidecar/ai-console/src/pty.rs` byte-for-byte in shape (spawn/reader/writer/resize/kill), plus a
`PtyRegistry` (id-keyed live sessions, Arc-around-Mutex, cheaply cloneable like `IndexService`). Home
crate: `src-tauri`, not `cpe-server` — a PTY session owns OS processes/handles and the streaming command
uses `ipc::Channel`, both app-adapter territory per CLAUDE.md. `portable-pty = "0.8"` added to
`src-tauri/Cargo.toml` (same version as the sidecar's, resolves to the same 0.8.1 in the lockfile — no
drift).

Commands (thin dispatchers): `open_pty` (async + spawn_blocking for the OS spawn; starts a raw
`std::thread` reader pump — not another `spawn_blocking`, since it's unbounded for the session's life —
streaming base64-encoded output over `on_output: ipc::Channel<String>`, mirroring the sidecar's own PTY
wire format), `write_pty` (async + spawn_blocking), `resize_pty` (sync — a quick ioctl/WinAPI call, no
subprocess I/O), `close_pty` (async + spawn_blocking). The reader thread also self-cleans the registry
entry on EOF (child exits on its own, e.g. `exit`), so a session never lingers as "live" even if the
frontend never calls `close_pty` — the DoD's "no PTY/background cost when nothing's open" holds whether
the panel closes cleanly or the shell just ends.

Wired `terminal_tabs::TerminalDock` (CPE-947) with a new `TerminalDockState` wrapper (in `cpe-server`,
mirrors `IndexService`'s shape) + 6 thin `terminal_dock_*` commands (open/close/activate/set_cwd/tabs/
active) — kept deliberately decoupled from the PTY registry (a tab is bookkeeping; a PTY session is a
process), the two correlate by id from the frontend side (CPE-1243).

Tests: 6 new `pty::tests` in `src-tauri` (spawn+stream echo, cwd+env applied, write reaches an
interactive shell + its echo streams back, resize+kill reaps the child — polled via `try_wait` until the
process is gone, registry open/write/resize/close round-trip, unknown-id errors) — all REAL PTY spawns,
none mocked. 2 new `TerminalDockState` tests in `cpe-server`. `cargo test` green in both crates (108
src-tauri tests incl. the 6 new + existing 102; cpe-server's existing suite + 2 new, all passing).
`cargo clippy --all-targets -- -D warnings` clean for `src-tauri` (default + `--features
sidecar-platform`) and `cpe-server` (default + `--features "index specta"`). Regenerated
`src/lib/bindings.gen.ts` (new `TermTab` export + the 10 new commands) via `cargo run --bin
export_bindings --features "specta-bindings sidecar-platform"` — required since `TermTab` is now reached
by a command return type for the first time. `Cargo.lock` committed for the `portable-pty` addition; no
`os error 225` seen locally (that's Defender quarantine, not a real failure, per the ticket's note) —
CI's 3-OS matrix is still the first cross-platform validation. Scope held to backend only — no xterm.js/
UI (that's CPE-1243, unblocked by this).
