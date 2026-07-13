---
id: CPE-334
title: "Mount a real console: WebSocket streaming + WebGL + full terminal feature set"
type: Feature
status: Done
closed: 2026-07-13
priority: High
component: Multiple
created: 2026-07-13
---

## Summary

The terminal must be indistinguishable from a native console (the VS Code integrated
terminal bar) for arbitrarily long agent sessions. The console itself is already real (a
genuine ConPTY/pty running the real CLI); what was poor was the renderer + transport
(120 ms polling, a new TCP connection per keystroke, an unbounded server buffer). Replace
the transport with a true streaming pipe and bring xterm.js up to native quality.

## Scope

1. **WebSocket transport** — `GET /api/session/{id}/ws` upgrades to a WebSocket. The
   sidecar streams PTY output as binary frames the instant they're produced and pipes WS
   messages to PTY input. One persistent connection for the session (no polling, no
   connection churn). Handshake = sha1+base64 of the client key; frame codec hand-rolled
   (server frames unmasked, client frames unmasked, ping/pong/close handled) so we own the
   threading (writer thread + reader loop over a split stream).
2. **Bounded memory + reattach** — the PTY→ring reader keeps only a bounded tail
   (last ~512 KB) and broadcasts live chunks to the attached WS; on connect the ring is
   replayed then live-streamed. Memory stays flat over long sessions; reopening the pane
   rejoins the session with recent scrollback.
3. **Native-quality renderer** — xterm.js **WebGL** renderer + fit + **search** (Ctrl+F,
   vital for long sessions) + **web-links** + **unicode11** (correct widths). Deep
   scrollback (50k). Resize/reflow over the existing `/resize` endpoint.
4. **Copy/paste** — requires relaxing the console iframe to `allow-same-origin`
   (first-party, loopback-only page); documented in the threat model.

## Acceptance
- A long Claude Code session streams instantly, scrolls smoothly, supports select/copy/
  paste + Ctrl+F, reflows on resize, and never grows server memory without bound.
- Delete-test + other sidecars unaffected; ai-console + host tests green.

## Work Log
2026-07-13 — Implemented. Transport: hand-rolled WebSocket in http.rs (accept-key via
sha1+base64 — RFC 6455 vector test; frame read/write with masking — round-trip test);
serve() upgrades and hands the stream to console::ws_route, which replays the session's
bounded ring then streams live PTY output as binary frames and pipes input to the PTY.
Session model: PTY->bounded ring (512KB) + live subscriber; polling /output+/input removed
(resize stays HTTP). Renderer: xterm WebGL + fit + search (Ctrl+F) + web-links + unicode11,
50k scrollback, copy/paste (Ctrl+Shift+C/V). Iframe relaxed to allow-same-origin (frame
runs on its own loopback origin ≠ host, so isolation holds) — threat model updated.
ai-console 72 tests + host 77 + npm check 0/0, feature clippy green. Done.
