---
id: CPE-1893
title: the signed agent catalog has not published since 2026-07-25, because `catalog` needs a job that always fails
type: bug
priority: High
status: Backlog
tags: ready
estimate: S
created: 2026-08-26
---

## Summary

`release.yml`'s `catalog` job is declared `needs: release` with **no `if:` condition**. `release` has
failed on every tagged run since 2026-07-26, so `catalog` has been reported `skipped` on all ~15 of
them. Net effect, unnoticed for a month: **the signed agent-catalog bundle has not been published
since 2026-07-25.**

This is a distinct failure from the one CPE-1874 covers. CPE-1874 is about the *verification step*
being dead. This is about a *publishing* job that has silently produced nothing for 31 days because
it was chained behind that dead step. Nobody flagged it because `skipped` reads as benign in the
run summary — it is the same shape as a path-filtered job that had nothing to do.

The catalog pipeline itself is built and activated (CPE-308), and the signing key is configured, so
this is not a "never finished" — it is a working feature that stopped shipping and said nothing.

Found 2026-08-26 by CPE-1873's independent Security Auditor while establishing whether that PR's
green depended on the broken release job. It did not; this turned up alongside.

## Acceptance criteria

- [ ] Determine what consumers do when the catalog goes stale for a month — does the app fall back,
      pin, or silently serve nothing? Record the answer; it decides whether this is High or Critical.
- [ ] Decouple `catalog` from `release`'s success where that is correct, or make the skip **loud**.
      A publishing job that produces nothing must not report as `skipped` indistinguishably from a
      job that had nothing to do.
- [ ] Add a freshness check that fails when the published catalog is older than a chosen threshold,
      so the next month-long gap surfaces on its own rather than waiting for an auditor.
- [ ] Red-proof it: arrange the failing-`release` condition, observe the new signal fire, restore.
- [ ] Do not fix `release.yml`'s underlying verify failure here — that is CPE-1872 / CPE-1874.

## Notes

Evidence: last successful `release.yml` run was `v0.57.33` on 2026-07-25. Every run from
`v0.57.35-sidecar` (2026-07-26) through `v0.57.69` / `v0.57.69-sidecar` (2026-08-23) failed on all
three legs at `Verify updater manifest + signatures (CPE-1058)`. Confirmed on runs `30219127836`,
`31133248284`, `32645894722`, `32645968177`.

Note the corrected window: the outage is **31 days from 2026-07-26**, not the 27-days-from-2026-08-04
figure carried in earlier tickets and in the prior run's checkpoint. Correct that where it appears.

Related: **CPE-1874** (the releases that shipped without their signatures verified), **CPE-1872**
(the redesigned `verify-published-manifest` job — note it merged 2026-08-24, *after* the newest tag,
so it has **never executed** and is entirely unexercised in production), **CPE-308** (the catalog
auto-update pipeline this job publishes for).
