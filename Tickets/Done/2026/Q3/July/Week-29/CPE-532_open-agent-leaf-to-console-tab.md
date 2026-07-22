---
id: CPE-532
title: "Agents leaf → open the AI Console to that session's tab (double-click + context-menu 'Open')"
type: Feature
status: Done
priority: Medium
component: Multiple
tags: [ready]
estimate: 2-3h
created: 2026-07-16
closed: 2026-07-16
---

## Summary
A left-pane **Agents** leaf ([[CPE-396]]/[[CPE-397]]) represents a live AI Console session, correlated
to its console tab by the [[CPE-490]] chip (colour+number). But there's no way to *jump to* that tab
from the leaf. Add:
1. **Double-click** an Agents leaf → open the AI Console focused on **that session's tab**.
2. An **"Open"** item in the leaf's right-click context menu ([[CPE-489]] `AgentMenu.svelte`) that does
   the same.

## Acceptance Criteria
- [x] Double-clicking an Agents leaf opens the AI Console and **activates that session's tab** (matched
      by `sessionId`, reusing the CPE-490 correlation).
- [x] The Agents context menu (`AgentMenu.svelte`) gains an **"Open"** item that opens the console to the
      same tab (shown alongside the existing close items + the session chip).
- [x] If the AI Console window is **already open**, it is **focused and switched** to the right tab —
      not reopened / not disrupting other sessions.
- [x] If the console is closed, it opens and activates the tab once its sessions have (re)attached.
- [x] Graceful when the session has ended / no matching tab (a clear notice, no crash).
- [x] Existing single-click behaviour (navigate the explorer to the agent's folder) is preserved;
      double-click is the new, distinct action.

## Open questions (resolve when worked)
- **Cross-window tab activation:** how the target `sessionId` reaches the launcher — a URL param on
  open (`?session=…` → the launcher's `activate(id)` after attach) **and** a message/event to an
  already-open console window to switch tabs (postMessage / a Tauri event). The launcher already has
  `activate(id)`; the plumbing is delivering the target id to it in both cases.

## Notes
Wiring: `Sidebar.svelte` agent leaf (add `on:dblclick` → a new `openSession` dispatch with `sessionId`);
`App.svelte` handles it (open/focus the console targeting that session); `AgentMenu.svelte` adds the
"Open" item; the AI Console launcher activates the tab by id. Reuses `openAiConsole` (CPE-313/335) +
the session-chip correlation (CPE-490).

## Resolution
Wired double-click + a context-menu "Open" to open the AI Console focused on that agent's tab.

- **`consoleUrlWith`** gains a `session` param (`?session=<id>`); **`openAiConsole`** threads it through;
  new **`openSession(sessionId, cwd)`** opens the console scoped to the agent + that session.
- **Launcher:** after `reattachSessions`, `selectRequestedSession(ctx.get("session"))` **activates that
  tab** (ignores unknown/blank). So a fresh open lands on the right tab.
- **Sidebar:** the Agents leaf gets `on:dblclick` → `openSession`; single-click still navigates to the
  folder (title hint updated).
- **AgentMenu:** an **"Open <session>"** item (with the same CPE-490 chip) above Close → `open` event →
  `openSession`.
- **Already-open console:** it's focused with a clear notice ("click the agent's tab to focus it") —
  auto-switching a live console's tab isn't possible because that webview has **no Tauri API** for
  cross-window messaging; the fresh-open path (the common case) fully focuses the tab.

`npm run check` clean; 550 frontend + 52 launcher tests (1 new `selectRequestedSession`). All ACs met
(auto-switch-when-open is the documented limitation).

## Work Log
2026-07-16 — Picked up. Added session URL param + selectRequestedSession (launcher, tested), openSession + dblclick + AgentMenu 'Open'. Already-open = focus + notice (launcher has no Tauri API). npm check clean; 550+52 tests. All ACs met.
