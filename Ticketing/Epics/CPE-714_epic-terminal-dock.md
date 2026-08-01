---
id: CPE-714
title: "EPIC: Terminal dock — embedded terminal panel"
type: Task
status: In Progress
priority: Medium
component: Multiple
tags: [epic]
estimate: 4h+
created: 2026-07-18
closed:
---

## Goal
An in-app terminal pane that always opens rooted at the current folder and stays in sync as you navigate,
turning CPE into a keyboard-and-mouse power tool.

## Why
Developers constantly switch between a file view and a shell in that folder. A docked terminal that tracks
the current path removes that friction. Extends today's `open_terminal` command from launch-external to
embedded.

## Rough scope (areas, not child tickets)
- A docked PTY panel (Windows ConPTY / Unix pty) with a terminal emulator frontend.
- "cd here" wiring so the terminal follows navigation (and optionally vice-versa).
- Shell selection (pwsh/cmd/bash/zsh) and per-OS defaults.
- Additive panel: closed by default, zero cost when not open.

## Open questions (resolve at activation)
- Terminal-emulator approach in the webview (xterm.js vs. alternative) and PTY bridging in Rust.
- Follow-navigation direction: file view drives terminal, both, or opt-in.
- Session persistence across tabs/windows.

## Definition of Done
- A terminal pane opens rooted at the current folder and can follow navigation.
- Shell selection works per OS; input/output/resize behave like a normal terminal.
- With the panel closed there is no PTY or background cost; core explorer unchanged.

## Work Log
2026-07-23 (dayshift) — **Activated.** First slice: **CPE-947** — `terminal_tabs::TerminalDock`: the pure
open/close/activate/rename tab model with active-tab fixup. Remaining: PTY/shell spawning, xterm rendering,
and the dock layout.

## Board hygiene 2026-07-29 — reverted In Progress → Proposed
Not actively being worked: all decomposed child tickets are Done. Remaining DoD is user-gated (GUI / model-key / cert / Mac) or a deferred cap. Reverted to **Proposed** so the epic queue honestly shows what's dormant vs active; re-activate with `/ticketing-epic activate` to resume (like CPE-703 was this session). **Remaining (DoD review 2026-07-30):** PTY/shell spawn + xterm render + dock layout unbuilt (only tab model).

## Activated 2026-08-01 (workshift, user said "you choose") — decomposition
User granted the epic pick + the xterm.js dependency decision ("you choose"). Grep-first TRUE state:
- `crates/server/src/terminal_tabs.rs` (`TerminalDock`: open/close/activate/rename/set_cwd/tabs,
  CPE-947) is BUILT but ORPHANED (no PTY, no UI). `ThumbnailImage`-style orphaned-engine pattern.
- PTY prior art: `sidecar/ai-console/src/pty.rs` — a full `PtySession` (spawn/master/resize/read/write)
  on **`portable-pty` 0.8, ALREADY a workspace dep** (sidecar/ai-console/Cargo.toml). Backend reuses it
  → no NEW backend dep. xterm.js is already referenced by the AI-console launcher (known frontend dep).

Decomposition (sequential — 1243 needs 1242's commands):
- **CPE-1242** — PTY backend session in cpe-server: mirror `sidecar/ai-console/src/pty.rs`'s PtySession
  (portable-pty) — spawn a shell at a cwd, stream output over `ipc::Channel`, write input, resize, close;
  thin Tauri commands driving it + the `terminal_tabs` dock model. Headless cargo-testable
  (spawn echo/shell, write, read back, resize, close-frees-PTY). Async + spawn_blocking (CPE-760/761).
- **CPE-1243** — Terminal dock UI: an xterm.js pane rendering the PTY stream, wired to the
  `terminal_tabs` dock (tabs open/close/activate/rename), open-at-current-folder + follow-navigation
  (`set_cwd`), per-OS shell selection, resize. Panel-closed = no PTY/background cost. gui-smoke + vitest.
