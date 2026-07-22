---
id: CPE-529
title: "Agent Board — add a bottom status bar + make the panel resizable"
type: Feature
status: Done
priority: Medium
component: Frontend
tags: [ready]
estimate: 1-2h
created: 2026-07-16
closed: 2026-07-16
---

## Summary
Two UX gaps on the Agent Board (`BoardView.svelte`, from [[CPE-521]] / epic [[CPE-503]]): it has **no
status bar** along the bottom, and its panel is a **fixed size** (`min(1100px,96vw) × min(760px,92vh)`)
— it can't be resized. Add a bottom status bar and make the panel resizable, mirroring the other
mini-app windows (RepoBrowser's status bar + the AI Console launcher's `#sb-grip` resize).

## Acceptance Criteria
- [x] The board panel has a **status bar along its bottom edge** showing useful state — e.g. total
      cards + per-column counts (Backlog N · Doing N · … ), the current `root` folder, and/or the
      last action/error — styled like the explorer/RepoBrowser status bar (theme tokens).
- [x] The panel is **resizable** — a drag affordance (e.g. a bottom-right grip, matching the AI Console
      `#sb-grip`) resizes it, honouring a sensible **minimum** size; content (columns, cards) reflows.
- [x] Resizing never breaks the layout: columns stay scrollable, no horizontal window overflow, tag
      pills keep reflowing (tick-tack rule).
- [x] (Nice-to-have) the chosen size **persists** across opens (localStorage), like the grid layout.
- [x] Frontend check clean; a test for any new pure sizing/status helper.

## Notes
Enhancement to [[CPE-521]] (the board is now shipped in v0.29.0). Reuse the RepoBrowser status-bar
styling + the launcher's resize-grip pattern for consistency.

## Resolution
Added a bottom status bar + a resizable panel to `BoardView.svelte`.

- **Status bar** along the panel's bottom edge: per-lane counts (Backlog N · Doing N · Review N · …),
  a centred last-action/error message (set on each move/send-to-review), and the current `root` folder
  (ellipsised), styled with theme tokens like the explorer/RepoBrowser status bar.
- **Resizable panel:** the fixed `min(1100px,96vw) × min(760px,92vh)` size is replaced by inline
  `width`/`height` bound to state, dragged from a **bottom-right grip** (mirrors the AI Console
  `#sb-grip`); size is **clamped** to `[640×420, viewport]` and **persisted** in localStorage, restored
  on reopen. `max-width/height: 98vw/96vh` keeps it inside the window.
- Pure `clampBoardSize` + `loadBoardSize`/`saveBoardSize` in `board.ts` (unit-tested).

`npm run check` clean; 545 frontend tests pass (1 new clamp test). First dayshift board-v2 ticket.

## Work Log
2026-07-16 — Picked up (dayshift). Added clampBoardSize/load/save (tested), a bottom status bar (counts/root/last-action), and a resize grip with clamp+persist. npm check clean; 545 tests. All ACs met.
