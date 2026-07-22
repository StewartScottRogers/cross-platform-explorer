---
id: CPE-489
title: "Close a single agent session from the left-pane Agents context menu"
type: Feature
status: Done
priority: Medium
component: Frontend
tags: [ready]
estimate: 1-2h
created: 2026-07-16
closed: 2026-07-16
epic: CPE-261
---

## Summary
The left-pane **Agents** section lists each running AI Console session as a leaf. Right-clicking a
leaf today opens a context menu (`AgentMenu.svelte`) whose only action closes **all** consoles
(`closeAllConsoles`) — and the leaf's right-click doesn't even pass *which* session was clicked
(`Sidebar.svelte` dispatches `agentMenu` with just `{x, y}`). Extend it so a user can close **just the
one session** they right-clicked, while keeping a "Close all" option.

## Acceptance Criteria
- [x] Right-clicking a specific Agents leaf offers **"Close this session"** (labelled with the
      agent/session so it's clear which one), which closes only that session — the other running
      sessions and their tabs are untouched.
- [x] The existing **"Close all"** action remains available (e.g. as a second item in the same menu,
      and/or the AI Console toolbar button's right-click, which already closes all).
- [x] `Sidebar.svelte` passes the leaf's `sessionId` (+ agent name) in the `agentMenu` dispatch so the
      menu knows the target; `AgentMenu.svelte` supports more than one item.
- [x] Closing one session ends that agent's PTY/tab in the AI Console and removes its left-pane leaf;
      the session list (`agentSessions`) updates reactively. If it was the last session, the behaviour
      matches "close all" (console can shut down).
- [x] Headless test coverage for the Sidebar dispatch (target session id) and the menu's per-session
      action.

## Notes
Implementation surface: `Sidebar.svelte` (pass session id on right-click), `AgentMenu.svelte`
(multi-item menu), `App.svelte` (a `closeOneConsole(sessionId)` handler). Backend: today only
`closeAllConsoles` exists — closing one session needs a way to route "close session `<id>`" to the AI
Console (a host command + a console op / reuse the console's existing per-session close / daemon
`kill(id)`). Related to [[CPE-490]] (both enhance the same Agents leaves) and [[CPE-442]] (close-all).

## Resolution
Right-clicking a specific Agents leaf now offers **"Close <agent · model>"** (closes just that session)
alongside **"Close all consoles"**. `Sidebar.svelte` passes the leaf's `sessionId` + a human label in
the `agentMenu` dispatch; `AgentMenu.svelte` became a multi-item menu (`closeOne` event); `App.svelte`
gained `closeOneConsole(id)`. The new host command `sidecar_close_session` (src-tauri, feature-gated)
POSTs to the console's existing `/api/session/{id}/close` over its loopback URL — the console emits an
`ended` for that session, pruning its leaf while the others keep running (id is validated to a simple
token so it can't reshape the URL path). Headless tests cover the Sidebar dispatch + the menu's
per-session action. Files: `src/lib/components/{Sidebar,AgentMenu}.svelte`, `src/App.svelte`,
`src-tauri/src/lib.rs`. `npm run check` clean; tests + clippy green.
