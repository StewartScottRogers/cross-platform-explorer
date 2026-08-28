---
id: CPE-1970
title: merging on stale checks silently bypasses every guard that landed in between — measured on PR #1056, and `main` has no branch protection to prevent it
type: bug
priority: High
status: Open
tags: ready
estimate: M
created: 2026-08-27
---

## Summary

Found by CPE-1948's worker while tracing why one `RATCHETS.md` row was already stale.

**The measurement:**

| event | time |
|---|---|
| PR #1056 (CPE-1928)'s **last CI run** | 16:29Z |
| `ratchet-guard` job landed on `main` | 17:42Z |
| PR #1056 **merged** | 18:36Z |

#1056 legitimately added a new bidi render site (`text:blockedRemedy`), raising `bidi-render-registry`
1552 → 1553. **The `ratchet-guard` job never judged it** — the guard did not exist when #1056's checks
ran, and nothing re-ran them before the merge. The raise went in undeclared, and the doc's table has
been wrong ever since.

**This is not a hole in the guard's logic.** A guard is only ever as live as the newest CI run of the
PR it judges. The hole is in the merge procedure.

## Why it is High

**`main` has no branch protection at all.** Confirmed twice, independently, by two different agents:

```
GET /repos/…/branches/main/protection  -> 404 {"message":"Branch not protected"}
GET /repos/…/branches/main/rulesets    -> []
GET /repos/…/branches/main             -> {"name":"main","protected":false}
```

So there are **no required status checks**, and in particular **no "require branches to be up to date
before merging."** Nothing in the system prevents the #1056 shape from recurring, and `--admin` merges
bypass nothing because there is nothing to bypass.

**The blast radius is every guard this repo has.** The sprint of 2026-08-27 alone landed
`ratchet-guard`, `ci-verdict` (CPE-1956), the theme-parity checks (CPE-1962), the skip-notice guard,
and two `shellScriptLines` parser fixes (CPE-1936) that changed what **every** workflow scan can see.
Any PR merged on checks that predate one of those was never judged by it. **The Foreman merged several
PRs this shift on runs that predated later merges** — including two whose only red was verified from an
older run.

**And the failure is silent in both directions**: nothing marks the merged commit as unjudged, and the
guard's next run is against a `main` that already contains the unjudged change, so it measures the new
value as the baseline and reports green forever.

## Acceptance criteria

- [ ] **Establish the real exposure before fixing.** Enumerate merges where the merged commit's newest
      check run predates a guard-adding commit on `main`, over a window worth reporting. `gh api` has
      both timestamps. **A count, not an anecdote** — and if the answer is "one", that is a real result
      and changes the priority.
- [ ] **Decide the remedy and argue it.** Branch protection with *require branches to be up to date*
      is the obvious answer and it is a **repository-settings change, not a code change** — so it needs
      the user, and this ticket should say exactly which settings and what they cost (every PR needs a
      rebase before merge; on a 3-OS matrix that is real wall-clock). The cheaper alternative is a
      pre-merge check the Foreman runs: compare the PR's newest check-run timestamp against
      `main`'s newest guard-touching commit and refuse if it is behind.
- [ ] **Whatever is chosen, make the bypass visible rather than impossible.** *"This PR was judged by a
      guard set 2 hours older than `main`'s"* printed at merge time is worth more than a rule that gets
      turned off the first time it is inconvenient.
- [ ] **`scripts/ci-poll.mjs` is the natural home** — it already resolves the rollup, and after
      CPE-1906 (PR #1078) it has distinct verdicts for did-not-run and could-not-ask. A **stale-checks**
      verdict fits the same vocabulary. Coordinate with that PR rather than colliding.
- [ ] **Fix `bidi-render-registry`'s undeclared raise** as part of this, or say why it is left: 1552 →
      1553 from #1056 (CPE-1928)'s `text:blockedRemedy`. It is legitimate work that was never declared,
      so it needs a `RATCHETS.md` licence row naming CPE-1928 retroactively.
- [ ] Check whether the same shape can bypass `gui-smoke/known-failing.json`'s ratchet, the hex ratchet,
      and the eight allowlists — they are all measured against a merge base, which is the same
      assumption.

## Notes

Filed 2026-08-27 by the sprint Foreman from CPE-1948's worker (PR #1081), which traced a single stale
table row back to a merge-procedure gap rather than accepting the number as drift.

The `main`-unprotected fact was found separately by PR #1074's Reviewer while asking what that PR
actually bought, and confirmed again here. Both halves point at the same missing control.

Related: **CPE-1948** (the doc guard, PR #1081 — where this was found), **CPE-1928** (PR #1056, the
undeclared raise), **CPE-1934** (the ratchet-guard job this bypassed), **CPE-1906** (PR #1078,
`ci-poll.mjs`'s verdict vocabulary), **CPE-1956** (PR #1074, where `main`-unprotected was first
measured).

## Evidence sharpened 2026-08-27 — confirmed independently, and there is a better instrument

PR #1081's Reviewer re-checked all three timestamps via `gh api` and confirmed them exactly:
run **33093506408** created `16:29:53Z`, `ratchet-guard` landed at `17:42:59Z` (commit `47cb1240`,
PR #1052), #1056 merged `18:36:20Z`.

**Two precision corrections to the summary above:**

- *"last CI run was 16:29Z"* is the **created-at**. That run **finished at 18:35:13Z** — one minute
  before the merge. So the merge was not made on an obviously-ancient run; it was made on a run that
  had just completed, which is exactly why nobody noticed. **A recency check on the run's finish time
  would not have caught this.**
- The **GUI smoke** workflow *was* re-run at `17:47:13Z` (attempt 2, after the guard landed). That did
  not help, because `ratchet-guard` lives in **`ci.yml`** — a partial re-run re-judges only the
  workflow you re-ran.

**A stronger instrument than timestamps, and the one to build the fix on:** `ratchet-guard` does not
appear in that run's job list at all (14 jobs, none of them it), and **`ratchet-guard` is absent from
`ci.yml` at #1056's head SHA `1b5c6651` — grep count 0.** So it *could not* have judged that PR. That
is a definite answer where a timestamp comparison is only an inference.

Generalised: **ask whether the guard's job name appears in the PR's own rollup**, and whether the
guard's definition exists at the PR's head SHA. Both are one API call or one `git cat-file` and neither
depends on clock reasoning.

**The same instrument settled a live question this shift.** Four open PRs showed a `GUI smoke shard 2`
red after CPE-1960's fix merged, which would have meant the fix did not work. `git cat-file -e
<head-sha>:gui-smoke/lib/scrollIntoView.ts` returned **not-found on all four branches and found on
`main`** — so the branches simply predate the fix, decisively, in one command. Reading four job logs
would have suggested the same thing without proving it.
