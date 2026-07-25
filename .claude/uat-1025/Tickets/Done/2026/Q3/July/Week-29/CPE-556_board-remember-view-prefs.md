---
id: CPE-556
title: "Agent Board remembers its view mode + archived toggle across opens"
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
Small polish on the board thread (CPE-551/554/555). The Agent Board resets to **Board** view (vs Epics)
and hides archived Done every time it opens. Persist those two view preferences (like the board root +
size already are) so the board reopens the way you left it.

## Acceptance Criteria
- [x] The board view mode (`board` / `epics`) persists across opens (localStorage `cpe.boardView`).
- [x] The "show archived Done" toggle persists across opens (`cpe.boardArchived`).
- [x] A malformed/absent stored value degrades to the current defaults (board view, archived hidden).
- [x] `npm run check` clean; a component test covers restore-on-open.

## Resolution
`BoardView` initialises `viewMode` from `savedView()` (localStorage `cpe.boardView`, `"epics"` else
`"board"`) and `showArchived` from `savedArchived()` (`cpe.boardArchived === "1"`), and reactive
`$: persistView(viewMode)` / `$: persistArchived(showArchived)` write changes back — both defensively
try/catch'd so a missing/blocked localStorage degrades to the defaults. Consistent with the existing
`cpe.boardRoot`/board-size persistence. Test: `BoardView.test.ts` restores the saved view mode on open (the
Epics button is `active`); suite 5 passed; `npm run check` 0/0. Purely additive.

## Notes
Isolated to `BoardView` + localStorage (consistent with `cpe.boardRoot`/board size). Purely additive.
