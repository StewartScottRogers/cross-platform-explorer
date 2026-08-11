---
id: CPE-1628
title: "savedSearchStore.test.ts fails only inside a full-suite run — test-order pollution makes the suite intermittently red"
type: Bug
status: Backlog
priority: Medium
component: Frontend
tags: [ready]
created: 2026-08-11
closed:
---

## Why
Observed by the independent Reviewer of PR #817, on a branch whose diff does not touch this file at all.
A full `npx vitest run` failed at `src/lib/savedSearchStore.test.ts:172` — `expect(persisted).toHaveLength(1)`
received `2`. Running that file alone passed 24/24, and re-running the full suite passed 3316/3316.

So it is order/state pollution, not a real regression. But an intermittently red suite is corrosive: it
trains the crew to re-run and shrug, which is precisely how a genuine failure eventually gets waved
through. This crew's whole QA model rests on a green run meaning something.

## The likely mechanism
`persisted` accumulating an extra entry points at shared state surviving between test files — a
module-level store, a `localStorage`/persistence shim, or a subscription never torn down — so a saved
search written by an earlier file is still present when this one asserts. Confirm before fixing.

## Scope
- Reproduce deterministically: run the full suite with a fixed seed / no shuffle, then bisect the file
  order to identify the polluter (vitest accepts an explicit file list, which makes narrowing quick).
- Fix the **root cause** — reset the store/persistence between files — rather than making the assertion
  tolerant. An assertion loosened to `>= 1` would hide the very leak this ticket exists to remove.
- Add whatever teardown the pattern needs so the same class of leak cannot recur silently.

## Acceptance criteria
- The polluting interaction is named in the work log, with the evidence that identified it.
- The full suite passes on at least 5 consecutive runs with no order-dependent failure.
- The fix is isolation/teardown, not a weakened assertion.

**Conflict surface:** `src/lib/savedSearchStore.ts` and its test, plus whichever test setup/teardown file
the leak traces to. Independent of current feature work.
