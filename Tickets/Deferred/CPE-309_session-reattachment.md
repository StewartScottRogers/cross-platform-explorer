---
id: CPE-309
title: Session reattachment across sidecar restart
type: Feature
status: Deferred
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

- [~] A running agent session survives a sidecar restart, or fails gracefully with
      the transcript preserved — never a silent kill of the user's work. *(Graceful half done via
      CPE-370. Live survival: engine + supervision built + tested but NOT shipped by default —
      detached-ConPTY yields no output on Windows; needs host-owned non-detached daemon. See the
      2026-07-15 regression note.)*
- [~] Reattach restores live I/O to the console UI; state is reconciled ([[CPE-299]]). *(Mechanism
      built + automated-tested via `DaemonEngine`; not the production default yet.)*
- [x] Interaction with resource budgets ([[CPE-297]]) and reaping is defined.
- [~] Tested: restart the sidecar mid-session, assert the session is recoverable. *(Automated at the
      process boundary with a non-detached daemon; the product path awaits host-owned supervision.)*

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

2026-07-15 — **Slice 3a landed: the daemon CLIENT** (`sidecar/ai-console/src/session_client.rs`) —
the counterpart to the slice-2 server and the exact API the `console.rs` swap will proxy through:
`SessionClient::{connect,launch,attach,input,resize,kill,list}`. One background thread demuxes the
socket — session I/O (`replay`/`output`/`exit`) is routed to the matching `attach` stream; control
acks (`launched`/`ok`/`error`/`sessions`) go to a control channel the request methods wait on. Fixed a
latent leak found while building it: the reader now holds a `Weak<Inner>` and `Inner::drop` shuts the
socket down, so a dropped client actually closes its connection/thread (important once the console
opens/closes these per restart). **3 tests** incl. `a_second_client_reattaches_and_gets_replay_plus_
live_output` — a *second* `SessionClient` reconnects after the first drops and recovers replay + live
output, the client-side reattach. Protocol is now complete + tested on **both ends**.
`cargo test -p ai-console` **147 + 7 + 1** green; clippy `--all-targets` clean.

2026-07-15 — Assessed the remaining S3 rewire before touching it: pointing `console.rs` at the daemon
is invasive because it also entangles **history recording (CPE-370)** and the **read tap (CPE-405)**
(both currently live in the in-process reader thread and must move to the client attach pump), and it
only pays off once the daemon is a **supervised separate process (S4)** — so S3-completion + S4 are
coupled and best done together, carefully, against the working 147-test session subsystem. Deferred
as the final slice rather than rushed. Foundation (engine + server + client, all tested) is in place.

## Design decided + first slice landed 2026-07-15 (dayshift)
**Design call made** (`docs/design/CPE-309-session-reattach.md`): PTY ownership moves out of the console UI process into the long-lived `--session-daemon` process; the console becomes a `SessionClient` of it, routes all session ops through it, and on restart reconnects + `list`s + reattaches (the daemon replays each ring). The daemon-owns-PTYs model sidesteps Windows ConPTY re-parenting (the daemon never dies on a console restart).

**Built + tested (engine/transport):** `SessionDaemon`, `session_server`, `SessionClient`, the `--session-daemon` process mode — all unit-tested. **New this slice:** `session_supervisor::SessionDaemonHandle` — spawns/owns the daemon child, reads its `PORT`, hands out clients, reaps on drop; an integration test spawns the REAL process, connects a client, lists, and reaps.

**Remaining (kept open, now `ready`):** route `ConsoleState`'s `handle_launch`/`ws_route`/input/resize/close_*/`/api/sessions` through the `SessionClient` instead of owning in-process `PtySession`s; reattach on console boot; supervise+restart. **Verification is a multi-process runtime test** (launch → kill the console, not the daemon → new console reattaches with scrollback) — the honest gate before closing.

2026-07-15 (user: "do CPE-309") — **Landed the automated cross-process reattach proof** and reassessed
closure. Added `tests/session_reattach_across_restart.rs`: it spawns the **real** `--session-daemon`
process, has client A launch a session and read its first output, **drops client A** (the launching
console dying — crash/update/toggle), then a **new** client B reconnects to the same daemon process,
`list`s the session (still alive), re-attaches, and recovers the **scrollback replay (READY) + live
output (TICK)**. This is the automated form of AC4's core claim — "restart the sidecar mid-session,
session recoverable" — proven against a genuinely separate OS process, not an in-process served daemon
(the existing `session_daemon_process.rs` only proved boot/bind/list-empty). **Ran locally: 1 passed
in 4.98s; `cargo clippy --all-targets -D warnings` clean.** Purely additive — no source touched, so the
147-test session subsystem is unchanged.

**Honest closure assessment — Deferred (not Done).** The reattach *mechanism* (engine + server + client
+ supervisor + this real-process reattach test) is complete and verifiable headlessly. But the four
ACs are **product-level** and the remaining tail is coupled + gated on a live run I cannot do headlessly:
  - **S3 (console rewire)** — route `ConsoleState`'s launch/ws/input/resize/close through the
    `SessionClient`, moving history recording (CPE-370) + the read-tap (CPE-405) into the attach pump.
    Invasive to a working 2228-line, 147-test file. On its own it delivers **no** new user-visible
    capability (a UI reopen already reattaches in-process via CPE-461), so it is net-negative unless
    landed **with** S4 — which is why the prior sessions deferred them together.
  - **S4 (host supervision)** — the daemon must outlive the **sidecar process** (discover-or-spawn +
    no-reap). On Windows this collides with the host's job-object that kills the sidecar's process
    tree on exit; whether the daemon actually survives a host-driven sidecar swap can only be
    confirmed by a **real host run**.
  - **AC4 product gate** — a human GUI walkthrough: launch an agent, kill the sidecar *process*, reopen,
    confirm the tab reattaches with scrollback. Not performable headlessly here.

