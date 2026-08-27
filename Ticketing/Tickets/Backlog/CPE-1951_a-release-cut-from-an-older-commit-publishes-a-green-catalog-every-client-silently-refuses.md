---
id: CPE-1951
title: a release cut from an **older** commit publishes a fully green catalog that every client silently refuses — the floor is a static ratchet, not a monotonic one
type: bug
priority: Medium
status: Open
tags: ready
estimate: M
created: 2026-08-27
---

## Summary

CPE-1941 (PR #1061) fixed the old-tag republish by deriving each catalog entry's `version` from the
**tagged commit's committer timestamp** instead of a publish-time clock. That is right, and it
introduces a consequence the PR does not close: **the version now tracks commit order, not release
order.**

Cut a release from a commit older than the last released one — a hotfix on a maintenance branch, a
revert branch, `git tag` on a non-tip commit, or re-cutting an earlier tag to fix an asset — and you
stamp a *lower* version.

Concretely: `v0.57.70` from `%ct = 1787200000`, then `v0.57.71` off an older base at `1787100000`.

**What gets published:** a fully valid bundle. The floor check passes (well above `1787000000`), the
future check passes, index and per-manifest signatures verify, sha256 binds, `gh release upload`
succeeds. **The job is green.**

**What clients do:** every entry returns `ApplyOutcome::Rollback`. Nothing is written. Nothing is
logged as a release failure.

**Found independently by both of PR #1061's gates** — the Security Auditor and the Reviewer, neither
seeing the other's report. Under the old `date +%s` scheme this failure mode did not exist.

## Why it is silent

Nothing on the publish side compares the derived version against the **last published** version. The
floor (`CATALOG_VERSION_FLOOR`) is a **static** ratchet — it only rejects versions below a fixed
constant — not a monotonic one.

Detection today is `catalog-freshness.yml`'s 14-day `now - version` alarm, which only fires if the
older base commit is already past the threshold. **A hotfix off a three-day-old base stays green
while every client refuses it**, indefinitely.

It is an **availability** failure, not a security one — nothing unsafe is accepted — but it is the
kind that surfaces as "why has nobody's agent catalog updated in a month".

## Mitigating today, and why that is not enough

`scripts/release.ps1:515` does `git push origin HEAD --tags`, so tags are cut from the current HEAD
and the linear process cannot hit this by accident. It bites a **deliberate** off-tip tag — which is
exactly what someone reaches for under pressure, during a hotfix, when they are least likely to
notice a silent client-side refusal.

## The fix shape, from CPE-1941's author (who was asked to pick, and argued for it)

**Take the index-fetch lower-bound check, not a `LAST_PUBLISHED_VERSION` counter:**

1. A committed counter must be bumped by something. Auto-bumping means the release job commits back
   to the repo from a detached tag checkout; manual means it rots — and **a stale counter degrades
   into exactly the static ratchet we already have.**
2. **The fetch is not the trust dependency CPE-1924 rejected.** That objection was about *trusting*
   fetched content to decide what to publish. This uses it only as a **lower bound that fails the
   build** — a hostile or garbage response can cause a false *failure*, never a false success. It
   fails closed, so it needs no signature verification to be safe, and the job already runs
   `gh release upload` against the same host, so it is not even a new egress class.
3. It closes the legacy window in the forward direction too: if an old-tag re-run ever stamped a
   large `date +%s`, the next real release would fail **loudly** instead of being silently refused
   everywhere.

**Implementation note from the same author:** resolve what clients resolve —
`releases/latest/download/catalog-index.json` — and handle the draft/`latest` distinction. During a
release run `latest` is still the *previous* release, which is exactly the bound wanted.

## Acceptance criteria

- [ ] Add a publish-time lower-bound check against the last published catalog index. It must be
      **fatal**, and it must fail closed on any fetch error — never skip the check because the fetch
      did not work. This repo has lost 27 days to a broken release workflow; a check that fails
      *silently* here is worse than the bug.
- [ ] **Demonstrate the bug first**: tag an older commit in a fixture, publish, and show a green job
      producing entries every client refuses. Assert on the client outcome and the on-disk state, not
      on a verdict enum.
- [ ] Then show it refused after the change, **and** a legitimate newer release still accepted. Both
      directions.
- [ ] Handle the `latest`/draft distinction explicitly — PR #1061's review found `v0.57.32` is a
      **draft**, never served to any client, which is exactly the sort of thing this check will trip
      over. Say what happens when `latest` has no catalog index at all — which is true **right now**:
      `/releases/latest/` resolves to `v0.57.69-sidecar`, and only the plain channel runs the
      `catalog` job at all, so the live index URL 404s (confirmed 2026-08-27 by
      `catalog-freshness.yml` run 33107498544 → issue #1062).
      **Record correction (CPE-1953):** the last release that actually PUBLISHED a catalog index is
      **v0.57.33**, not v0.57.32. An asset scan says v0.57.32 only because v0.57.33's release was
      later deleted; v0.57.32 is itself a draft no client ever fetched. The last successful
      `release.yml` run was v0.57.33 on **2026-07-25**.
- [ ] Keep `CATALOG_VERSION_FLOOR`. The static floor and the monotonic bound answer different
      questions and both are wanted.
- [ ] Red-proof the fetch failure path: a 404, a truncated body, a 500, and a timeout must each fail
      the build with a distinct message.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1061's Security Auditor (F1) and Reviewer (F2), which
found it independently of each other, plus the author's recommended shape.

Related: **CPE-1941** (the versioning change, PR #1061), **CPE-1940** (the fail-closed baseline, PR
#1058 — the index-fetch work would build on its `VerifiedIndex`), **CPE-1924** (where an index fetch
was first costed and rejected, for a different reason), **CPE-308** (the catalog pipeline),
**CPE-1953** (no release since **v0.57.33** publishes a catalog index at all — corrected there from
v0.57.32).
