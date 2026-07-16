---
id: CPE-521
title: "Agent Board — Kanban UI: columns + drag a card to change status"
type: Feature
status: Done
priority: Medium
component: Frontend
tags: [needs-prereq]
epic: CPE-503
sprint: SPR-03
closed: 2026-07-16
estimate: 3-4h
created: 2026-07-16
---

## Summary
The Agent Board view ([[CPE-503]], wave 1): a **Kanban** of the `Tickets/` folders — columns
(Backlog / Doing / Blocked / Deferred / Done / Epics? / Sprints?) of cards from [[CPE-520]]. **Drag a
card** to another column → calls `board_move` (the file moves + status updates). Read + drag only;
agent dispatch is wave 2.

## Acceptance Criteria
- [x] A board view renders columns of cards (id, title, type/priority, tags) from `board_cards`.
- [x] Dragging a card to another column calls `board_move` and reflects the new state; failure surfaces
      without losing the card.
- [x] The board refreshes to stay consistent with the folders (and the CLI `/ticketing-*` flow).
- [x] Cards follow the UI conventions (tick-tack reflow for tag pills; menu/theme rules).
- [x] Frontend tests for the column model + a drag→move interaction (mocked backend).

## Resolution
Added the Agent Board Kanban view over the real `Tickets/` folders.

- **`src/lib/board.ts`** (new, pure, 5 tests): `BOARD_COLUMNS` (the 5 workflow columns), `groupByColumn`
  (grouped + **numeric** id ordering so CPE-9 precedes CPE-100; unknown columns dropped), `columnCounts`,
  `isColumn`, and `isValidMove` (known card + valid + actually-different column).
- **`src/lib/components/BoardView.svelte`** (new): a bordered overlay with the 5 columns of cards
  (id · priority · title · **reflowing** tag/epic/sprint pills — tick-tack rule; epic/sprint pills
  tinted). **HTML5 drag-and-drop** — dragging a card highlights the target column and, on drop, calls
  `board_move` **optimistically** (the card moves instantly) then reloads to reconcile with the folders
  (also picking up CLI `/ticketing-*` changes); a failed move surfaces an error and the reload restores
  truth. A Refresh button + close.
- **Wiring:** a **"Agent Board"** entry in the Sidebar (next to Repositories) opens it on the current
  folder (`root = currentPath`), so any repo whose `Tickets/` follows the convention gets a board.

Read + drag only — agent dispatch is wave 2 (CPE-522). Verified: `npm run check` clean; 531 frontend
tests pass (5 new board-model tests). Second ticket of SPR-03 — completes the board wave-1 foundation.

## Work Log
2026-07-16 — Picked up (SPR-03 wave 1; prereq CPE-520). Estimate: 3-4h.
2026-07-16 — Built board.ts (pure grouping/ordering/move-validation) + BoardView.svelte (columns, drag-and-drop → optimistic board_move → reload) + a Sidebar entry. 5 new tests. npm check clean; 531 tests pass. All ACs met.

## Notes
**needs-prereq:** [[CPE-520]] (the card + move backend). Read+drag-first per the activation decision.