**Deferred-on:** internal prereq — S3 + S4 must be built together, and their honest verification is a
live multi-process / host GUI run unavailable in this headless environment. Doing S3 alone would risk
the session subsystem for zero user-visible gain. **Revisit-when:** picking up the console rewire +
host daemon-supervision as one careful change, paired with a real GUI restart-survival run. The
mechanism they build on is now fully proven (this ticket's test) — the tail is integration + a run,
not new mechanism.

## S3 + S4 implemented 2026-07-15 (user: "do S3+S4 now")
The console-side integration + host-side supervision are built and CI/locally verified. Remaining is
the one manual GUI restart-check the user agreed to run.

**S3 — console routes through the daemon (backend seam).** New `session_engine.rs`: a `SessionEngine`
+ `SessionIo` seam abstracts *where a session's PTY lives*. `LocalEngine` (in-process, the historical
behaviour, kept as the default so the 183-test session subsystem is untouched) and `DaemonEngine`
(PTY in the daemon, via `SessionClient`). `console.rs` no longer spawns `PtySession` directly:
`handle_launch` → `engine.launch()`, and a shared `adopt_session()` runs the reader pipeline
(ring + live fan-out + read-tap CPE-405 + usage CPE-311, and on stream-close history CPE-370 +
`ended` announce) off the engine's output channel. `ws_route`/close/close_all/resize go through the
`SessionIo`. History + read-tap + usage moved into the shared pump exactly as planned.

**S4 — the daemon outlives the sidecar.** `SessionDaemonHandle::discover_or_spawn(exe, port_file)`:
reconnect to an already-running daemon (recorded in a temp port-file) across a console restart, else
spawn a **detached** daemon (`CREATE_BREAKAWAY_FROM_JOB|DETACHED_PROCESS|NEW_PROCESS_GROUP` on
Windows; `setsid` on Unix) and record its port. A *discovered* daemon is never reaped on drop (it must
survive); Rust doesn't kill a child on drop, so a console **crash/hard-kill** leaves the detached
daemon running for the next console to rediscover. `main.rs` wires `DaemonEngine` in production with a
graceful fallback to in-process if the daemon can't start (never blocks a launch;
`CPE_AICONSOLE_NO_DAEMON` forces in-process), and calls `reattach_running_sessions()` on boot to
re-open a tab per surviving session (scrollback + live I/O restored).

**Verified headlessly (local + CI):**
- `tests/session_engine_daemon.rs` — the `DaemonEngine` launches in the REAL daemon process, lists the
  session as reattachable, and `attach` recovers replay (scrollback) + live output. **1 passed.**
- `tests/session_reattach_across_restart.rs` — client dies, new client reattaches (the raw mechanism).
- `cargo test` **183 lib + all integration green**; `cargo clippy --all-targets -D warnings` clean.
- The 183-test session subsystem is unchanged (LocalEngine default).

## Remaining — the ONE manual GUI sign-off (user)
The only thing not confirmable headlessly is whether the **detached daemon survives a host-driven
sidecar-process kill on Windows** (the job-object breakaway question). To close:
1. Install the build carrying this change, open the AI Console, launch an agent, let it print output.
2. Kill the **sidecar process** (Task Manager → the ai-console/session-daemon process for the UI), or
   trigger an app update, so the console process dies.
3. Reopen the AI Console → the agent's tab should return with its scrollback and keep streaming.

If the tab comes back live → close as Done. If the daemon was killed with the sidecar (job-object did
not break away), the fix is host-side: have the **host** (src-tauri) spawn/own the daemon so it is
never in the sidecar's job — a small follow-up, the engine/seam/supervision all stay as-is.

## Regression found + fixed 2026-07-15 (user: "no output on any tab … you fix")
Shipping the DaemonEngine as the **production default** (v0.19.0) broke the AI Console: every tab
showed "Session running" but **no output**. Root cause: the daemon is spawned **detached**
(`DETACHED_PROCESS`, no console) for job-object survival, but Windows **ConPTY** (portable-pty)
produces no output for children spawned in a process with no console — so the PTY ran but streamed
nothing. The automated `session_engine_daemon.rs` passed only because it spawns the daemon
**non-detached** (inheriting the test's console), which is exactly the gap a headless test couldn't
catch and the GUI run surfaced.

**Fix:** the DaemonEngine is now **opt-in** behind `CPE_AICONSOLE_SESSION_DAEMON`; the production
default reverts to the proven **in-process `LocalEngine`**, so the AI Console works again. Added a
headless regression guard `console::tests::a_launched_session_streams_output_into_its_replay_ring`
(launch via the default engine → assert output reaches the replay ring), so the "no output" break
can't recur silently. `cargo test --lib` 184 passed; clippy `--all-targets -D warnings` clean.

**Status:** the S3 seam + DaemonEngine + supervision are all built and tested, but daemon-survival is
**not shipped** (opt-in only) because detached-ConPTY yields no output. The correct path for the
survival feature is **host-owned, non-detached daemon supervision**: the Tauri host (src-tauri)
spawns the session daemon **with a console** (so ConPTY works) and outside the sidecar's job (so it
survives), then hands its address to the sidecar. That's a focused host-side follow-up — the
engine/seam/client/supervisor all stay as-is. Deferring on that internal prereq; the default app is
healthy in the meantime.
