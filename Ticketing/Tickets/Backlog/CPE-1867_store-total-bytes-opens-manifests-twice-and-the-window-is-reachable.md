---
id: CPE-1867
title: store_total_bytes opens manifests/ twice, and the window between them is reachable
type: bug
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-22
closed:
---

## Problem

`store_total_bytes` (CPE-1844) checks that `manifests/` is readable, then calls `manifests_naming` to
build the witness. Two opens, one window.

`manifests_naming` is deliberately **generous** — if it cannot read the directory it returns "all of them
are named", which is the safe direction for `prune` (generous means *don't delete*) and the **wrong**
direction here (generous means *delete*). CPE-1844's pre-check exists precisely to split those two, and it
holds against every deterministic shape.

But the window between the two opens is **reachable, not theoretical**. Measured by the independent
Security Auditor with a thread renaming `manifests/` away and back:

```
worst Ok value under the race = 2000000000, errs = 0     (out of 30,000 calls)
```

That is the full directory sum — CPE-1844's round-1 behaviour, which the same audit proved destructive.

## Why Low

It grants an attacker nothing they do not already have: anyone who can rename `manifests/` can simply
write the 122-byte witness manifest instead, which is the residual CPE-1844 discloses and bounds. The
existing note predicts this case and bounds it exactly right — *at most the directory sum*.

So this is closing a disclosed window, not a live hole.

## Acceptance criteria

- [ ] `store_total_bytes` opens `manifests/` **once**. The shape the audit suggests: give `manifests_naming`
      a variant that returns the `read_dir` failure instead of falling back, and call that here.
- [ ] The generous fallback must remain the default for `prune` — it is safe there and changing it would
      make a read failure delete blobs a manifest still names. Two call sites, two policies, one
      implementation: say plainly at both which direction each needs and why.
- [ ] Keep CPE-1844's non-directory test (a file staged at `manifests/`), which is portable where a
      permission denial is not — both OSes disagree about staging that.
- [ ] Red-proof the single-open version against the same racing-rename harness and report the worst `Ok`
      value over a comparable number of calls. A fix that narrows the window rather than closing it should
      say so with a number.
- [ ] Assert the fixture is live before asserting the harm — that the rename actually landed during the
      call. CPE-1844's round-1 liveness claim inverted from 2-passed/9-failed to 9-passed/2-failed under a
      decoy-sibling trap, and three tests were certifying nothing.

## Notes

Found by the independent Security Auditor during CPE-1844's round-2 re-audit, which returned MERGE and
classed this as recording-grade. Its own words: *closable in ~5 lines, and I'd file it.*

Read CPE-1844's Work Log first — it carries the opposite-failure-directions argument this ticket depends
on, and the measured cost of the witness (~16 ms for a 10,000-file tree under the shipped policy).

Related: CPE-1844 (the witness), CPE-1861 (`manifests_naming` itself), CPE-1863 (the byte-cap loop that
consumes this figure).
