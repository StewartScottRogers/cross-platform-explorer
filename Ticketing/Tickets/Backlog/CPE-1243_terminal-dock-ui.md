---
id: CPE-1243
title: "Terminal dock UI: xterm.js pane + dock tabs + open-at-current-folder"
type: Task
priority: Medium
component: frontend
tags: [ready]
estimate: 4h
created: 2026-08-01
epic: CPE-714
closed:
prereq: CPE-1242
---

## Context
With the PTY backend (CPE-1242) in place, build the terminal dock UI: an xterm.js pane rendering the
PTY stream, wired to the `terminal_tabs` dock model, opening rooted at the current folder and following
navigation. xterm.js is already referenced by the AI-console launcher; add it as a frontend dep for the
main app (the standard terminal-emulator — user approved via "you choose").

## GREP FIRST
- CPE-1242's PTY commands + `ipc::Channel` output stream (how to subscribe/write/resize/close).
- `crates/server/src/terminal_tabs.rs` (dock/tab model) + its bindings.
- How the AI-console launcher uses xterm (`src/lib/ai-console-launcher*` / the launcher HTML) for a
  reference wiring; how the app docks panels (e.g. TransferPanel / a bottom panel) + tab strips
  (docs/design/TABS.md — reuse `.tab`/`.tab.active`).
- `invoke`/`rawInvoke`+createChannel from `src/lib/invoke.ts`; the current-folder path source (App.svelte).

## Build
- A dockable Terminal panel with an xterm.js terminal bound to a PTY session: output stream → xterm
  write; xterm input → PTY write; xterm resize (fit) → PTY resize.
- Dock tabs via `terminal_tabs` (open/close/activate/rename); a new terminal opens rooted at the CURRENT
  folder; option to follow navigation (`set_cwd`). Per-OS default shell + a shell picker.
- Panel closed → all PTY sessions closed, no background cost; core explorer unaffected when unused.
- Tab strip reuses the TABS.md convention; any pills reflow; theme vars only; dialogs (if any) get the
  visible border.

## Acceptance criteria
- A terminal pane opens at the current folder, streams I/O like a normal terminal, resizes correctly,
  supports multiple tabs, and closes cleanly (no leaked PTY).
- Follows navigation (opens/`cd`s to the current folder) per the DoD.
- `npm run check` + `npm test` (component/logic tests, non-hollow) + a gui-smoke render pin of the
  terminal pane showing real shell output. bindings regen if a specta struct changed.
- Add the xterm.js dep (justified — user-approved terminal emulator); pin a version, no other new deps.

## Notes
Prereq CPE-1242. The gui-smoke terminal render can be flaky (xterm canvas); assert a stable DOM signal
(a known echoed string in the xterm buffer/DOM) + snap. Live-terminal feel is user-gated for final polish.
