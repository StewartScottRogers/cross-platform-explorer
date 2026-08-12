---
id: CPE-1621
title: "\"Close all consoles\" (sidebar/toolbar) doesn't stop running agents — only hides them"
type: Bug
status: Backlog
priority: High
component: Backend
tags: [ready]
created: 2026-08-11
---

## Why
Found writing the docs depth pass on `src/docs/04-ai-console.md` (Agent Deck, CPE-1619, epic CPE-1569) —
verifying the "per-session close vs close entirely" distinction against the real code before documenting
it, per the epic's verify-against-code rule. The main app's own code comment claims one thing; the code it
calls does something narrower.

`src/App.svelte:1289-1291`:
```
/** Close the Agent Deck entirely (all running agents) and clear the Agents leaves. The console
    process is reaped, so no per-session `ended` arrives — clear the leaves here (CPE-457). */
async function closeAllConsoles() {
```

This is the handler behind **"Close all consoles"** — reachable by right-clicking an **Agents** leaf in
the left sidebar, or the **Agent Deck** toolbar button. The comment claims it closes "all running agents."

## The gap
`closeAllConsoles()` does two things: `commands.sidecarStop("ai-console")`, then `clearAgentSessions()`
(a pure client-side store wipe, `src/lib/agentSessions.ts:76-80` — just `store.set([])`).

