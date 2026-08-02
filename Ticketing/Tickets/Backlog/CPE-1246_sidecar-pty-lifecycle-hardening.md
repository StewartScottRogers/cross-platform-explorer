---
id: CPE-1246
title: "Apply the CPE-1244 PTY lifecycle hardening to sidecar/ai-console/src/pty.rs"
type: Task
priority: Low
component: sidecar
tags: [ready]
created: 2026-08-01
closed:
---

## Context
CPE-1244 hardened the MAIN app's `src-tauri/src/pty.rs` (app-quit `close_all` sweep + a `wait()` reap
after `kill()` so a SIGHUP-ignoring child isn't left a zombie after SIGKILL, + a belt-and-braces `Drop`).
The AI-console sidecar's `sidecar/ai-console/src/pty.rs` — the ORIGINAL pattern this was mirrored from —
has the identical two gaps (both the CPE-1242 worker and the CPE-1244 review/checker noted it). Apply the
same treatment there.

## Acceptance criteria
- `sidecar/ai-console/src/pty.rs`'s kill reaps after SIGKILL (a `wait()` after `kill()`), and the sidecar
  closes its live PTY sessions on its own shutdown path (mirror `close_all` + the exit hook).
- Real tests mirroring CPE-1244's (OS-level pid check: killed/swept child is actually gone, no zombie).
- No new deps.

## Notes
Straight port of CPE-1244 (#547) to the sidecar. Low priority (the sidecar is a separate process the OS
reaps on its own exit anyway; this is defense-in-depth + parity).
