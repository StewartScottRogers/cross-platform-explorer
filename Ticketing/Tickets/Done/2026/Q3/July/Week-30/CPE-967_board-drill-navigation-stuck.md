---
id: CPE-967
title: Agent Board — after drilling an epic to a no-match filter you can't get back
type: bug
component: Frontend
priority: high
status: Done
tags: ready
created: 2026-07-23
closed: 2026-07-23
epic: CPE-503
---

## Summary
Following an epic's "View tickets" sets the filter to the epic id. If it yields no board cards, the board
shows "No cards match" and the user is stranded — the Board/Epics toggle appears dead and there's no clear
way back. Two causes:
1. The top-level `{:else if noMatch}` branch sits ABOVE the `viewMode === "epics"` branch, so once the
   filter matches no *board* cards it hijacks the render even in Epics view — clicking ◧ Epics does nothing.
2. `noMatch` only looks at non-archived `filtered`, so a closed epic (all children archived) shows "No
   cards match" while hiding the archived cards that ARE displayable; and there's no clear-filter affordance.

## Acceptance Criteria
- [x] The "No cards match" empty state is rendered INSIDE the board branch; the Epics view has its own
      "No epics match" empty state — so toggling Board ⇄ Epics always works.
- [x] `noMatch` accounts for archived (a closed epic's archived tickets show instead of a false empty).
- [x] Both empty states offer a **Clear filter** button (sets the filter empty); Esc still clears.
- [x] `npm run check` 0/0; vitest green (board filter tests); GUI-verify the drill→back flow.