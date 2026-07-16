---
id: CPE-521
title: "Agent Board — Kanban UI: columns + drag a card to change status"
type: Feature
status: Open
priority: Medium
component: Frontend
tags: [needs-prereq]
epic: CPE-503
sprint: SPR-03
estimate: 3-4h
created: 2026-07-16
---

## Summary
The Agent Board view ([[CPE-503]], wave 1): a **Kanban** of the `Tickets/` folders — columns
(Backlog / Doing / Blocked / Deferred / Done / Epics? / Sprints?) of cards from [[CPE-520]]. **Drag a
card** to another column → calls `board_move` (the file moves + status updates). Read + drag only;
agent dispatch is wave 2.

## Acceptance Criteria
- [ ] A board view renders columns of cards (id, title, type/priority, tags) from `board_cards`.
- [ ] Dragging a card to another column calls `board_move` and reflects the new state; failure surfaces
      without losing the card.
- [ ] The board refreshes to stay consistent with the folders (and the CLI `/ticketing-*` flow).
- [ ] Cards follow the UI conventions (tick-tack reflow for tag pills; menu/theme rules).
- [ ] Frontend tests for the column model + a drag→move interaction (mocked backend).

## Notes
**needs-prereq:** [[CPE-520]] (the card + move backend). Read+drag-first per the activation decision.
