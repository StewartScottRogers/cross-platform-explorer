---
id: CPE-493
title: "Show the session chip in the Agents context menu (disambiguate 'Close this session')"
type: Feature
status: Done
priority: Medium
component: Frontend
tags: [ready]
estimate: 30m
created: 2026-07-16
closed: 2026-07-16
epic: CPE-261
---

## Summary
The left-pane Agents leaves now carry a correlation chip (colour + number, CPE-490). The right-click
context menu's "Close this session" shows only a text label — so with several sessions a user can't be
sure which leaf the menu belongs to. Render the **same chip** in the menu item so it's unambiguous.

## Acceptance Criteria
- [x] The Agents context menu's per-session "Close …" item shows the **same colour+number chip** as the
      leaf it was opened from (derived from the same `sessionId`).
- [x] The chip is identical to the leaf's (shared `sessionChip` helper — no drift).
- [x] The close-all item is unchanged.

## Resolution
`AgentMenu.svelte` now computes the chip from its `sessionId` prop via the shared
`sessionColor`/`sessionNum` (`src/lib/sessionChip.ts`) and renders it as the leading element of the
per-session "Close …" item — the same 16px colour+number chip the leaf shows — so the menu visually
matches the leaf it belongs to. Files: `src/lib/components/AgentMenu.svelte`.
