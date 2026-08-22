---
id: CPE-1860
title: the pdfium fetch sites sit inside their cap but below the margin the new guard requires
type: task
priority: Low
status: Backlog
tags: ready
estimate: S
created: 2026-08-22
closed:
---

## Problem

CPE-1849 added an arithmetic assertion to `src/lib/releaseHangHardening.test.ts`: for each guarded fetch
site, `calls x worstCasePerCall < timeoutMinutes x 60 x 0.9`, where `worstCasePerCall` is
`retry-max-time + retry-delay + max-time` — the corrected formula, after measurement showed the
retry-delay term had been omitted through two tickets.

It was **not** extended to `ci.yml`'s three pdfium sites, and the reason is not scope. With the corrected
formula those sites are **333s against a 360s cap — a 7.5% margin**, below the guard's 10% threshold.

They are **inside the cap and therefore not broken**. But folding them into the guard as it stands would
red it on work nobody did, and loosening the threshold until they passed would turn a real check into a
rubber stamp.

## The second, more interesting half

The **10% margin fraction is a judgement, not a measurement.** CPE-1849's worker was explicit about this:
it is argued from the steps' non-curl work being milliseconds, not derived from run history the way
CPE-1824 sized its `timeout-minutes` caps against real CI durations.

And that judgement is exactly what excludes the pdfium sites. So the question "should these sites be
guarded" and the question "is 10% the right number" are the same question, and neither has data behind it
yet.

## Acceptance criteria

- [ ] Derive the margin fraction from run history rather than argument — the same method CPE-1824 used to
      size the caps (`gh api` job/step durations across recent runs). Report what the non-curl work in
      these steps actually costs.
- [ ] With a measured fraction, decide the pdfium sites: either they pass and are folded into the guard,
      or they need their `--retry-max-time` or `timeout-minutes` adjusted first. Do not adjust the
      threshold to fit them.
- [ ] If the sites are changed, re-run a positive control through the modified flags — exit code, elapsed
      time, byte count — not just a parse check.
- [ ] Remove CPE-1849's exclusion note once the sites are either guarded or explicitly recorded as
      permanently out.
- [ ] Check whether any other guarded site is close to the threshold; a margin that only one site fails is
      worth knowing about before it becomes two.

## Notes

Filed from CPE-1849's round 2, where the worker declined to fold these in and gave its reasoning rather
than calling it scope. That reasoning is the ticket: *"loosening the threshold until they passed would
make a finding into a rubber stamp."*

Worth knowing before starting: CPE-1849's review established by experiment that curl checks the retry
timer **before** sleeping `--retry-delay`, so the real per-call bound is
`retry-max-time + retry-delay + max-time`. A case the old formula bounded at 6s measured 7.9s. Both
CPE-1824 and CPE-1849 had the formula wrong because both scaled experiments used `--retry-delay 1`, too
small a term to expose it.

Related: CPE-1824 (the caps and the original guard), CPE-1849 (the corrected formula and the arithmetic
assertion).
