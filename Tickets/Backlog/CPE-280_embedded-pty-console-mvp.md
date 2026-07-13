---
id: CPE-280
title: Embedded PTY console — run an installed agent (MVP)
type: Feature
status: Open
priority: High
component: Multiple
estimate: 4h+
created: 2026-07-13
---

## Summary

The console itself: a real pseudo-terminal (e.g. `portable-pty`) driving an
already-installed native CLI, streamed to an xterm-style terminal in the sidecar's
UI, with its working directory set to the repo the explorer has open (via the
context capability). This is the AI Console MVP — a genuine `claude`/`aider`/`codex`
session running in-app.

## Acceptance Criteria

- [ ] Spawn a chosen installed agent in a PTY, cwd = current repo/folder.
- [ ] Full interactive TUI: streamed output, keyboard input, resize, signals.
- [ ] Clean start/stop; session ends without orphan processes.
- [ ] Works cross-platform (Windows ConPTY / Unix pty).
- [ ] No secrets echoed to logs; env from the vault ([[CPE-279]]) if a profile is
      chosen.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-278]], [[CPE-281]], [[CPE-267]]. **Phase:** C2 (Console MVP).
**Epic:** [[CPE-261]].

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
