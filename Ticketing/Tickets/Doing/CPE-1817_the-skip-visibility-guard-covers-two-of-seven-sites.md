---
id: CPE-1817
title: the skip-visibility guard covers two of seven sites while reading as if it covers the mechanism
type: task
priority: Medium
status: Doing
tags: ready
estimate: S
created: 2026-08-20
closed:
---

## Problem

CPE-1806 routed all **seven** `trash_roundtrip_available()` call sites through `require_staged`, so a test
that cannot stage its precondition now fails loudly instead of skipping into a green log. It then added a CI
guard so that routing cannot be silently deleted again.

**The guard covers 2 of the 7 sites.** The surrounding comment at `.github/workflows/ci.yml:250-253` and
`:281-282` reads as covering the mechanism. The other five — including the second Linux-only test,
`restore_and_empty_trash_fail_loudly_instead_of_reporting_false_success_when_the_dependency_panics` —
remain deletable with CI green.

A second, narrower gap in the same block: the **Windows arm of block 2** (`ci.yml:309-312`) is itself
zero-match-vulnerable, because `grep ... || true` asserts nothing. Every scenario that could exploit it also
reds the Linux leg, and `fail-fast: false` guarantees that leg runs — so the exposure is real but covered.

## Why it matters

This is the third layer of the same problem and each layer was found by asking the same question. The test
could skip silently (CPE-1806). The guard against that could be deleted silently (CPE-1806's first review).
Now the guard's *coverage* is narrower than its own description.

None of these is a behaviour bug. Each is a claim that reads stronger than the evidence behind it — which is
the defect class this repo has spent a sprint learning to find.

The comment is the part that makes it worth fixing rather than noting: a future reader deleting the routing
from one of the five uncovered sites will read "the mechanism is guarded", see green, and be wrong.

## What to do

- Extend the guard from 2 sites to all 7, or **narrow the comment to say exactly which two are covered and
  why the other five are not**. Either is honest; the current pairing is not. Prefer extending — the
  asymmetric block already exists and the marginal cost per site is small.
- Assert the `CPE-1268` notice on the **Windows arm** so it cannot pass by matching nothing in isolation.
- Note the ordering constraint: the two Linux-only tests do not compile off Linux, so any added canary needs
  the same explicit per-OS skip the existing blocks use. **An implicit skip here would be the fourth layer
  of the joke.**

## Evidence

Per the Evidence Rules in `Ticketing/wiki.md`. Red-proof **per site**: delete the routing from each newly
covered site in turn and confirm the guard reds for that one. A single deletion proving a single red does not
establish coverage of seven.

## Notes

Filed by the Foreman from the round-2 re-review of PR #961, 2026-08-20, which approved the PR — this was
explicitly out of scope for the round it asked for.

Related: **CPE-1806** (the routing and the guard), **CPE-1717** (`require_staged`), **CPE-1724** (the batched
routing of the remaining staging mechanisms), **CPE-1815** (the probe's collapsed failure causes).
