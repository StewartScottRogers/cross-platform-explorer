---
id: CPE-1713
title: Two timeout tests wait out their real 60s deadline, adding ~6 minutes to every CI run
type: task
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-13
closed:
---

## Problem

Raised independently by **both** the reviewer and the UAT on PR #892 (CPE-1706), and flagged by each as a
recommendation rather than a gate. Filed rather than fixed there.

CPE-1706 ships one test per crate that drives the **real `connect()`** against a dribbling fixture and
waits out the full shipped 60-second deadline. Measured effect on the green path:

| Crate | Before | After |
|---|---|---|
| `cpe-s3` | 1.20 s | **60.02 s** |
| `cpe-webdav` | 0.61 s | **60.01 s** |

That is **+120 s per OS × 3 OSes ≈ 6 minutes of CI wall clock on every run, forever.**

## Why it was done this way, and why that reasoning is sound

Do not "optimise" this by reverting to an injected duration and calling it done. **That is exactly what let
round 1 ship a configuration that did not bound anything.** Round 1's tests proved the mechanism through a
seam while the shipped values left the dribble hole wide open — the tests passed and the product was broken.

CPE-1706's own round-1 code comment argued *against* waiting out a shipped 30 s because it "would cost 30 s
of wall clock in every CI job on three OSes". Round 2 then paid 4× that cost knowingly, because the cheaper
approach had already failed once. That history is the point: the expensive test exists because the cheap one
lied.

## The narrower gap worth closing

The scaled-duration test and the value-pin assertion together already prove two of the three things:

1. **the mechanism works** — the scaled test, with a short injected `Duration` through the same
   `build_agent` → `req.call()` path;
2. **the value is sane** — `the_shipped_timeout_values_are_finite_and_within_sane_bounds`, which bites at
   5 s / 120 s and 400 s / 3600 s, so an absurd constant reds.

What they leave open is only (3): **is the constant actually wired to the call site?** Both checkers
independently proposed the same cheap closure — a **field assertion** on the deadline the provider actually
carries (e.g. `S3Provider::connect(..).request_deadline`, compared in microseconds), which catches a
disconnected constant without waiting for it to elapse.

## Acceptance criteria

- [ ] CI wall clock for `cpe-s3` and `cpe-webdav` returns to roughly its pre-CPE-1706 level.
- [ ] **All three properties stay pinned** — mechanism, value, and wiring. Removing any one of them must
      turn a **distinct** test red. Write down which test covers which property.
- [ ] **Prove the replacement catches what the slow test caught.** Disconnect the constant from the call
      site (pass a different duration, or drop the `.timeout()` call) and show the new assertion red. If it
      does not catch that, the slow test stays — a faster suite is not worth re-opening the hole.
- [ ] Confirm nothing can hang CI. libtest has no per-test timeout, so any test whose regression mode is an
      unbounded wait rather than a red is unacceptable (this is CPE-1706 item 5's rule).
- [ ] Report the before and after wall clock per crate, measured, not estimated.

## Notes

Filed by the Foreman from the PR #892 review and UAT, 2026-08-13. Low priority: it costs time, not
correctness, and the current state is safe. **If the cheap replacement cannot be proven to catch a
disconnected constant, close this ticket as won't-fix and keep the 6 minutes** — that is a perfectly good
outcome and better than trading real coverage for build speed.

Related: **CPE-1706** (which added them and argued the trade), **CPE-1683** (the earlier timeout-adjacent
guard work), and the Evidence Rules in `Ticketing/wiki.md`.
