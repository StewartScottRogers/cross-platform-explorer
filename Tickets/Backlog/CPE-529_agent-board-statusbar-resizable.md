---
id: CPE-529
title: "Agent Board — add a bottom status bar + make the panel resizable"
type: Feature
status: Open
priority: Medium
component: Frontend
tags: [ready]
estimate: 1-2h
created: 2026-07-16
---

## Summary
Two UX gaps on the Agent Board (`BoardView.svelte`, from [[CPE-521]] / epic [[CPE-503]]): it has **no
status bar** along the bottom, and its panel is a **fixed size** (`min(1100px,96vw) × min(760px,92vh)`)
— it can't be resized. Add a bottom status bar and make the panel resizable, mirroring the other
mini-app windows (RepoBrowser's status bar + the AI Console launcher's `#sb-grip` resize).

## Acceptance Criteria
- [ ] The board panel has a **status bar along its bottom edge** showing useful state — e.g. total
      cards + per-column counts (Backlog N · Doing N · … ), the current `root` folder, and/or the
      last action/error — styled like the explorer/RepoBrowser status bar (theme tokens).
- [ ] The panel is **resizable** — a drag affordance (e.g. a bottom-right grip, matching the AI Console
      `#sb-grip`) resizes it, honouring a sensible **minimum** size; content (columns, cards) reflows.
- [ ] Resizing never breaks the layout: columns stay scrollable, no horizontal window overflow, tag
      pills keep reflowing (tick-tack rule).
- [ ] (Nice-to-have) the chosen size **persists** across opens (localStorage), like the grid layout.
- [ ] Frontend check clean; a test for any new pure sizing/status helper.

## Notes
Enhancement to [[CPE-521]] (the board is now shipped in v0.29.0). Reuse the RepoBrowser status-bar
styling + the launcher's resize-grip pattern for consistency.
