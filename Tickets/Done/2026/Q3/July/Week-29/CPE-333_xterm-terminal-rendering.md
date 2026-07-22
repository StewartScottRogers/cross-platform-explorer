---
id: CPE-333
title: "Render the console with a real terminal emulator (xterm.js)"
type: Bug
status: Done
closed: 2026-07-13
priority: High
component: Frontend
created: 2026-07-13
---

## Summary

A launched agent connects and works end-to-end (verified: Claude Code via OpenRouter,
typed input, agent responded), but the output is **garbled** — the launcher rendered PTY
output into a `<pre>` with naive ANSI-stripping, which can't display a full-screen TUI
(cursor moves, redraws, colours, box-drawing).

## Fix

Render with **xterm.js** (vendored, MIT, inlined into the sidecar's self-contained page so
it works under the sandboxed iframe). Feed it the exact PTY byte stream:

- Backend `handle_output` base64-encodes the raw slice (no lossy UTF-8); a new
  `/api/session/{id}/resize` forwards terminal size to the PTY.
- Launcher creates a `Terminal` + `FitAddon`, writes decoded bytes, sends `onData` to the
  input endpoint, and reports size on fit/resize.

## Acceptance
- A launched Claude Code session renders as a proper terminal (readable TUI, colours,
  redraws), accepts typing, and reflows on window resize.

## Work Log
2026-07-13 — Vendored xterm.js + fit addon (MIT) into src/vendor/, inlined into the
launcher page via launcher_html(). Backend: handle_output base64-encodes raw PTY bytes;
new /api/session/{id}/resize forwards size to the PTY (Session.pty). Launcher: xterm
Terminal + FitAddon, writes decoded bytes, onData->input, fit/resize->resize. 70 tests +
clippy green. Done.
