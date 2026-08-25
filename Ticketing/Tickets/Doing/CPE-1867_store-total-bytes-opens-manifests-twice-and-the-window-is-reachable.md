---
id: CPE-1867
title: store_total_bytes opens manifests/ twice, and the window between them is reachable
type: bug
priority: Low
status: Doing
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

- [x] `store_total_bytes` opens `manifests/` **once**. The shape the audit suggests: give `manifests_naming`
      a variant that returns the `read_dir` failure instead of falling back, and call that here.
- [x] The generous fallback must remain the default for `prune` — it is safe there and changing it would
      make a read failure delete blobs a manifest still names. Two call sites, two policies, one
      implementation: say plainly at both which direction each needs and why.
- [x] Keep CPE-1844's non-directory test (a file staged at `manifests/`), which is portable where a
      permission denial is not — both OSes disagree about staging that.
- [x] Red-proof the single-open version against the same racing-rename harness and report the worst `Ok`
      value over a comparable number of calls. A fix that narrows the window rather than closing it should
      say so with a number.
- [x] Assert the fixture is live before asserting the harm — that the rename actually landed during the
      call. CPE-1844's round-1 liveness claim inverted from 2-passed/9-failed to 9-passed/2-failed under a
      decoy-sibling trap, and three tests were certifying nothing.

## Work Log

**2026-08-23 — Investigated and fixed.**

**What can change in the window, and the consequence — established before choosing a fix.** The window
sits between `store_total_bytes`'s own probing `read_dir(manifests/)` and `manifests_naming`'s separate,
internal `read_dir(manifests/)`. Anything that removes or replaces `manifests/` between those two calls
(a rename, a delete, a remount) makes the first call see "readable" honestly and the second call fail —
which fires `manifests_naming`'s generous fallback ("all of them are named"), the answer that is safe for
`prune` and wrong here, because "all of them are named" is the maximal footprint and the maximal footprint
is what drives the byte-cap loop to delete checkpoints. The round-2 audit proved this reachable (not merely
theoretical) with a thread racing a rename: `worst Ok value under the race = 2000000000, errs = 0` out of
30,000 calls. So the honest answer is not "unreachable in practice" — the window is real, even though (per
the ticket's own "Why Low") it grants an attacker nothing beyond CPE-1844's already-disclosed bound.

**Fix.** Split `manifests_naming`'s scan out into `manifests_naming_strict`, which returns the `read_dir`
error instead of falling back, and made `manifests_naming` itself a one-line wrapper
(`manifests_naming_strict(..).unwrap_or_else(|_| wanted.clone())`) so `prune`'s generous policy is
unchanged and undisturbed. `store_total_bytes` now calls `manifests_naming_strict` directly — one
`read_dir`, not two — mapping `NotFound` to `Ok(0)` (a store that has never captured) and anything else to
`Err` (refuse, matching the pre-fix policy for an unreadable witness). Two call sites, two policies, one
scan, exactly as the acceptance criteria ask.

**Red-then-green.** Reproduced the audit's exact bug by temporarily reverting `store_total_bytes` to the
two-open shape (probing `read_dir`, then calling the generous `manifests_naming`) and running the new
`cpe_1867_a_racing_rename_of_manifests_never_returns_the_generous_directory_sum` test (20,000 calls, a
2 GB unnamed decoy blob, a thread racing `manifests/`'s rename):

```
RED (two-open, reverted):
thread '...cpe_1867_...' panicked: assertion `left != right` failed: HARM: a racing rename of
manifests/ made store_total_bytes count every blob file (the generous fallback), which is the
figure that deletes checkpoints
  left: 2000000029
 right: 2000000029
```

Reapplied the fix and reran the same test:

```
GREEN (single-open, the fix):
[CPE-1867] 20000 calls under a racing rename: worst Ok = 29, errs = 0,
Ok(0)-from-a-landed-race = 14477, toggles observed = 1300
test ...cpe_1867_a_racing_rename_of_manifests_never_returns_the_generous_directory_sum ... ok
```

`worst Ok = 29` is the honest, fully-witnessed total (never the 2,000,000,029 the two-open version
produced) — the fix **closes** the window rather than narrowing it: there is only one `read_dir` call left,
so there is no second call for a race to land in. 14,477 of the 20,000 calls landed the rename mid-call and
came back `Ok(0)` — proof the race pressure was real and was handled by the `NotFound` branch, not the
generous one.

Full `cargo test --lib snapshot_` run: 88 passed, 0 failed (includes CPE-1844's `store_total_bytes`
suite, CPE-1861's manifest-witness suite, and CPE-1871's `cpe_1871_an_undeletable_blobs_freed_bytes_still_count_as_progress`
pin, all still green — no pin weakened).

Kept CPE-1844's non-directory test (`cpe_1844_an_unreadable_manifests_dir_refuses_instead_of_counting_everything`)
unchanged; it still exercises the single-`read_dir`-fails-non-`NotFound` branch and stays green.

## Notes

Found by the independent Security Auditor during CPE-1844's round-2 re-audit, which returned MERGE and
classed this as recording-grade. Its own words: *closable in ~5 lines, and I'd file it.*

Read CPE-1844's Work Log first — it carries the opposite-failure-directions argument this ticket depends
on, and the measured cost of the witness (~16 ms for a 10,000-file tree under the shipped policy).

Related: CPE-1844 (the witness), CPE-1861 (`manifests_naming` itself), CPE-1863 (the byte-cap loop that
consumes this figure).
