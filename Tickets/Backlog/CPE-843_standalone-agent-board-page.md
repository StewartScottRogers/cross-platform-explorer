---
id: CPE-843
title: Standalone Agent Board page — render BoardView chrome-less for its own window
type: feature
component: Frontend
priority: medium
status: Open
tags: ready
created: 2026-07-21
epic: CPE-841
estimate: 2-3h
---

## Summary
Foundation for the standalone Agent Board window (epic CPE-841). Let the frontend render **just** the
`BoardView` — no explorer chrome (no menu/nav/command bars, tab strip, sidebar, or status bar) — when the
app is loaded with an `agent-board` marker (a URL hash/query the board window will use). Reuse the existing
`board.ts` model + the `ticket_board` backend unchanged; the standalone page drives the same `invoke`
commands the embedded view does. The normal explorer render path is untouched when the marker is absent.

## Acceptance Criteria
- [ ] Loading the app URL with the agent-board marker (e.g. `#agent-board`) mounts **only** the BoardView,
      filling the window, with no explorer chrome.
- [ ] Without the marker, the app renders the normal explorer exactly as today (no behavioural change).
- [ ] The standalone board reads and moves cards via the existing `ticket_board` commands — no backend
      change.
- [ ] Marker detection + the standalone-vs-explorer mount decision is unit-tested (vitest); `npm run
      check` clean.

## Work Log
