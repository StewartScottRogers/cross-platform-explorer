---
id: CPE-1930
title: the new `clickReaches` layout guard only ever exercises the **first** matching element, so a regression on the other two slips through
type: bug
priority: Low
status: Open
tags: ready
estimate: XS
created: 2026-08-27
---

## Summary

CPE-1883 added a `clickReaches` check kind to the layout-guard harness — the third check kind in that
harness — which dispatches a **real** CDP mouse click rather than asking `elementFromPoint`. That was
the right instrument: `elementFromPoint` / `elementsFromPoint` returned a false clean on this exact
question **twice**, to two different agents.

But its `selectors: [".git .git-btn"]` resolves via `document.querySelector`, which only ever grabs
the **first** match — Pull, given DOM order. The selector reads as "the class of git buttons"; the
check only protects one of them.

## Measured

PR #1045's Reviewer isolated a synthetic regression to **Push only**
(`.git button.git-btn:nth-of-type(2) { pointer-events: none; }`), confirmed with its own dispatched
clicks that Push was genuinely broken at 900px while Pull and Sync were fine, then ran the real
`npm run harness:layout-guard`:

    14/14 PASS      <- false negative

For contrast, the guard **is** real for what it targets: reverting CPE-1883's actual
`pointer-events: none` fix line correctly reds both widths, and a zero-sized-target stress test made
it report `CLICK-MISS ... landed on SPAN.git instead` honestly rather than crashing, hanging, or going
silently green.

## Why it is Low

Every regression actually seen across CPE-1883's three rounds was scoped to the **shared
`:focus-visible` base rule** all three buttons sit under — which the current check does catch. The gap
is a future regression narrowly scoped to Push or Sync specifically.

## Acceptance criteria

- [ ] Make `clickReaches` iterate `querySelectorAll` rather than resolving one element — or list
      Pull / Push / Sync explicitly. Prefer the former: a new button added to that chip should be
      covered without anyone remembering to add it.
- [ ] Sweep the harness's **other** check kinds (`rectBounds`, `pseudoOnScreen`) for the same
      one-element assumption.
- [ ] Red-proof it the way the Reviewer did: break **only the second** matching element and confirm
      the guard now fails, naming which one.
- [ ] Mind the 600px case — Push and Sync are off-viewport at that width regardless of focus state
      (a pre-existing row-overflow issue, unrelated to CPE-1883). An iterating check must skip or
      report off-viewport targets honestly rather than counting them as misses.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1045's Reviewer, which flagged it as a low-severity
follow-up rather than a blocker and was explicit that it is a testing-coverage nuance, not a defect in
what ships.

Related: **CPE-1883** (which added the check), **CPE-1882** (the layout-guard harness).
