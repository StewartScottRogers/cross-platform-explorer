---
id: CPE-336
title: "Multiple AI Console sessions + docking"
type: Feature
status: Open
priority: Medium
component: Multiple
created: 2026-07-13
---

## Summary

CPE-335 made the AI Console a single pop-out window. Support MULTIPLE concurrent consoles
and docking them together: each session its own window, with an in-app tabbed dock that can
collect several, pop a tab out to its own window, or dock a window back in (the user's
"dockable when more than one is present").

## Scope
- Backend: the host holds one `AiConsoleState` connection; support N keyed sessions so
  multiple sidecars/sessions run at once (each its own process).
- Frontend: window-per-session (unique labels) + a tabbed dock manager (drag to dock/undock).

## Depends on: [[CPE-335]].
