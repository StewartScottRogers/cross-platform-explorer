---
id: CPE-309
title: Session reattachment across sidecar restart
type: Feature
status: Open
priority: Medium
component: Backend
tags: [big-design]
estimate: 3-4h
created: 2026-07-13
---

## Summary

A long-running agent session shouldn't die because the AI Console sidecar was
restarted (crash, update, or user toggle). Decide and implement how running PTY
sessions survive: either the PTY-owning process outlives the sidecar UI process and
re-attaches, or sessions checkpoint and resume cleanly.

## Acceptance Criteria

- [ ] A running agent session survives a sidecar restart, or fails gracefully with
      the transcript preserved — never a silent kill of the user's work.
- [ ] Reattach restores live I/O to the console UI; state is reconciled ([[CPE-299]]).
- [ ] Interaction with resource budgets ([[CPE-297]]) and reaping is defined.
- [ ] Tested: restart the sidecar mid-session, assert the session is recoverable.

## Notes — Dependencies / Schedule
**Depends on:** [[CPE-280]], [[CPE-265]]. **Phase:** C2/C3. **Epic:** [[CPE-261]].

## Work Log
2026-07-13 — Filed during epic-plan hardening.
2026-07-14 — Triage while working the backlog. Two findings: (1) **Live reattach of a *running*
agent across a sidecar restart is architecturally impossible as built** — the agent PTYs are
children of the sidecar process, so they die when it does. True live reattach would require
re-parenting the agent to a supervisor that outlives the UI process (a large change) — that is the
real `big-design` core of this ticket. (2) The *achievable* value — sessions + transcripts survive
a restart and are relaunchable — is deliverable now: `history.rs` (CPE-292) already implements the
persistence, but it was **never wired in**. Split that implementable slice to **CPE-370** (`ready`).
This ticket stays `big-design` for the live-reattach core; revisit if PTY re-parenting is pursued.
2026-07-14 — **CPE-370 landed**: sessions + redacted transcripts now survive a restart and are
relaunchable (agent/provider/model/cwd) from the launcher's "Recent sessions" panel. The graceful
half of this ticket's first acceptance criterion ("fails gracefully with the transcript preserved
— never a silent kill") is met. Remaining here: *live* reattach of a still-running agent (needs
PTY re-parenting) — the `big-design` core.

2026-07-14 — Picked up to build live reattach (user chose "implement now"). Confirmed the topology:
`ConsoleState` owns each `Session` (PTY + ring + live sender) **inside the ai-console process**, so
every agent PTY is a child of that process and dies when it restarts. `portable-pty` cannot
re-parent a PTY to a surviving process (ConPTY on Windows is process-bound; a Unix master fd closes
with its owner). So the only viable design is a **PTY-owning engine whose sessions persist
independently of any attached client**, later hosted in a separate long-lived process the UI
reconnects to.

2026-07-14 — **Slice 1 landed: the reattach ENGINE** — `sidecar/ai-console/src/session_daemon.rs`
(`SessionDaemon`). It owns PTY sessions decoupled from clients: a per-session reader thread pumps
output into a bounded replay ring (256 KiB) and fans out to all current subscribers, pruning any
whose receiver dropped (a disconnected/restarted UI). `attach(id)` returns the buffered scrollback
to replay **plus** a live receiver — so a *new* client re-attaches and resumes a still-running
agent. Also: multi-client fan-out, `input`/`resize`, `list`, `kill`, and `reap_exited` (keyed on
the child's real `try_wait` status, robust to ConPTY's missing-EOF quirk) — the latter **defines the
resource-budget/reaping interaction (AC3)**: the supervisor samples the daemon process's memory and
reaping exited sessions bounds it. **5 tests** (4 cross-platform + 1 Unix-only input echo), incl.
`a_session_survives_a_client_dropping_and_a_new_client_reattaches` which is the direct AC2/AC4
mechanism proof (drop client A → session keeps running → client B reattaches, gets the replay, then
live output). Verified: `cargo test -p ai-console` 132 passed / 0 failed; clippy `--all-targets` clean.

2026-07-14 — Returned to Backlog as `big-design`: the ENGINE is done + tested, but **end-to-end live
reattach is not yet wired**, so the ticket is not closed. Remaining slices (each a `ready` follow-up):
  - **S2 — daemon process:** run `SessionDaemon` in a separate, long-lived process behind a loopback
    socket (reuse the `http.rs` loopback + framing pattern); a control/stream protocol
    (launch/attach/input/resize/kill/list ↔ output/exit) with attach-replay.
  - **S3 — console server points at the daemon:** `console.rs` stops owning `PtySession` directly and
    proxies the frontend WebSocket ↔ the daemon; on UI restart it reconnects and replays (this is
    where AC2 becomes end-to-end).
  - **S4 — host supervision & lifecycle:** the host keeps the daemon alive across an ai-console
    UI-sidecar restart (crash/update/toggle), with budgets/reaping (CPE-297) wired to the daemon; a
    cross-process integration test: launch a session, kill+respawn the UI sidecar, assert live I/O
    resumes.

2026-07-14 — **Slice 2 landed: the daemon runs as its own process behind a loopback socket.** Added
`sidecar/ai-console/src/session_server.rs` — a newline-delimited JSON protocol over loopback TCP
around the slice-1 `SessionDaemon`: client ops `launch`/`attach`/`input`/`resize`/`kill`/`list`,
daemon events `launched`/`replay`/`output`/`exit`/`sessions`/`ok`/`error` (PTY bytes base64). On
`attach` it writes the buffered **replay** then streams live **output** on a per-connection pump
thread; the shared `SessionDaemon` means a session outlives any one connection. Added a
`--session-daemon [--port N]` process entry in `main.rs` (prints `PORT <n>` for a parent to read).
**Tests:** an in-process **socket** test proving the reattach over the wire — client A launches +
reads output, **disconnects**, client B **reconnects**, `list` shows the session still alive, and B
gets the **replay + live** output; an error-handling test; and a **cross-process** integration test
(`tests/session_daemon_process.rs`) spawning the real binary in daemon mode, reading its `PORT`, and
driving the protocol over a real socket. `cargo test -p ai-console` **144 + 2 + 1** green; clippy
`--all-targets` clean.

2026-07-14 — Still open (`big-design`): the reattach **mechanism** is now proven end-to-end at the
socket/process layer, but the **product** wiring remains — **S3** (point `console.rs` at the daemon so
the actual launcher UI reattaches) and **S4** (host keeps the daemon alive across a UI-sidecar
restart + budgets/reaping). Those two close AC1's "survives" half and AC2 end-to-end.
