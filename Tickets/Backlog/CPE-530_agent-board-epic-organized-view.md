---
id: CPE-530
title: "Agent Board — epic-organized view: epics in a left pane, tickets to-do (top) → done (bottom)"
type: Feature
status: Open
priority: Medium
component: Multiple
tags: [big-design, needs-decision]
estimate: 3-4h
created: 2026-07-16
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
- [ ] A **left pane** lists epics (from `Tickets/Epics/` + closed epics in `Done/`), with status
      (`Proposed`/`In Progress`/`Done`) and `X of Y children Done`; selectable.
- [ ] Selecting an epic shows **its child tickets** (cards whose `epic:` frontmatter names it), ordered
      **to-do → done**: outstanding statuses (Backlog / Doing / Blocked / Deferred) at the **top**,
      **Done** at the **bottom** (with a visible divider / grouping).
- [ ] An **"No epic"** grouping surfaces tickets not attached to any epic (so nothing is hidden).
- [ ] Backend surfaces epics (id, title, status, child counts) for the left pane — `board_cards` returns
      each card's `epic`, but the epic *list* + titles need a command (e.g. `board_epics(root)`).
- [ ] Card actions from the flat board still work (move status, Dispatch, Review) within this view.
- [ ] Frontend check clean; tests for the epic-grouping + to-do/done ordering (pure helper).

## Open questions (resolve when worked)
- **needs-decision:** does the epic view **replace** the column Kanban, or is it a **toggle** alongside
  it (like Tabs⇄Grid in the AI Console)? (Recommend a toggle so both reads are available.)
- Drag semantics in the epic view — drag a card up/down to change status, or keep status changes to
  explicit actions here?

## Notes
Reorganisation of [[CPE-521]] / epic [[CPE-503]]. Relates to [[CPE-529]] (status bar + resizable). The
board already reads real `Tickets/` files and each card carries its `epic:`, so the grouping is a
frontend concern + a small `board_epics` backend addition.
