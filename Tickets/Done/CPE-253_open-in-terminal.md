---
id: CPE-253
title: Open in Terminal here
type: Feature
status: Done
priority: Medium
component: Backend + Frontend
estimate: 1h
created: 2026-07-13
---

## Summary

A developer-focused explorer (this app reads git remotes and targets devs
watching AI agents) should let you drop into a terminal at the folder you are
looking at. Add **Open in Terminal** to the folder context menu and the
empty-area menu, launching the platform's terminal with its working directory
set to that folder.

## Acceptance Criteria

- [ ] New backend command `open_terminal(path)` opens the OS terminal at `path`:
      Windows → Windows Terminal (`wt`) with a new-cmd-window fallback; macOS →
      Terminal.app; Linux → `x-terminal-emulator`/`gnome-terminal`/`konsole`/`xterm`.
- [ ] Right-click a folder offers **Open in Terminal** (opens in that folder).
- [ ] Right-click empty space offers **Open in Terminal** (opens in the current folder).
- [ ] Unavailable in Home and inside the read-only archive view.
- [ ] `cargo build`/`clippy` and `npm run check` pass.

## Resolution

Added backend `open_terminal(path)` that launches the platform terminal with its
working directory set to `path`: Windows tries `wt.exe -d` then falls back to a
fresh cmd window (current_dir), macOS opens Terminal.app, Linux tries
x-terminal-emulator/gnome-terminal/konsole/xterm in turn. Wired **Open in
Terminal** into the folder context menu (opens the selected folder) and the
empty-area menu (opens the current folder); both hidden in Home and the
read-only archive view. Follows the existing `open_external` spawn style; no
unit test (side-effecting spawn), matching `open_external`.

## Work Log
2026-07-13 — Filed and picked up during Nightshift.
2026-07-13 — Implemented backend + frontend. Verified: cargo test 59 pass,
clippy clean (fixed a needless-return), npm run check + vitest 241 pass.
Landed on branch cpe-253-open-terminal.
