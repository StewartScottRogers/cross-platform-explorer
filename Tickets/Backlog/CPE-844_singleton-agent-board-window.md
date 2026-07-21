---
id: CPE-844
title: Singleton Agent Board window + launcher (WebviewWindow, app-wide singleton)
type: feature
component: Frontend
priority: medium
status: Open
tags: needs-prereq
created: 2026-07-21
epic: CPE-841
estimate: 3-4h
---

## Summary
Give the Agent Board its own window, mirroring the AI Console window pattern in `App.svelte`
(`openAiConsole`/`launchAiConsole`). `openAgentBoard()` does `WebviewWindow.getByLabel(AGENT_BOARD_LABEL)`
→ `setFocus()` when one exists (**app-wide singleton**), else `new WebviewWindow(AGENT_BOARD_LABEL, { url:
<app url + agent-board marker (CPE-843)>, title: "Agent Board", resizable, minWidth/minHeight })`. Add a
launcher that **keeps the embedded BoardView** (per the epic decision) and offers "open in window" — a
pop-out button in the board header plus a menu/Sidebar entry.

**Key difference from the AI Console window:** the AI Console window loads an *untrusted* sidecar URL and
is deliberately in no capability (no Tauri API). This board window renders **our own trusted BoardView**,
so it needs `invoke` access — its label must be added to `src-tauri/capabilities/default.json`, or the
board can't read/move cards. The window remembers its size/position across restarts
(`tauri-plugin-window-state`, as the main window does, CPE-228). Prereq: CPE-843.

## Acceptance Criteria
- [ ] A launcher (pop-out button in the board header + a menu/Sidebar entry) opens the Agent Board in its
      own window; the embedded in-app view still works (Keep both).
- [ ] App-wide singleton: a second launch **focuses the existing window** instead of opening another.
- [ ] The board window can `invoke` the `ticket_board` commands (its label is in `default.json`
      capabilities) — reading and moving cards works from the window.
- [ ] The window is resizable with a sensible min size and remembers its size/position across restarts.
- [ ] GUI-verified: open the board, move a card, relaunch focuses the same window; the main explorer is
      unaffected.

## Notes
Prereq: **CPE-843** (the chrome-less standalone page the window loads).

## Work Log
