# CPE-309 — Session reattachment across sidecar restart (design)

**Status:** Design decided (2026-07-15). Engine + transport built; the console-side integration is the
remaining implementation.

## Problem

An agent session (a PTY running a coding CLI) today lives **in-process** in the AI Console sidecar
(`ConsoleState.sessions`). If that process restarts — a crash, an app update that swaps the sidecar
binary, or the user toggling the mode off/on — every running agent dies. CPE-461 fixed the *UI-only*
close/reopen (the launcher iframe is destroyed but the sidecar keeps running, so it reattaches). This
ticket is the harder case: the **sidecar process itself** restarts.

## Decision

**Move PTY ownership out of the console UI process into a separate, long-lived session-daemon
process.** The console UI process becomes a *client* of the daemon:

1. On startup the console **ensures a session daemon is running** — it spawns the
   `--session-daemon` child (which binds `127.0.0.1:<port>` and prints `PORT <n>`) if one isn't
   already reachable, and connects a `SessionClient` to it.
2. All session operations — **launch / attach / input / resize / kill / close_all / list** — are
   **routed through the `SessionClient`** to the daemon, instead of `ConsoleState` owning `PtySession`s
   directly.
3. When the console UI process restarts, the daemon **keeps running** (separate process = separate
   lifetime), still holding every PTY + its replay ring. The new console instance reconnects,
   **`list`s** the daemon's sessions, and **reattaches** each — the daemon replays the scrollback ring,
   then streams live output. This is the CPE-461 reattach flow, now spanning a full process restart.

The daemon's lifetime is decoupled from the console: it survives a console restart, and is reaped only
on an explicit `close_all`/shutdown (or when orphaned and idle — see Risks).

## What's already built (engine + transport, unit-tested)

- **`SessionDaemon`** (`session_daemon.rs`) — owns PTY sessions independent of any client; `attach`
  returns the replay ring + a live receiver; survives a client dropping. Tests cover reattach after a
  client drop, two concurrent clients, input routing, reaping, and `close_all` (CPE-442).
- **`session_server`** (`session_server.rs`) — serves the daemon over a loopback socket, one JSON line
  per op (`launch`/`attach`/`input`/`resize`/`kill`/`close_all`/`list`), base64 PTY bytes. Tests cover
  a reconnecting client replaying + resuming, and error handling.
- **`SessionClient`** (`session_client.rs`) — the console→daemon client (`connect`/`launch`/`attach`/
  `input`/`resize`/`kill`/`list`).
- **`--session-daemon` process mode** (`main.rs`) — runs the daemon as its own process, binds a port,
  prints `PORT <n>` for a supervisor to read. An integration test spawns it and speaks the protocol.

## Remaining implementation (the console-side integration)

1. **Daemon supervisor** (this ticket's first landed slice): a `SessionDaemonHandle` that spawns the
   `--session-daemon` child, reads its `PORT`, connects a `SessionClient`, and reaps the child on drop
   — the building block the console uses to reach the daemon. *(Landed + tested against the real
   process.)*
2. **Route `ConsoleState` session ops through the daemon** — replace the in-process `PtySession`
   ownership in `handle_launch`/`ws_route`/`input`/`resize`/`close_*`/`/api/sessions` with calls to the
   `SessionClient`. The launcher-facing HTTP/WS surface is unchanged; only the backing store moves.
3. **Reattach on console startup** — on boot, `list` the daemon's sessions and expose them via
   `/api/sessions` (CPE-461) so the launcher restores tabs across a full sidecar restart.
4. **Supervise + restart** — if the daemon dies, the console restarts it (sessions in a dead daemon are
   genuinely gone; a restart re-establishes a clean daemon).

## Risks / open points

- **Orphaned daemons.** A daemon that outlives its console must not linger forever. Mitigation: the
  daemon exits when idle (no sessions) for a grace period, or the console `close_all` + shutdown reaps
  it. (The `close_all` op exists.)
- **Windows PTY re-parenting.** The PTY child is owned by the daemon process, so re-parenting isn't
  needed — the daemon never dies on a console restart. This is why the daemon-owns-PTYs model is chosen
  over trying to re-parent a PTY into a new process (which ConPTY makes hard).
- **Verification is a multi-process runtime test:** launch a session → kill the console process (not the
  daemon) → start a new console → confirm the session is listed + reattaches with its scrollback. This
  needs a real run; it can't be fully asserted in a single-process unit test (the engine/server/client
  pieces are unit-tested individually).

## Verification plan

- Unit: the supervisor (spawn → connect → list → reap) against the real `--session-daemon` process.
- Unit: the daemon/server/client reattach behaviour (already covered).
- Runtime (GUI): the full restart-survival flow above — the honest gate before this ticket closes.
