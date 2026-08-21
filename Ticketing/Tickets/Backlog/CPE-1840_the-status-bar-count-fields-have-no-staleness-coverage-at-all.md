---
id: CPE-1840
title: the status bar's two count fields have no staleness coverage, so a stale count can ship unnoticed
type: task
priority: Medium
status: Backlog
tags: ready
estimate: S
created: 2026-08-20
closed:
---

## Problem

The status bar carries two counts derived from a listing — `filteredHidden` ("N hidden by filter",
pre-existing) and `unreadableCount` ("N entries could not be read", added by CPE-1780). Both have a
**first-paint** path that is genuinely pinned, and **three staleness paths that are pinned nowhere**.

All three mutations below are line-count-preserving and leave the whole suite green at 319 files / 4223
tests, measured during the CPE-1780 review:

1. **Delete the cache-served reset** (`ExplorerPane.svelte`, cache branch: `unreadableCount = 0`). A cache
   hit then carries the *previous folder's* count on screen until revalidation finishes — contradicting
   the prop's own documented contract.
2. **Weaken the `<StatusBar>` staleness gate** from
   `isHome || archive || smartFolder || structuredSearch ? 0 : unreadableCount` to `isHome ? 0 :
   unreadableCount`. Only the `isHome` arm is tested (via Ctrl+T); the archive, smart-folder and
   structured-search arms are unpinned, so the count can survive into a view it does not describe.
3. **Delete `unreadableCount = fresh.unreadable`** in `revalidateDir`. The count then never updates on
   revalidation.

`filteredHidden` has **identical holes in all three places**. That is why this is one ticket rather than
two: the mutations, the fix and the tests are the same shape for both fields, and splitting them would
duplicate the work.

## Why it matters

These counts exist to stop the app making a false statement about a folder — CPE-1708 and CPE-1780 are
both about a listing quietly shorter than the folder really is. A count that is *correct on first paint
and stale thereafter* makes exactly that false statement, in a way nobody would notice, because the
number looks plausible.

It is not a regression: CPE-1780 shipped `unreadableCount` at exact parity with the already-merged
`filteredHidden`, which is why it was not a merge blocker. It is a gap both fields share.

## Acceptance criteria

- [ ] Each of the three staleness paths is pinned for **both** fields — cache-served reset, the
      `<StatusBar>` gate's non-`isHome` arms, and the `revalidateDir` update.
- [ ] Red-proof every new test with the exact mutation above: make the one-line change, observe red,
      revert, record which line. All three currently leave the suite green, so a test that does not red
      under them has not closed the gap.
- [ ] The `<StatusBar>` gate is pinned per arm, not as a whole — a single test that only exercises
      `isHome` is what left three arms uncovered in the first place.
- [ ] Check whether any other listing-derived prop threaded to the status bar has the same shape, and say
      so either way rather than fixing only the two named here.

## Notes

Filed from the CPE-1780 review, where the Reviewer swept for line-count-preserving mutations after the
split and found these three. It explicitly recommended one ticket covering both fields, and judged none
of them blocking because the first-paint path is pinned and the new field ships at parity with the
existing one.

Related: CPE-1833 (those same two notes are never announced to a screen reader), CPE-1836 (the row's
layout at the 600px floor), CPE-1838 (the in-flight-listing mechanism these counts ride on).
