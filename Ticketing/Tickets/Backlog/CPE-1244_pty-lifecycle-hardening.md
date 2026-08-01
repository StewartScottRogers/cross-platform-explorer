---
id: CPE-1244
title: "PTY lifecycle hardening: app-quit session sweep + SIGKILL-fallback reap"
type: Task
priority: Medium
component: src-tauri
tags: [ready]
created: 2026-08-01
epic: CPE-714
closed:
---

## Context
Surfaced by the CPE-1242 review (both notes are pre-existing in the sidecar's `pty.rs` reference, not
regressions this PR introduced):
1. **No app-quit PTY sweep** — `PtySession` has no `Drop` that kills the child, and there's no
   `RunEvent::Exit` / `on_window_event` hook closing outstanding `PtyRegistry` sessions on app quit. A
   user quitting with terminal tabs still open would orphan those shell processes.
2. **SIGKILL-fallback not re-reaped** — portable-pty 0.8.1's `ChildKiller` sends SIGHUP, polls
   `try_wait` ~5×50ms, then escalates to SIGKILL WITHOUT a follow-up wait — so a child that ignores
   SIGHUP for >250ms could be SIGKILL'd but left a transient zombie until something else reaps it.

## Acceptance criteria
- On app quit (Tauri `RunEvent::Exit`), all live `PtyRegistry` sessions are closed (children killed) —
  no orphaned shells. Test/verify (e.g. an OS-level check like CPE-1242's tasklist approach).
- After a kill escalates to SIGKILL, the child is actually reaped (a final `wait`), leaving no zombie
  even for a SIGHUP-ignoring child. (May need a small wrapper around portable-pty's killer.)
- Consider whether the sidecar's own pty.rs should get the same hardening (note it; separate PR).

## Notes
Follow-up to CPE-1242 (#544). Not a blocker for the terminal dock's happy path (explicit close +
shell-exit both clean up); this covers the app-quit + hostile-child edges.
