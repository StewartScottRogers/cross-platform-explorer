---
id: CPE-457
title: "Close the AI Console from the explorer + keep Agents leaves in sync"
type: Feature
status: Done
priority: Medium
component: Multiple
tags: [ready]
created: 2026-07-15
closed: 2026-07-15
---

## Summary
Right-click affordances to close the AI Console from the explorer, and reliable Agent-Watch leaf
sync when a console/tab closes. Two user asks: (1) each Agents leaf gets a context menu that closes
the AI Console; (2) the AI Console toolbar button gets a right-click "Close all". Closing a console
or tab must remove the corresponding leaf under "Agents".

## Acceptance Criteria
- [x] Each Agents leaf has a right-click menu ("Close AI Console").
- [x] The AI Console toolbar button has a right-click menu ("Close all consoles").
- [x] Confirming stops the console and clears the Agents leaves.
- [x] Closing a session/tab from the launcher removes its leaf promptly (reliable `ended` announce).
- [x] Tests: AgentMenu component + the close-announces-`ended` backend path; suites green.

## Resolution
- **Reliable leaf sync (backend):** `ConsoleState::close_session`/`close_all` now announce a minimal
  `{"event":"ended","sessionId":id}` immediately after killing (new `announce_ended`), so a launcher
  tab-close (CPE-442) drops the explorer's Agents leaf at once rather than waiting for a Windows
  ConPTY EOF that may never come. Unit-tested with a recording announcer.
- **Explorer context menus (frontend):** new `AgentMenu.svelte` (mirrors TabMenu). An Agents leaf's
  `contextmenu` (Sidebar `agentMenu` event) and the AI Console button's `contextmenu` both open it —
  "Close AI Console" / "Close all consoles". Confirming runs `closeAllConsoles()` →
  `invoke("sidecar_stop","ai-console")` + `clearAgentSessions()` (the process is reaped, so the
  leaves are cleared here). New `clearAgentSessions()` store helper. AgentMenu component tests.

## Notes / honest scope
The explorer's menus close the **whole** AI Console (one sidecar hosting all agents) via the existing
`sidecar_stop`, not a single agent — true per-agent close from the explorer needs host→console
*outbound* request plumbing (`ConsoleConn` is stop-only today), a larger GUI-gated change. Per-tab
close **inside** the launcher (CPE-442) already closes one agent, and now syncs that one leaf. Final
visual behaviour wants a GUI eyeball. svelte-check 0, ai-console clippy clean, frontend suite 431 green.