`sidecar_stop` (`src-tauri/src/lib.rs:7972-7979`), for `id == "ai-console"`:
```rust
*state.conn.lock()... = None;
*state.url.lock()... = None; // no reuse once stopped (CPE-464)
```
It only drops the host's connection/URL to the ai-console **UI sidecar** (the HTTP server the launcher
page talks to). It never touches `state.daemon` (`Mutex<Option<HostSessionDaemon>>`,
`src-tauri/src/lib.rs:8097`) — the **separate**, host-owned process that actually holds every agent's PTY.
That struct's own doc comment (`lib.rs:8096`) says it is `None` until first started, **"reaped when this
state drops (app exit)"** — i.e. by design it survives the UI sidecar being stopped. `Drop for
HostSessionDaemon` (`lib.rs:8107-8110`) is the only thing that kills its child process, and nothing in
`closeAllConsoles`'s path drops it.

Compare this to the **in-console** "Close all" button (`sidecar/ai-console/src/launcher.html:1276-1329`),
which confirms with *"Close all sessions? Any running agents will be terminated."* and then really does
terminate them via `/api/close-all`. So the app ships **two different "Close all" actions with two
different real effects** — one that actually stops agents, one that only hides them from the UI — and the
one that doesn't work is the one reachable from the main explorer window (sidebar / toolbar), which is
the more prominent, more likely-to-be-used entry point.

**User impact:** a user who right-clicks an Agents leaf (or the toolbar button) and picks "Close all
consoles," expecting exactly what the in-console button's confirm text promises, instead leaves every
agent process running — invisibly, with no UI surfacing it — until they either quit the whole app or
reopen the Agent Deck and close sessions individually or via its real Close all. For a coding agent with
file/shell access, an orphaned background process nobody knows is still running is a meaningful problem,
not just a resource leak.

## Fix
Make `closeAllConsoles()` (or `sidecar_stop`) actually stop the session daemon too — not just the UI
sidecar's connection — so the main-window "Close all consoles" genuinely matches its own doc comment and
the in-console button's promise. At minimum, either:
1. Have `sidecar_stop("ai-console")` also stop/drop `state.daemon` when closing entirely (distinguish this
   from whatever keeps the daemon alive across a UI-sidecar-only restart, if that distinction is
   intentional elsewhere), or
2. If the daemon must legitimately survive (e.g. some other code path depends on it staying up across
   sidecar restarts), change `closeAllConsoles`'s label/comment and confirm text to be honest about what it
   does, and add a real "and stop every running agent" action for parity with the in-console button.
Add a regression test that launches a session, calls the sidebar/toolbar "Close all consoles" path, and
asserts the daemon-owned process is actually gone (not just the UI connection).

**Conflict surface:** `src-tauri/src/lib.rs` (`sidecar_stop`, `HostSessionDaemon`/`AiConsoleState`),
`src/App.svelte` (`closeAllConsoles`), possibly `sidecar/ai-console/src/session_daemon.rs`.

## Acceptance criteria
- "Close all consoles" from the sidebar or the Agent Deck toolbar button either genuinely stops every
  running agent process, or its label/comment/confirm text are corrected to describe what it actually does
  (UI-only), with a working "stop everything" alternative available from the main window.
- A test verifies the chosen behavior at the process level, not just the client-side leaf list.
- `cargo test` / the relevant harness passes; `npm run check` passes.

## Notes
High priority: this is a "the button lies about what it does" bug with a real resource/process-leak
consequence, on a feature (running coding agents with filesystem access) where an invisible orphaned
process is a genuine surprise, not just cosmetic. Model: sonnet — needs the session-daemon lifecycle
understood before touching it, not purely mechanical.

## Work Log
2026-08-11 — Fixed by reusing the ALREADY-WORKING mechanism, not by touching `HostSessionDaemon`/
`AiConsoleState.daemon` at all. Root cause confirmed by reading `ConsoleState::close_all`
(`sidecar/ai-console/src/console.rs:878-888`, behind `POST /api/close-all`): it kills every session via
`s.io.kill()`, and `SessionIo` is a trait (`session_engine.rs`) implemented by BOTH `LocalIo` and
`DaemonIo` — the daemon-backed case routes `kill()` through `SessionClient::kill(id)` over the real
socket protocol into the SEPARATE session-daemon process. So `/api/close-all` on the console's own
loopback UI server was ALREADY capable of genuinely killing daemon-held sessions; the bug was purely that
nothing on the main-window path ever called it. `sidecar_stop("ai-console")` only ever dropped
`state.conn`/`state.url` (the UI sidecar's connection) — it never talked to the console's HTTP API or the
daemon at all.

**Fix**: added `sidecar_close_all_sessions` (`src-tauri/src/lib.rs:8141-8163`, feature `sidecar-platform`,
registered in both `generate_handler!` and `collect_commands!`) — a thin dispatcher that POSTs
`{url}/api/close-all`, mirroring `sidecar_close_session`'s existing pattern for the single-session close.
`closeAllConsoles()` (`src/App.svelte`, ~L1296-1332) now calls `commands.sidecarCloseAllSessions()`
**BEFORE** `commands.sidecarStop("ai-console")` — ordering matters: `sidecar_stop` nulls `state.url`, so
calling it first (the original bug's shape, if you squint) would make the close-all endpoint permanently
unreachable. Regenerated `src/lib/bindings.gen.ts` via `cargo run --bin export_bindings --features
"specta-bindings sidecar-platform"` (clean 21-line diff, exactly the new command).

**Design decisions (asked to decide-and-log, not ask):**
1. **Reap sessions individually via the existing endpoint, not the whole daemon process.** Rejected
   "kill `state.daemon`" (fix option 1 in the ticket) because the daemon is documented to intentionally
   survive a UI-sidecar restart (`AiConsoleState.daemon`'s own doc comment) — reaping the process itself
   would contradict that design and also require the host to speak the daemon's line protocol directly
   (no existing client in `src-tauri`), where reusing `/api/close-all` needed zero new wiring. The daemon
   process itself is left running (now empty) after "Close all consoles" — a deliberate, documented
   choice, not an oversight.
2. **What the user sees**: no change to the honest-signal shape already established by CPE-1626 — the
   Agents leaves stay visible until `closeAllConsoles` actually finishes (the `await` chain), so a slow or
   failed close-all no longer clears the UI before the real work is attempted (previously `sidecarStop`
   alone was near-instant and always "succeeded" client-side regardless of daemon state, which was
   itself part of the lie). A failed close-all is still swallowed to `console.debug` and the UI proceeds
   to clear anyway — same swallow-and-continue shape `sidecarStop`/`sidecarCloseSession` already use;
   changing that failure-surfacing behavior more broadly is out of this ticket's scope.
3. **Confirmation — added a UI confirm dialog, no backend `confirmed` boolean.** Before this fix, "Close
   all consoles" fired INSTANTLY on click with no confirm at all (verified by reading `AgentMenu.svelte`
   — `dispatch("confirm")` on click, no modal). Since the fix makes this action newly and genuinely
   destructive (it used to be an inert no-op on running agents), I added `confirmCloseAllConsoles()` +
   reused the app's existing `confirm`/`ConfirmDialog` pattern (same one `spaceDelete`/permanent-delete
   use), with wording that mirrors the in-console button's own warning ("Every running agent will be
   terminated…") — this is exactly the parity with the in-console button the ticket's acceptance
   criteria asks for. I deliberately did NOT add a backend `confirmed: bool` parameter the way
   `shred_paths` (CPE-1611) / `vault_create` (CPE-1630) do, for two reasons: (a) the per-session close
   path (`sidecar_close_session`) — which the ticket explicitly says must stay unchanged — performs the
   IDENTICAL kill operation with zero backend gate today, so adding one only to the batched path would be
   an inconsistent asymmetry, not a coherent safety boundary; (b) this action is reachable ONLY via a
   deliberate right-click + menu selection, never at launch or idly, so it doesn't collide with the
   user's standing "no modal permission popups" preference (that preference targets unprompted nagging,
   not a confirm following a destructive click the user just made). Per-session close
   (`closeOneConsole`) is untouched — still unconfirmed, still a single-target kill, exactly as before.

**Regression test — process level, not just client bookkeeping**: added
`close_all_reaches_a_session_daemon_backed_session_not_just_local_bookkeeping`
(`sidecar/ai-console/src/console.rs`, after `close_reclaims_sessions_via_the_routes_one_and_all`). Wires a
`ConsoleState` to a REAL `DaemonEngine` talking to a REAL session daemon (`session_server::serve` over an
actual `TcpListener`, same code the out-of-process daemon runs), launches a real `cmd /c ping`/`sh -c
sleep` child through it, then proves via a SECOND, independent `SessionClient` connection (bypassing
`ConsoleState` entirely) that the daemon's OWN session table — not just `ConsoleState.sessions` — is
empty after `POST /api/close-all`. `cargo test` (sidecar/ai-console, `--lib`): 382 passed, 0 failed, 2
ignored (pre-existing ignores, unrelated).

**Manual process-table evidence (the thing that actually matters for a High-priority "the button lies"
ticket)**: ran the REAL built `sidecar/ai-console/target/debug/ai-console.exe --session-daemon` as its
own OS process, launched a real `cmd /c ping -n 60 127.0.0.1` session through it over the real socket
protocol, and inspected the live process table (`Get-CimInstance Win32_Process`, walking the descendant
tree by `ParentProcessId`) at each stage:
- **Before any close**: 3 real processes present — `cmd.exe` (PID 20092, child of the daemon PID),
  `cmd.exe` (PID 7864), `PING.EXE` (PID 31764).
- **Simulating the pre-fix bug** (doing nothing to the daemon — exactly what `sidecar_stop` alone did):
  same 3 processes still present and running.
- **Applying the fix** (sending the daemon the `close_all` op — the exact op `/api/close-all` triggers via
  `ConsoleState::close_all` → `DaemonIo::kill` → `SessionClient::kill`): all 3 processes GONE (0
  descendants) on the very next check.
- Daemon process itself cleaned up at the end; confirmed gone.
This is real OS process-table state, not a read of the code — the pre-fix branch really does leave a live
agent process running, and the fix really does reap it.

**Docs**: corrected `src/docs/04-ai-console.md`'s "Limits / notes" section (previously written by the
CPE-1619 depth pass to accurately describe the THEN-broken behavior, per that epic's verify-against-code
rule) — it now says "Close all consoles" genuinely terminates every agent and matches the in-console
button, and documents the new confirm dialog. Also updated the one-line summary in the sidebar
right-click menu list. No new user-facing *section* was added (this is an edit to an existing page), so
no `sectionDocs.ts` entry is needed per CLAUDE.md.

**Verification**: `npm run check` (0 errors/0 warnings); full `npx vitest run` (289 files, 3661 tests, all
green — includes the updated `App.agentWatchPauseMetrics.test.ts` "CPE-1626 loss path 1" test, which now
clicks through the new confirm dialog); `cargo clippy --all-targets -- -D warnings` for `src-tauri` in
BOTH default and `--features sidecar-platform` modes, clean; `cargo clippy --all-targets -- -D warnings`
for `sidecar/ai-console`, clean; `cargo build --features sidecar-platform` clean.

**Left out / not done**: did not touch `HostSessionDaemon`'s lifecycle or add any host-side daemon RPC
client (deliberately unnecessary, see design decision 1). Did not change `sidecar_close_session`'s
behavior, signature, or confirmation. Did not add a backend `confirmed` gate (see design decision 3).
