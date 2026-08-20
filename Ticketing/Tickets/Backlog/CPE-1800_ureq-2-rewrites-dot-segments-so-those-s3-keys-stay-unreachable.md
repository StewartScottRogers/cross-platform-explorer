---
id: CPE-1800
title: ureq 2 rewrites dot-segments, so those S3 keys stay unreachable — migrating to ureq 3 is the only fix
type: task
priority: Medium
status: Backlog
tags: big-design
estimate: L
created: 2026-08-20
closed:
---

## Decision needed

S3 keys containing dot-segments — `a/../b`, `a/./b` — are **legal, distinct keys**, and this app cannot
reach them. CPE-1721 established exactly why, by measurement rather than from documentation, and the
finding is that **no fix exists short of a dependency major migration**.

This ticket exists to put that decision in front of a human rather than have a worker take it inside a
sprint.

## What was measured

A raw-socket probe was built to observe what actually goes on the wire, across two major versions:

- **ureq 2 rewrites** `a/../b`, `a/./b` **and** `a/%2e%2e/b`. It writes `unit.url.path()` from an
  already-normalised `url::Url`, and there is no hook to intercept it.
- **ureq 3 sends all seven tested shapes byte-identical.** It uses `http::Uri`, which is
  syntax-only — no normalisation.

The probe also **kills the obvious cheap fix**: WHATWG treats `%2e%2e` as a dot segment, so `url`
normalises it too; and the one spelling that survives, `%252e%252e`, decodes at S3 to a *different*
key. **There is no encoding of `.` that `url` passes through and S3 decodes back.**

So the options are exactly two: migrate to ureq 3, or accept that these keys remain unreachable.

## The cost, stated honestly

Migrating is not a version bump. It is a **major migration of the whole request layer**, and it brings
in a new dependency family (`http`) — against this repo's standing no-new-dependencies guardrail. That
is why the CPE-1721 worker deliberately did **not** do it: the fix is proven viable, and the call is
not a worker's to make inside a sprint slot.

## What CPE-1721 shipped instead

The measurement is recorded in the module doc, against the evidence rather than as an assertion. And
one real harm was folded in: `list` no longer hands back a browsable folder whose every child it would
then refuse to open — a listing that advertises objects the app cannot reach is worse than one that
does not show them.

## Deciding it

Weigh:

- **How common are dot-segment keys in practice?** They are legal and some tools generate them, but a
  bucket full of them is not the common case. If nobody hits this, the migration buys little.
- **What else would ureq 3 bring?** A major migration is a good moment to gain things, not only to
  pay. Check whether it fixes or simplifies anything else in the network stack — that changes the
  arithmetic.
- **The guardrail is a default, not a law.** `http` is not a frivolous dependency; the question is
  whether this justifies it.

If the answer is "not now", say so **in the module doc next to the measurement**, so the next person to
find an unreachable key finds the reasoning rather than rediscovering it.

## Notes

Filed by the Foreman from PR #955 (CPE-1721/CPE-1722), 2026-08-20. The worker escalated rather than
either doing the migration unilaterally or quietly leaving the bug undocumented — the right call, and
the measurement it produced is what makes this decidable at all.

Related: **CPE-1721** (the investigation), **CPE-1722** (the path-grammar half, fixed),
**CPE-1736** (the S3 fixture's inability to serve these key shapes).
