---
id: CPE-461
title: "Reattach AI Console tabs to running sessions on reopen"
type: Defect
status: Done
priority: High
component: Multiple
tags: [ready]
created: 2026-07-15
closed: 2026-07-15
epic: CPE-261
---

## Summary
Closing the AI Console and reopening it loses all its tabs, even though the agent sessions keep
running (they stay listed under "Agents" in the explorer left pane). The launcher must **reattach** a
tab to each still-running session on reopen — or the two views are inconsistent (agents alive in the
left pane, no tabs in the console).

## Root cause
The AI Console UI is an iframe that's destroyed on close; the launcher's tab state (the `sessions`
Map + tab DOM) dies with it. The sessions themselves live in the sidecar process (`ConsoleState.
sessions`) and survive. On reopen the iframe reloads `launcher.html` fresh with no tabs, and it had
no way to discover the still-running sessions.

## Acceptance Criteria
- [x] On reopen, the AI Console recreates a tab for each still-running session (reconnecting its
      WebSocket, which replays the scrollback ring), so no tabs are lost while agents run.
- [x] Tab labels match the original (agent · provider · model).
- [x] If nothing is running, the console opens with no tabs (no phantom tabs).
- [x] Tests: backend `/api/sessions` list + a jsdom boot-reattach test.

## Resolution
- **`console.rs`:** `Session` gains a `name` field (the tab label), set from a new `tabName` field the
  launcher sends on `/api/launch` (falls back to the agent name). New `GET /api/sessions` →
  `[{id, name}]` of the running sessions (sorted by sequential id = launch order). The sessions
  outlive the launcher UI, so this is the reattach source.
- **`launcher.html`:** `launchWith` sends `tabName`; `load()` (boot) calls `reattachSessions()` which
  fetches `/api/sessions` and `addSession(id, name)` for each running session not already tabbed —
  idempotent, so a catalog reload doesn't duplicate tabs. `addSession` reconnects the WebSocket, which
  replays the session's ring buffer (its scrollback).
- Tests: `/api/sessions` assertion in the console route test + 2 jsdom tests (reattach a running
  session on boot; no phantom tabs when none run). svelte-check 0, ai-console clippy clean, frontend
  suite 438 green. Final live reattach (real PTY replay) is GUI-verified.
