---
id: CPE-714
title: "EPIC: Terminal dock — embedded terminal panel"
type: Task
status: Proposed
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
