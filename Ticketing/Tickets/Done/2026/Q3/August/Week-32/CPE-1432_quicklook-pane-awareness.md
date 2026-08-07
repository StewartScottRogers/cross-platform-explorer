---
id: CPE-1432
title: "Space quick-look (image + media) should honor the active pane in dual-pane mode"
type: Bug
status: Backlog
priority: Low
component: Frontend
tags: [ready]
epic: CPE-617
created: 2026-08-07
---
## Observation (from CPE-1430 review, PR #699)
Both the image quick-look (CPE-645) and the new media quick-look (CPE-1430) read pane A's
`selectedEntries`/`visible` **unconditionally**, regardless of `activePane`. So in dual-pane commander mode,
pressing **Space** while pane **B** is active still opens the quick-look for pane **A**'s selection and steps
through pane A's folder.

This is a **pre-existing** pattern inherited from CPE-645 (not introduced by CPE-1430) — but it's the same
class of pane-awareness gap the dual-pane parity program (CPE-1370–1388, CPE-1424) has been closing everywhere
else (route via `activePane` / `paneStateFor`).

## Scope
Make Space quick-look pane-aware: when pane B is active, open the image/media quick-look for pane B's
selection and step through pane B's listing. Reuse the established `paneStateFor(inPaneB)` pattern. Keep pane A
behavior identical when pane A is active (or when not in dual-pane mode).

## Acceptance
- Space with pane B active opens quick-look for pane B's selected file and steps through pane B's folder.
- Pane A behavior unchanged when pane A is active / single-pane.
- Unit test the pane selection (mirror App.paneB* specs); `npm run check` + `npx vitest run` green.

## Notes
Low priority — cosmetic/ergonomic in dual-pane mode; both quick-looks work correctly in single-pane and for
pane A. Good candidate to bundle with any future dual-pane parity pass.
