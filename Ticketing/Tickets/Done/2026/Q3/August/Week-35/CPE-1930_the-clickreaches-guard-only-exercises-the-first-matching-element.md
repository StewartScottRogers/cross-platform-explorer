---
id: CPE-1930
title: the new `clickReaches` layout guard only ever exercises the **first** matching element, so a regression on the other two slips through
type: bug
priority: Low
status: Done
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

## Work Log

**2026-08-27 — closed inside PR #1045 (CPE-1883), not as its own change.**

CPE-1883's worker fixed this in the same push that resolved its merge conflict, rather than
leaving it filed: `clickReaches` now iterates every `.git-btn` match via `querySelectorAll` instead
of testing only the first.

Doing so immediately surfaced a second, smaller gap in its own first attempt — a pure viewport-bounds
check wrongly treated `.git-btn[1]` (Push) as testable at 600px, when it is actually clipped by
`.git`'s own pre-existing `overflow: hidden` (CPE-1836 territory, not this ticket's). Fixed with a
geometric ancestor-clip walk, no hit-test API, and **deliberately not gated on element size** so a
future zero-sized-button regression still gets a real dispatched click rather than a silent skip.

Measured final state: **600px busy** — Pull `clicked=true`, Push/Sync correctly skipped as
not-paintable; **900px busy** — all three `clicked=true`. Red-proofed by removing `pointer-events:
none` again: now correctly caught at **both** widths, where round 3's single-target version only ever
exercised Pull.

The remaining acceptance criterion — sweeping `rectBounds` and `pseudoOnScreen` for the same
one-element assumption — was **not** done and is worth a follow-up if either grows a multi-element
selector.
