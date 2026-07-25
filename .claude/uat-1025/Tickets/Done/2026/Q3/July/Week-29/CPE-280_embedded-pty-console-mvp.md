---
id: CPE-280
title: Embedded PTY console — run an installed agent (MVP)
type: Feature
status: Done
priority: High
component: Multiple
estimate: 4h+
created: 2026-07-13
closed: 2026-07-13
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
**Depends on:** [[CPE-278]], [[CPE-281]], [[CPE-267]]; PTY approach proven in the
spike [[CPE-294]]. **Phase:** C2 (Console MVP). **Epic:** [[CPE-261]]. Runs the
agent scoped per [[CPE-306]]; survives restarts per [[CPE-309]].

## Resolution

Implemented `pty` in ai-console using `portable-pty` (Windows ConPTY / Unix openpty): `PtySession::spawn(PtyLaunch{program,args,cwd,env,rows,cols})` runs a native CLI in a real pseudo-terminal; `reader()`/`writer()` stream I/O, `resize()`, `is_running()`, `kill()`, `wait()`. cwd is set to the session repo and env is injected into the child only (secrets from the vault, never logged). 3 real-PTY tests (streams output, env injection, resize+liveness+kill), cross-platform via `shell_command`. Reads use a timed drain because a Windows ConPTY master may never signal EOF (documented in the test). 41 crate tests + clippy green; portable-pty builds in the cross-OS CI job.

**Deferred:** wiring the session's cwd/env from the live context+routing+vault, and the xterm frontend, land with the launcher UI ([[CPE-289]]) / UI mount ([[CPE-271]]); reattachment is [[CPE-309]].

## Work Log
2026-07-13 — Filed during Nightshift epic planning.
2026-07-13 — Implemented the real PTY console core during dayshift (fixed a ConPTY EOF hang with timed reads). Done.
