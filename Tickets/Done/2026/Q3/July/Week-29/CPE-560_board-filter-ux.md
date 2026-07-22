---
id: CPE-560
title: "Agent Board filter — 'no matches' hint + Escape to clear"
type: Feature
status: Done
priority: Low
component: Frontend
tags: [ready]
epic: CPE-503
estimate: 30m
created: 2026-07-17
closed: 2026-07-17
---

## Summary
Polish on the CPE-555 board filter. When a filter query matches nothing, the board currently just shows
empty lanes (looks like a blank/broken board again). Show a clear "no cards match" hint instead, and let
**Escape** in the filter box clear it quickly.

## Acceptance Criteria
- [x] When the board has cards but the active filter query matches none, show a "No cards match …"
      message (with the query) instead of empty lanes.
- [x] Pressing Escape in the filter input clears the query.
- [x] Clearing the query restores the full board.
- [x] `npm run check` clean; a component test covers the no-match hint + Escape-clear.

## Resolution
`BoardView` gained `$: noMatch` (cards exist, query non-blank, filtered empty) → renders a
`No cards match "<query>".` hint branch (before the board/epics render) instead of blank lanes; an
`onSearchKeydown` clears `boardQuery` on Escape (and stops propagation so it doesn't also close the board).
Component test drives a card → non-matching filter → asserts the hint + the card hidden → Escape → card
back. `BoardView.test.ts` 6 passed; `npm run check` 0/0. Board UI text is hardcoded English (the board
isn't localized), so no i18n change.

## Notes
Board UI text is hardcoded English (the board isn't localized — consistent with "Agent Board",
"No tickets found here."), so no i18n change. Isolated to `BoardView`. Purely additive.
