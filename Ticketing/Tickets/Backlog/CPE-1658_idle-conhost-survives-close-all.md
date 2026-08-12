---
id: CPE-1658
title: An idle conhost.exe survives "Close all consoles"
type: bug
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-11
closed:
---

## Problem

Secondary finding from the independent UAT on PR #843 (CPE-1621), observed at the real process level.

With a real agent running under the session daemon, `POST /api/close-all` kills the agent and its shells
promptly — measured:

```
+0.42s  pid 6960   cmd.exe  (outer)            GONE
+0.47s  pid 8856   PING.EXE (the real agent)   GONE
+0.61s  pid 26920  conhost.exe --headless      GONE
+0.68s  pid 30336  cmd.exe  (inner)            GONE
```

But a second, **non-headless** `conhost.exe` (pid 19572 in that run) survived the full 15-second poll
window untouched.

It holds no PTY and runs nothing — an idle leftover console-host handle, not the agent — so it does **not**
reproduce CPE-1621's bug (no agent keeps running, and nothing is hidden from the user). It is incomplete
cleanup: one stray process per console left behind on every close-all, which will accumulate across a long
session.

## Acceptance criteria

- [ ] After a close-all, no `conhost.exe` attributable to the closed console survives — or, if one
      legitimately must (e.g. it is shared, or Windows owns its lifetime), that is documented in the code
      with the reason, and this ticket closes as won't-fix with that explanation recorded.
- [ ] Verified at the **process level** — a before/after PID table like the one above, not a unit test.
- [ ] The agent-killing behaviour PR #843 established does not regress: the real agent and its shells must
      still die within about a second.
- [ ] Nothing kills a process the app did not start. Scope the cleanup to this console's own child tree,
      never a blanket sweep by image name.

## Notes

- Source: independent UAT on PR #843, 2026-08-11, step 1-3 process-table evidence.
- Related: [[CPE-1621]] "Close all consoles" doesn't stop running agents.
- Low priority: a leaked idle handle, not a running agent, and not a lying UI.

## Work Log

- 2026-08-11 — Filed by the Foreman from the PR #843 UAT's secondary observation.
