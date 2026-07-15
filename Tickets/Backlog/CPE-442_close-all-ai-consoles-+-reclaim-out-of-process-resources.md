---
id: CPE-442
title: "Close all AI Consoles + reclaim out-of-process resources"
type: Feature
status: Open
priority: Medium
component: Multiple
tags: [ready]
estimate: 2-3h
created: 2026-07-15
---

## Summary
Give the user a single action that closes **every** open AI Console at once and guarantees all of the
console's **out-of-process resources** are reclaimed — the session-daemon process(es), spawned agent
CLI children + their PTYs, and any MCP server processes — so nothing is left running after the last
console is closed. Follow-up to the "how do I close consoles?" question: today a session can be
closed one pane at a time, but there's no "close all" and no explicit guarantee that the backing
processes are all reaped.

## Motivation
An AI Console session is more than a UI pane — it fronts a tree of OS processes (session daemon →
agent CLI under a PTY → optional MCP servers). Closing the pane must not leave orphans holding a PTY,
a port, or CPU. The user wants one deliberate "close everything and clean up" action, and the same
teardown must run on app quit.

## Acceptance Criteria
- [ ] A single **"Close all consoles"** action closes every open session/pane at once (UI control in
      the AI Console; confirm before terminating in-flight sessions).
- [ ] Teardown reclaims **all** out-of-process resources with no orphans: session-daemon process(es),
      agent CLI child processes + PTYs (ConPTY drained, not wedged), and MCP server processes — via
      the existing supervisor kill/reap + MCP lifecycle.
- [ ] The same teardown runs on host `WillQuit` (graceful `sidecar.shutdown` → supervisor reap),
      so quitting the app leaves nothing behind.
- [ ] Idempotent + safe: "close all" with nothing open is a no-op; terminating a running agent uses
      its own kill domain (no host impact) and cleans up its PTY/port.
- [ ] Verified headlessly where possible: a test asserts that after teardown the supervisor reports
      zero live children (extend the session/supervisor tests); document the manual process-enumeration
      QA step for the GUI path.

## Notes
- Builds on existing infrastructure: the `SessionDaemon`/`session_server`, the supervisor
  spawn/kill/reap with restart policy, ConPTY drain (avoids the known hang), and MCP lifecycle
  management. This ticket wires a **fan-out teardown** over them + a UI affordance, rather than new
  process machinery.
- Relates to CPE-441 (Full-output panel removal) only in that both touch the AI Console launcher UI;
  no dependency.
- Security: teardown must not leak secrets in logs on the kill path (`Redactor` already covers this).
