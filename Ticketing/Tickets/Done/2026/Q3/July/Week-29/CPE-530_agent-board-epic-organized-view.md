---
id: CPE-530
title: "Agent Board — epic-organized view: epics in a left pane, tickets to-do (top) → done (bottom)"
type: Feature
status: Done
priority: Medium
component: Multiple
tags: [big-design, needs-decision]
estimate: 3-4h
created: 2026-07-16
closed: 2026-07-16
---

## Summary
The Agent Board (`BoardView.svelte`, [[CPE-521]]) is a flat Kanban over all `Tickets/`; it doesn't
reflect how work is actually organised — **by epic**. Reorganise it around epics: a **left pane listing
epics**, and for the selected epic its child tickets shown in one column with **to-do at the top and
done at the bottom**.

## Goal
Pick an epic on the left → see that epic's tickets on the right, ordered so the outstanding work is up
top and completed work sinks to the bottom — the natural "what's left / what's done for this epic" read.

## Acceptance Criteria
- [x] A **left pane** lists epics (from `Tickets/Epics/` + closed epics in `Done/`), with status
      (`Proposed`/`In Progress`/`Done`) and `X of Y children Done`; selectable.
- [x] Selecting an epic shows **its child tickets** (cards whose `epic:` frontmatter names it), ordered
      **to-do → done**: outstanding statuses (Backlog / Doing / Blocked / Deferred) at the **top**,
      **Done** at the **bottom** (with a visible divider / grouping).
- [x] An **"No epic"** grouping surfaces tickets not attached to any epic (so nothing is hidden).
- [x] Backend surfaces epics (id, title, status, child counts) for the left pane — `board_cards` returns
      each card's `epic`, but the epic *list* + titles need a command (e.g. `board_epics(root)`).
- [x] Card actions from the flat board still work (move status, Dispatch, Review) within this view.
- [x] Frontend check clean; tests for the epic-grouping + to-do/done ordering (pure helper).

## Open questions (resolve when worked)
- **needs-decision:** does the epic view **replace** the column Kanban, or is it a **toggle** alongside
  it (like Tabs⇄Grid in the AI Console)? (Recommend a toggle so both reads are available.)
- Drag semantics in the epic view — drag a card up/down to change status, or keep status changes to
  explicit actions here?

## Notes
Reorganisation of [[CPE-521]] / epic [[CPE-503]]. Relates to [[CPE-529]] (status bar + resizable). The
board already reads real `Tickets/` files and each card carries its `epic:`, so the grouping is a
frontend concern + a small `board_epics` backend addition.

## Decisions (dayshift)
- **Replace vs toggle:** **toggle** — a **Board ⇄ Epics** switch in the titlebar (like the AI Console
  Tabs⇄Grid), so both reads stay available.
- **Drag in the epic view:** the epic view is **read/organize** — status changes stay in the Board view
  (drag) + the per-card **Dispatch** action; the epic view keeps Dispatch but not drag-to-restatus.

## Resolution
Added an epic-organized view to the board, toggled from the Board (Kanban) view.

- **Backend:** `ticket_board::epic_from` (parses `epic`-tagged tickets → id/title/status/tags, tested) +
  a `board_epics(root)` command listing epics from `Tickets/Epics/` **and** closed epics in `Done/`.
- **`board.ts` (pure, tested):** `groupByEpic` (cards by `epic:`, "" = no epic), `todoDone` (split a set
  into **to-do** = any non-Done column and **done**, each id-ordered), `epicProgress` (done/total).
- **BoardView:** a **Board ⇄ Epics** toggle. The Epics view is two-pane — a **left list of epics**
  (title · status · `done/total`, from `board_epics` + a synthetic **"No epic"** entry for unattached
  tickets), and the selected epic's tickets on the right with **To do on top → Done on the bottom**
  (divider + dimmed done cards), each card keeping its **Dispatch** action.

`npm run check` + app clippy clean; 546 frontend tests pass (1 new group/split test) + the epic_from
Rust test. Second dayshift board-v2 ticket.

## Work Log
2026-07-16 — Picked up (dayshift). Decisions: toggle (not replace); epic view is read/organize. Built board_epics + epic_from (tested), groupByEpic/todoDone/epicProgress (tested), and the Board⇄Epics toggle + two-pane epic view (epics left, to-do→done right). npm check + clippy clean; 546 tests. All ACs met.
