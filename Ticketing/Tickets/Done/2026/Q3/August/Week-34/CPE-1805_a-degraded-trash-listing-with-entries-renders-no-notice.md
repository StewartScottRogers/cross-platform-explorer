---
id: CPE-1805
title: a degraded trash listing that still has entries renders no notice at all
type: task
priority: Low
status: Done
tags: ready
estimate: S
created: 2026-08-20
closed: 2026-08-20
---

## Problem

`TrashView.svelte` (~line 200) special-cases **empty + degraded** only. A listing that is degraded *and*
still has entries falls straight through to the plain list, with no indication that what is shown is
incomplete.

Today that state is **unreachable** — the backend's degradation path returns zero entries by
construction. So this is not a live bug.

## Why file it anyway

It is an **undocumented dependency on a backend invariant.** The frontend is correct only because the
backend currently cannot produce degraded-with-entries. Nothing states that, nothing tests it, and
**CPE-1804's fix would break it**: once a listing can skip *some* items and return the rest, degraded
-with-entries becomes the normal case, and this render path will silently show a partial list as if it
were complete.

So the sequencing matters — whoever does CPE-1804 needs this handled, or they will ship the same lie in a
new shape.

## What to do

- Either make the invariant explicit and enforced (assert/test that degraded implies empty, so a future
  change fails loudly), **or** render an incomplete-listing notice above the list. Pick one and say why —
  they are genuinely different bets about where this codebase is heading, and CPE-1804 suggests an answer.
- If CPE-1804 is done first, this becomes part of it rather than a separate change; note that in whichever
  lands second.

## Notes

Filed by the Foreman from the independent review of PR #957, 2026-08-20.

Related: **CPE-1803** (built the degraded state), **CPE-1804** (the change that would make this live).

## Work Log

- 2026-08-20 — merged as **#962** (`6dc2b749`), batch 37, together with **CPE-1804**. The two were shipped in
  one PR because CPE-1804's fix is what makes this state reachable: before it, `degraded` implied zero
  entries, so the frontend was correct only by accident.

### The decision
Took **"render a notice"** over "assert the invariant". CPE-1804 *is* the answer to which way this codebase
is heading — partial listings are now normal, so an assertion that degraded-implies-empty would have been
false the day it landed.

### The shape that matters
The notice is driven by **`degraded` alone**. `entries.length` chooses only *where* it sits — centred in an
empty pane, or a sticky banner above a partial list — **never whether it appears**. So the frontend no longer
depends on the unstated backend invariant at all, which was the whole complaint in this ticket.

The titlebar count stays suppressed whenever the pass is incomplete, so the app never asserts a total it
cannot back.

### Verification
The UAT rendered all four states and dumped the DOM: genuinely empty, degraded-empty (panic route),
all-skipped (the new route), and **degraded with entries** — the previously-unreachable one this ticket is
about. It confirmed by `compareDocumentPosition` that the notice sits *above* the rows, and that a complete
listing with identical rows shows no notice at all.

Removing the banner reds two tests and leaves eighteen green, including the asymmetry test — so the pin
discriminates rather than merely failing.

### Remaining
**CPE-1816** — while the listing is still streaming, a partial list renders as complete with a confident item
count, because the incompleteness flag rides on the summary and the summary by construction arrives last.
Pre-existing and inherent to the streaming design; the window closes on completion.
