---
id: CPE-1365
title: "Agent Board Epics: Backlog (Proposed) epics render an in-work bar when their children are done"
type: Bug
status: Done
priority: Medium
component: Frontend
tags: [ready]
epic: CPE-922
created: 2026-08-06
closed: 2026-08-06
---

## Problem (reported live on v0.57.54-sidecar, Board → Epics view)

Epics in the **Backlog** column were "showing in work" — a full hatched/in-progress completion bar,
contradicting their not-started Backlog swim lane.

## Root cause

CPE-1356's `epicBar` (board.ts) only special-cased the **Done** lane and the zero-children case; any
other epic with `total > 0` fell into the `partial` (hatched in-work) branch — **including Backlog
(Proposed) epics**. Every one of the 9 Proposed epics on the board has fully-done children (they're epics
reverted to Proposed after their slices shipped — CPE-688 14/14, CPE-810 28/28, CPE-616 21/21, …), so they
all rendered a full hatched in-work bar while sitting in the Backlog column.

## Fix

Made `epicBar` fully **swim-lane-driven** (the column `epicColumn(status)` puts the epic in is the source
of truth, not the raw child counts):
- **Done** lane -> complete (solid full).
- **Backlog** lane (Proposed/not-started) -> empty/not-started track (label `—`) REGARDLESS of child
  counts. An epic can sit in Backlog with done children; it must not show an in-work bar there.
- **Doing** lane -> hatched partial (child %), or empty when it has no decomposed children.

Now every lane reads consistently: Backlog = empty, Doing = hatched-progress, Done = solid-full.

## Tests

`board.test.ts`: NEW case — `epicBar("Proposed", 14, 14)` and `("Proposed", 3, 10)` both resolve to the
empty/not-started state, not partial/complete. 25 board tests green; `npm run check` clean.

## Notes

Data observation (not fixed here — user's call): all 9 Backlog epics have 100%-done children; some may
warrant re-statusing to Done. Also CPE-1000/1002 carry `status: In Progress` while their own hygiene notes
say they were reverted to Proposed — a stale-status inconsistency that puts them in the Doing lane.

## Work Log

- 2026-08-06 — User caught it on v0.57.54 immediately after the CPE-1356 build. Root-caused to the missing
  Backlog-lane branch in epicBar; fixed swim-lane-first + added the regression test. Shipped in v0.57.55.
