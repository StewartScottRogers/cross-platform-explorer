---
id: CPE-926
title: Agent Board opens the bare sidecar board (no toolbar/epics) — route to full BoardView
type: bug
component: Frontend
priority: high
tags: ready
created: 2026-07-23
status: Done
---

## Summary
Opening the **Agent Board** showed a board with **no top toolbar and no Board/Epics toggle** — the user
couldn't switch to Epics or anything else. Root cause: `openAgentBoard()` *prefers* the out-of-process
**agent-board sidecar**, whose served UI (`sidecar/agent-board/src/ui.rs`) is a barer reimplementation —
just a text `<header>Agent Board</header>`, columns, and drag; **no toolbar, no Board⇄Epics kanban toggle,
no filter**. CPE-920 made the sidecar binary reliably bundled, so the app started *preferring* that bare
board over the full in-process `BoardView` (which has the toolbar + the CPE-922 Epics kanban).

## Fix
Route the Agent Board to the full in-process `BoardView` window (`index.html?board=1`) — it has the
toolbar, Board⇄Epics kanban, filter, and archive. Keep the sidecar-board code path behind a disabled
`PREFER_SIDECAR_BOARD` flag so it can return once it reaches feature parity.

## Acceptance Criteria
- [x] Opening the Agent Board shows the full toolbar (Filter, Board, Epics, Project, Refresh, Docs).
- [x] The Epics kanban (CPE-922) is reachable.
- [x] `npm run check` passes.

## Work Log
- 2026-07-23 — Diagnosed by running the frontend in a browser: BoardView (`?board=1`) renders the full
  toolbar + Epics kanban correctly; the breakage was the sidecar-board preference, not BoardView.

- 2026-07-23 — Fixed: openAgentBoard() now opens the in-process BoardView (PREFER_SIDECAR_BOARD=false) instead of the bare sidecar board. Verified BoardView renders the full toolbar + Epics kanban in a browser. check clean.
