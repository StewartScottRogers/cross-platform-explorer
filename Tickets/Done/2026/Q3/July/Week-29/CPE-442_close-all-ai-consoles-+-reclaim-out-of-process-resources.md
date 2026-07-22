---
id: CPE-442
title: "Close all AI Consoles + reclaim out-of-process resources"
type: Feature
status: Done
priority: Medium
component: Multiple
tags: [ready]
estimate: 2-3h
created: 2026-07-15
closed: 2026-07-15
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
- [x] A single **"Close all consoles"** action closes every open session/pane at once (UI control in
      the AI Console; confirm before terminating in-flight sessions). — `closeAllSessions()` + a
      "Close all" button pinned to the tab strip, with a `window.confirm` guard.
- [x] Teardown reclaims **all** out-of-process resources with no orphans: session-daemon process(es),
      agent CLI child processes + PTYs (ConPTY drained, not wedged), and MCP server processes — via
      the existing supervisor kill/reap + MCP lifecycle. — `ConsoleState::close_all` kills every agent
      PTY (child + PTY reclaimed; the reader thread hits EOF → the normal history/`ended` end path),
      and `SessionDaemon::close_all` + a `close_all` socket op cover the daemon path. **MCP:** the
      lifecycle (`McpProcess` self-kills on `Drop`, `McpManager::stop_all`) is ready but MCP is not yet
      wired into the session path, so there are no console-owned MCP processes to reap today — the
      close-all hook is exactly where `stop_all()` attaches when MCP lands.
- [x] The same teardown runs on host `WillQuit` (graceful `sidecar.shutdown` → supervisor reap),
      so quitting the app leaves nothing behind. — `main.rs` calls `console.close_all()` in the
      `Reaction::Exit` arm (reached by `sidecar.shutdown` / `WillQuit`) before `process::exit`.
- [x] Idempotent + safe: "close all" with nothing open is a no-op; terminating a running agent uses
      its own kill domain (no host impact) and cleans up its PTY/port. — verified by tests (repeat
      close-all returns `[]`; each session is killed via its own `PtySession`, never the host).
- [x] Verified headlessly where possible: a test asserts that after teardown the supervisor reports
      zero live children (extend the session/supervisor tests); document the manual process-enumeration
      QA step for the GUI path. — see Resolution (4 Rust + 3 jsdom tests); manual QA noted below.

## Notes
- Builds on existing infrastructure: the `SessionDaemon`/`session_server`, the supervisor
  spawn/kill/reap with restart policy, ConPTY drain (avoids the known hang), and MCP lifecycle
  management. This ticket wires a **fan-out teardown** over them + a UI affordance, rather than new
  process machinery.
- Relates to CPE-441 (Full-output panel removal) only in that both touch the AI Console launcher UI;
  no dependency.
- Security: teardown must not leak secrets in logs on the kill path (`Redactor` already covers this).

## Resolution
Root cause found first: the launcher's `closeSession()` only dropped the WebSocket + removed the
pane — it **never told the backend to kill the PTY**, so every "closed" pane left its agent child
running in `ConsoleState.sessions`. That is the orphan the ticket is about. Fix, per layer:

- **`console.rs`** — `ConsoleState::close_session(id)` (remove from the live set + kill the PTY) and
  `close_all()` (fan-out: drain the map, kill each PTY, return sorted ids). Killing the child drives
  the session's existing reader thread to EOF, which runs the normal end path (history record +
  `ended` Agent-Watch announce), so teardown reuses the natural lifecycle — no new bookkeeping. New
  routes `POST /api/session/{id}/close` and `POST /api/close-all`.
- **`launcher.html`** — a "Close all" button pinned to the end of the tab strip (present only while
  sessions exist); `closeAllSessions()` confirms, `POST /api/close-all`, then tears down every pane;
  `closeSession()` now also `POST`s `/api/session/{id}/close` so closing one pane reclaims its agent.
- **`main.rs`** — hold the `Arc<ConsoleState>`; on `Reaction::Exit` (from `sidecar.shutdown` / host
  `WillQuit`) call `console.close_all()` before `process::exit`, so quitting leaves nothing behind.
- **`session_daemon.rs` / `session_server.rs`** — `SessionDaemon::close_all()` + a `close_all` socket
  op (`{"ev":"closed","ids":[…]}`), covering the CPE-309 daemon path with the same guarantee.

**Verification (headless):** 4 Rust tests — `SessionDaemon::close_all` (kills + clears + idempotent),
the socket `close_all` op (list empty after), and the `ConsoleState` routes end-to-end (launch 2 long
-running agents → close one via its route → close-all the rest → session set empty + idempotent);
plus 3 jsdom launcher tests (per-pane close POSTs the reclaim; "Close all" confirms + POSTs
`/api/close-all` + clears panes + drops the button; cancel does nothing). `cargo test` 149 pass,
`clippy --all-targets -D warnings` clean, `npm run check` 0 errors, launcher suite 17 pass.

**Tradeoffs / honest scope:** (1) MCP reclamation is a ready hook, not active wiring — MCP isn't in
the session path yet, so nothing MCP-owned runs to reap today; `close_all` is where `stop_all()`
attaches when it lands. (2) The `WillQuit → Exit → close_all` chain is unit-tested at the `close_all`
end; the full quit-the-app flow is a **manual QA** step. Manual QA: open ≥2 consoles, note the agent
PIDs (Task Manager / `Get-Process`), click **Close all** (and separately, quit the app) → confirm the
tabs clear and none of those PIDs remain.

## Work Log
2026-07-15 — Picked up. Estimate: 2-3h. Plan: add a session-daemon `close_all` op that kills+reaps every session (agent child + PTY), a UI "Close all" affordance in the launcher, and wire the daemon teardown to run on shutdown. Verify headlessly: a test that after close_all the daemon reports zero sessions.
2026-07-15 — Found the real leak: launcher `closeSession()` never killed the backend PTY. Implemented `ConsoleState::close_session`/`close_all` + routes, `SessionDaemon::close_all` + socket op, `main.rs` shutdown teardown, and the launcher "Close all" UI + per-pane backend reclaim. 4 Rust + 3 jsdom tests, all green; clippy + npm check clean. MCP noted as a ready-but-unwired hook; WillQuit end-to-end noted as manual QA. All ACs met.
