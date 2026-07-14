---
id: CPE-337
title: "Verify AI Console scrollbar vs. full-screen TUI agents (alt-screen)"
type: Task
status: Done
closed: 2026-07-13
priority: Medium
component: AI Console
created: 2026-07-13
---

## Summary

CPE-336 replaced the flaky native terminal scrollbar with a custom, draggable thumb synced
to xterm's scroll state. Open question left over from that work: full-screen TUI agents such
as **Claude Code** may switch the terminal to the **alternate screen buffer** (`ESC[?1049h`),
which has no xterm scrollback. If so, the custom scrollbar would be pointing at an empty
buffer while an agent is running — misleading UI, or a no-op.

This ticket resolves that question definitively and documents the outcome.

## Investigation

Added a backend probe (`probe_claude_altscreen`, an `#[ignore]`d test in
`sidecar/ai-console/src/console.rs`) that spawns the real Claude Code TUI over a PTY and
scans the first seconds of output for alt-screen / mouse-tracking control sequences.

Result (2026-07-13):
- `ALT_SCREEN = true` — output contains `ESC[?1049h` (enter alternate screen buffer).
- Claude Code also enables **full mouse tracking**: `ESC[?1000h ?1002h ?1003h ?1006h`.

## Outcome — no functional change needed

The existing frontend design is already correct:

1. In the alternate buffer xterm.js keeps `buffer.active.baseY === 0` (no scrollback). The
   `updateThumb` guard `if (b.baseY <= 0) hide` therefore **hides the custom thumb** the
   moment an agent takes over the screen, and `ws.onmessage → scheduleThumb` re-evaluates it
   on every data chunk so it hides promptly on the buffer switch.
2. Because the agent enables mouse tracking, xterm forwards the wheel to the agent as mouse
   events, so scrolling *inside* the agent's own view works — the app owns its scrolling, as
   it should.

So the custom scrollbar governs normal-buffer (shell/history) scrollback and correctly
disappears under a full-screen agent. No behavioral fix required.

## Changes
- Keep `probe_claude_altscreen` as an ignored, documented regression probe.
- Add a code comment at the `updateThumb` guard recording *why* `baseY <= 0` is the correct
  alt-screen handling, so a future reader doesn't "fix" it into showing a bogus thumb.

## Depends on: [[CPE-336]].

## Work Log
2026-07-13 — Ran the probe against the real Claude Code TUI: confirmed it uses the alternate
screen buffer and full mouse tracking. Verified by code inspection that the `baseY <= 0`
guard in `updateThumb` already hides the custom thumb under a TUI agent and that wheel events
pass through to the agent. Concluded no behavioral change is warranted; documented the guard
and retained the probe. Done.
