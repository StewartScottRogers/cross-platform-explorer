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

## Work Log
2026-08-01 — Implemented end-to-end: `open_pty` extended with a `shell: Option<String>` param
(`pty::resolve_shell`, unit-tested) so the shell picker can override the OS default; bindings
regenerated. Added `src/lib/terminalClient.ts` (framework-agnostic PTY<->xterm wiring: `PtyBridge`
output/input/resize/close, `openTerminalTab`/`closeTerminalTab` pairing a `terminal_dock_*` tab with its
PTY, `followNavigation`+`cdCommand`, `shellChoicesFor`) with 18 unit tests, and
`src/lib/components/TerminalPanel.svelte` (xterm.js pane + tab strip reusing `.tab`/`.tab.active`, shell
picker, Follow-folder toggle, panel-close = every session closed) with 8 component tests. Wired into
App.svelte as a real docked row (not an overlay) between the explorer and the status bar, toggled via a
new CommandBar "Terminal" button; a new tab always opens rooted at `currentPath`. Follow navigation `cd`s
the active tab's live shell rather than respawning it (chosen behavior, noted in code). Added
`@xterm/xterm` 6.0.0 + `@xterm/addon-fit` 0.11.0 (exact-pinned) as the only new deps. Enabled xterm's
`screenReaderMode` (accessibility win + gives gui-smoke a stable DOM text signal independent of the
canvas renderer). Added `gui-smoke/specs/terminal-panel.smoke.ts`: opens the panel on the real built app,
types `echo <marker>` into the live PTY, asserts the echoed marker renders, closes the panel (asserts no
leaked PTY). `npm run check` (0 errors), `npm test` (1788 tests incl. new ones, all green), `cargo test`
(pty:: 9/9 incl. new `resolve_shell` tests), `cargo clippy --all-targets -D warnings` clean in both
feature modes, gui-smoke spec passing against a real `tauri build -- --no-bundle`. PR opened.
