---
id: CPE-1885
title: key the bidi-escape guard's registry by expression text, not line number — it cost three round-trips in one day
type: task
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-23
closed:
---

## Problem

`src/lib/bidiEscape.guard.test.ts` records each component's raw-render sites as
`"<line>:<expression>"` strings and compares them against a live rescan. Any edit that shifts lines in
a guarded component fails the guard with a wall of "NEW offender" and "STALE recorded" entries that
are **the same expressions at new addresses**.

Three separate round-trips lost to this in a single day (batched run `batched-2026-08-23-1124`):

1. **CPE-1833/CPE-1836** — the worker had to update 13 shifted line numbers for `StatusBar.svelte`.
2. **CPE-1827** — same, for `TrashView.svelte`.
3. **CPE-1827 again** — adding *one line* (`node.focus()` in `clampToAnchor`) shifted every site by 5
   and reddened CI with **27 "new" and 27 "stale" entries**, all of them the identical 27 expressions.
   The Foreman had to apply a mechanical `+5` and re-push.

## Why fix it rather than live with it

The guard is **sound** and must not be weakened. PR #1019's reviewer made the case for it precisely:
it compares against a **live rescan** rather than an allowlist, and requires exact set equality in both
directions, so it cannot silently lie — a new unregistered raw render fails, and a stale entry fails
too. That is the right shape and it has caught real offenders.

The problem is only that it **cries wolf**, and a guard that cries wolf on unrelated edits is a guard
people learn to update reflexively without reading. That is one small step from updating it reflexively
when it has found something real. The cost is not the minutes; it is the erosion.

## What to do

Key each entry by its **matched expression text** (plus the component) instead of its line number. The
expression is what the guard actually cares about — whether a raw, unescaped value reaches the DOM —
and it is stable under reformatting, insertion and deletion.

Watch for the one real wrinkle: a component with the **same expression on two different lines**
(`TrashView.svelte` already has `342:itemCountLabel` and `342:selectedCountLabel` on one line, and
`355`/`356` both `$t("trash.moreActions")`). A bare set of expressions loses that multiplicity, so
count occurrences rather than deduplicating, or key by `expression` plus an occurrence index.

**Prove it still bites.** The whole value is that it fails on a genuinely new raw render:

- add a new unescaped expression to a guarded component → must go **red**
- delete a registered one → must go **red** (the stale-entry direction)
- reformat a guarded component so every line moves, changing nothing else → must stay **green**

That third case is this ticket, and it is the one to demonstrate most carefully.

Consider also whether the failure message can name the *component and expression* first and leave the
addresses out — most of the 27-entry wall above was noise around a one-word fact.

## Acceptance criteria

- [ ] A pure reformat of a guarded component does not fail the guard.
- [ ] A genuinely new raw render still fails it — demonstrated.
- [ ] A deleted registered render still fails it — demonstrated.
- [ ] Duplicate expressions within one component are still counted correctly.
- [ ] The registry no longer contains line numbers, so nothing needs mechanical updating on an edit.

## Notes

Related but distinct: **CPE-1817** fixed the same fragility in a *different* guard, where a call-site
count used a single-line `grep` and mis-fired on a wrapped call. The fix there was to collapse
whitespace before counting. Two guards, one root cause: pinning code by its position in a file rather
than by what it says.

## Work Log

- **2026-08-23 20:40 USMST** — Filed by the Foreman during batched run `batched-2026-08-23-1124`,
  after the third occurrence in one day. PR #1019's reviewer independently recommended the same change
  and correctly noted the guard is self-correcting rather than merely fragile, which is why this is a
  usability fix and not a correctness one.
