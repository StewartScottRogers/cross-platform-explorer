---
id: CPE-523
title: "Agent Board — agents update cards + a review column"
type: Feature
status: Done
priority: Medium
component: Multiple
tags: [needs-prereq]
epic: CPE-503
sprint: SPR-04
closed: 2026-07-16
estimate: 2-3h
created: 2026-07-16
---

## Summary
Wave 2 of [[CPE-503]]: as a dispatched agent works, its **card reflects progress** (state / appended
findings), and finished work lands in a **Review** column before Done — closing the "agents move cards
through review" loop from the brief.

## Acceptance Criteria
- [x] A dispatched agent's progress updates its card (status and/or an appended findings note).
- [x] A **Review** step/column exists between Doing and Done; a card lands there for sign-off before Done.
- [x] Updates stay consistent with the `Tickets/` files + the CLI flow (no divergent state).
- [x] Tests for the card-update + review-transition logic.

## Resolution
Added a **Review** lane + agent card-update affordances to the board — all still backed by the real
`Tickets/` files (no new folder, so the CLI workflow is untouched).

- **Virtual Review lane** (no `Tickets/Review/` folder): a Doing card carrying the **`review` tag**
  shows in a Review lane between Doing and Done. `board.ts` `BOARD_LANES` / `laneFor` / `groupByLane`
  (pure, tested) derive it; the board renders 6 lanes.
- **Backend (pure + tested in `ticket_board.rs`):** `set_review(md, on)` adds/removes the `review` tag
  (creating a `tags:` line if absent, idempotent); `append_finding(md, note)` records a finding under a
  `## Findings` section (created if absent, newest-first, blank = no-op). Commands `board_review(root,
  id, on)` and `board_note(root, id, note)` apply them to the file — the latter is the affordance a
  dispatched agent (or the UI) uses to record progress.
- **UI:** dragging a Doing card into the **Review** lane marks it for review; dragging a Review card to
  a real column clears the tag and moves the file. Because the board reads the real files, an agent
  working the ticket via `/ticketing-work` (folder move + Work Log) is reflected on refresh — that's the
  primary "agents update cards" path; `board_note` is the direct one.

Consistency: the review tag + findings live in the actual ticket markdown, so the board and CLI never
diverge. Verified: `cargo clippy` clean both feature modes; **9 `ticket_board` tests** (incl.
set_review + append_finding) + **8 board-model tests** (incl. lane derivation); 534 frontend tests pass;
`npm run check` clean. Final ticket of SPR-04 — **completes the Agent Board epic CPE-503**.

## Work Log
2026-07-16 — Picked up (SPR-04 wave 2, final). Estimate: 2-3h.
2026-07-16 — Added the virtual Review lane (tag-driven, no new folder), set_review + append_finding pure helpers + board_review/board_note commands, and the drag-to-Review UI. 5 new tests (2 Rust review/finding wired via the existing 9; 3 TS lane). clippy clean both modes; 534 frontend tests pass. All ACs met.

## Notes
**needs-prereq:** [[CPE-522]] (dispatch). Ties to the Swarm quality-gate idea ([[CPE-518]]).
