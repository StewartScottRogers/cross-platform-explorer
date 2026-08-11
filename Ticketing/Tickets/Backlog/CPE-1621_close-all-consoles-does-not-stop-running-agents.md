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
