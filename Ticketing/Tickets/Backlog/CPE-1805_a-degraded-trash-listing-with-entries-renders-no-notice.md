---
id: CPE-1805
title: a degraded trash listing that still has entries renders no notice at all
type: task
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-20
closed:
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
