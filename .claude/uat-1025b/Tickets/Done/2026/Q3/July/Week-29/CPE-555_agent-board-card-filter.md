---
id: CPE-555
title: "Agent Board — filter/search cards by id, title, tag, type, or priority"
type: Feature
status: Done
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-503
estimate: 1h
created: 2026-07-17
closed: 2026-07-17
---

## Summary
As a project's `Tickets/` grows, the Agent Board (CPE-521) becomes a wall of cards with no way to narrow
it. Add a search box that filters the visible cards across all lanes/epics by a free-text query — matching
a card's **id, title, any tag, type, or priority** (case-insensitive substring). Empty query shows
everything (unchanged behaviour).

## Acceptance Criteria
- [x] A pure `filterCards(cards, query)` in `board.ts` matches id/title/tags/type/priority
      (case-insensitive substring); blank query returns all. Unit-tested.
- [x] `BoardView` shows a search input in the titlebar; typing filters the board (both the Kanban lanes
      and the Epics view, and the archived-Done list) live.
- [x] Empty/cleared query restores the full board.
- [x] `npm run check` clean; a board.ts unit test covers the filter.

## Resolution
Added pure `board.ts::filterCards(cards, query)` — case-insensitive substring over id/title/tags/type/
priority, blank → all (3 unit tests: blank passthrough, field matching, no-match). `BoardView` gained a
`boardQuery` search input in the titlebar; a reactive `$: filtered = filterCards(cards, boardQuery)` feeds
`groupByLane` (Kanban), `groupByEpic` (Epics view), and the archived-Done list, so typing narrows every
view live and clearing restores the full board. `epicProgress` stays on the full set (it's a progress
metric, not a filtered list). `board.test.ts` 14 passed, `BoardView.test.ts` 4 passed, `npm run check` 0/0.
Purely additive — empty box = prior behaviour. (Live GUI re-verify batched with the other nightshift
board fixes.)

## Notes
Pure filter core (like the other board logic) keeps it testable headlessly. Purely additive — no change to
default behaviour when the box is empty. Complements CPE-551/554 (root handling) on the same board thread.
