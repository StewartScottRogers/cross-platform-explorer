---
id: CPE-489
title: "Close a single agent session from the left-pane Agents context menu"
type: Feature
status: Open
priority: Medium
component: Frontend
tags: [ready]
estimate: 1-2h
created: 2026-07-16
epic: CPE-261
---

## Summary
The left-pane **Agents** section lists each running AI Console session as a leaf. Right-clicking a
leaf today opens a context menu (`AgentMenu.svelte`) whose only action closes **all** consoles
(`closeAllConsoles`) — and the leaf's right-click doesn't even pass *which* session was clicked
(`Sidebar.svelte` dispatches `agentMenu` with just `{x, y}`). Extend it so a user can close **just the
one session** they right-clicked, while keeping a "Close all" option.

## Acceptance Criteria
- [ ] Right-clicking a specific Agents leaf offers **"Close this session"** (labelled with the
      agent/session so it's clear which one), which closes only that session — the other running
      sessions and their tabs are untouched.
- [ ] The existing **"Close all"** action remains available (e.g. as a second item in the same menu,
      and/or the AI Console toolbar button's right-click, which already closes all).
- [ ] `Sidebar.svelte` passes the leaf's `sessionId` (+ agent name) in the `agentMenu` dispatch so the
      menu knows the target; `AgentMenu.svelte` supports more than one item.
- [ ] Closing one session ends that agent's PTY/tab in the AI Console and removes its left-pane leaf;
      the session list (`agentSessions`) updates reactively. If it was the last session, the behaviour
      matches "close all" (console can shut down).
- [ ] Headless test coverage for the Sidebar dispatch (target session id) and the menu's per-session
      action.

## Notes
Implementation surface: `Sidebar.svelte` (pass session id on right-click), `AgentMenu.svelte`
(multi-item menu), `App.svelte` (a `closeOneConsole(sessionId)` handler). Backend: today only
`closeAllConsoles` exists — closing one session needs a way to route "close session `<id>`" to the AI
Console (a host command + a console op / reuse the console's existing per-session close / daemon
`kill(id)`). Related to [[CPE-490]] (both enhance the same Agents leaves) and [[CPE-442]] (close-all).
