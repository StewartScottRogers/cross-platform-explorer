---
id: CPE-1970
title: merging on stale checks silently bypasses every guard that landed in between — measured on PR #1056, and `main` has no branch protection to prevent it
type: bug
priority: High
status: In Progress
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

## Work Log

### 2026-08-28 — worked to a PR

**1. Exposure, measured before anything was built.**
Window **2026-08-14T00:00:00Z → 2026-08-28T11:05:17Z** (14 days). Denominator **186** — every PR
merged into `main` in that window (`gh pr list --state merged`, whose 200-row cap is comfortably
outside the window: the oldest row returned merged 2026-08-13).

*"Guard-adding commit" was replaced by a stronger, definite question*, following this ticket's own
"Evidence sharpened" section: instead of dating guards and comparing clocks, ask whether a job `main`
already required produced **any check at all** on the PR's board. Both sides derived at run time —
the board from `gh pr view N --json statusCheckRollup`, the required set from `git show
<squash-commit>^:.github/workflows/*.yml` parsed by the shipped `scanWorkflowJobs`.

| outcome | PRs |
|---|---|
| clean | **168** |
| **exposed — a required job produced no check at all** | **16** |
| unreadable (#896, #899 — squash commits unreachable from `main`); counted fail-closed, not clean | **2** |

Absent job → merges it could not judge: `ratchet-guard` ×5 (#1053, #1055, #1057, #1054, **#1056**),
`ci-verdict` ×5 (#1073, #1075, #1076, #1077, #1078), `lockfile-preflight` ×2 (#1048, #1049), `msrv` ×2
(#1030, #1032), `ffmpeg-pin-guard` ×1 (#955), `gui-smoke-linux` ×1 (#921). **#1056 reproduces
independently.** 14 of the 16 fall in the window's last 36 hours — this shift's own merges, exactly as
the ticket says.

*A cheaper first instrument is recorded because it was wrong.* Comparing the PR head's workflow files
against `main`'s (pure git) also produced 16, but a different 16: it missed #921 and wrongly flagged
**#1039**, whose head tree lacked `lockfile-preflight` yet whose board carried the check — GitHub
builds `pull_request` checks from the **merge ref**, so `main`'s newer workflow can apply to a branch
that does not contain it. The rollup is decisive; the git tree is a hypothesis about it. The shipped
check uses the rollup.

**2. Remedy, argued — and it needs the user.** `docs/design/CI-STALENESS.md` §2 is the decision-ready
section: the exact ruleset items (required checks, **require branches to be up to date**, do-not-allow-
bypassing, which the crew's `--admin` merges currently walk past), the suggested required-check list
derived from `main`'s PR-triggered jobs, and the cost measured on this repo: **13.0 merges/day**,
**2.90 other PRs open at the average merge** (max 10), successful `ci.yml` **median 65.9 min / p90
108.2 min** (196 completed runs in the window). Consequence: merging becomes serial at one run apiece —
a queue of five drains in **~5.5 h**, the ceiling is **~21 merges/day at median and 13 at p90**, and the
crew already does 13.0. Recommendation is required-checks + up-to-date **plus a merge queue**, which
restores most of the throughput; §2d flags the trap that `ci.yml`'s `paths-ignore` turns a required
check into a permanent *pending* on ticket-only PRs.

**3. Shipped, because the settings change may never come.** `scripts/ci-poll.mjs` gains exit **5**:
`completed stale-checks` (a job `main` requires produced no check on this board) and `completed
coverage-unknown` (the check could not be computed — "did not run" is not "found nothing"). Every
verdict line, green ones included, now carries `coverage=ok | N-unjudged | unknown | n/a(<reason>)`,
appended after `gh_failures=` so the pinned key order is untouched. The required set is read from
**`origin/main`** via `git show`, not the working tree — a Worker's worktree *is* the PR branch, so
reading it would have reproduced the defect from inside the guard.

*Precision choice, argued at the site and in the doc:* "`main` moved at all" was rejected (593 commits
in 14 days, ~42/day — it fires on nearly every merge and gets aliased away); "the newest run finished
before the guard landed" was rejected by this ticket's own evidence (#1056's run finished 67 s before
the merge). *What it misses, stated:* a guard added **inside** an existing job — a new `.test.ts` under
`Frontend`, a new ratchet under `Ratchet guard`, CPE-1936's parser fix — is invisible to any name-based
instrument, which is why §2 is still the ask. Also a locally stale `origin/main` (the poll does not
fetch; the ref+SHA is printed and `sprint.md` now says to fetch first), and a workflow that contributed
nothing (the `paths-ignore` carve-out; those workflows are **named** on stdout rather than dropped).

*Noise, measured:* swept over the 184 evaluable merges — **168 clean, 16 firings**, all 16 naming a job
that still exists on `main` today, so **0** deletion/rename false positives.

**4. Red-proof, both directions, on exit code and stdout.** New `guard-gap` stub mode emits **one
identical rollup**; only `main` changes, via two fixture dirs asserted to differ by exactly one job:
`workflows-base-ahead/` → **exit 5**, `completed stale-checks`, guard named, `coverage=1-unjudged`;
`workflows-base/` → **exit 0**, `completed success`, `coverage=ok`. Plus unreadable base and empty base
→ exit 5 `coverage-unknown`; a red board stays exit 1 even with an unreadable base; `--run` →
`coverage=n/a(run-mode)`; pending → `coverage=n/a(board-pending)`. A CPE-1950 derivation leg reads
**this repo's real `.github/workflows`** through the shipped functions and reconstructs #1056 against
the real `ratchet-guard` label read out of `ci.yml`, so the fixture pair cannot rot into agreeing with
itself.

**CPE-1929's two green sabotages**, run by hand and written at the site: disabling the rung → **3
failed / 60 passed**; forcing `coverageOf` to always answer `ok` → **7 failed / 56 passed**. Different
tests, both red — nothing earlier in the ladder shadows it.

**5. The other ratchets** — `gui-smoke/known-failing.json`, the hex ratchet and the eight allowlists are
all measured by `ratchet-baselines.mjs` under the **one** `ratchet-guard` job, so a board without that
job bypasses all of them at once. That is one hole, not eight, and it is the one exit 5 refuses (5 of
the 16). Recorded in `docs/design/CI-STALENESS.md` §4.

**`bidi-render-registry`'s 1552 → 1553 is deliberately NOT backdated.** CPE-1948 already settled this in
`docs/design/RATCHETS.md`: a licence row is consumable only from inside the diff that performs the
raise, so a retroactive row would be the only row on that page false by the page's own definition. The
movement is recorded in that doc's recount, which names this ticket as the fix. The baseline today is
**1555** with a CPE-1925 row covering 1553 → 1555.

**No in-app doc change.** `src/docs/*.md` + `sectionDocs.ts` cover user-facing app **sections**; this is
CI/merge procedure with no app surface, so there is no `Section` to map.

**Gate table, re-run after the final edit.**

| check | result |
|---|---|
| `npm run check` (svelte-check + tsc) | 0 errors, 0 warnings |
| `npx vitest run` (whole root suite) | **358 files, 5283 passed, 2 skipped** |
| the 2 skipped | both in `catalogPublishLoudFailure.test.ts`, gated on `jq`, which is not installed on this machine |
| `src/lib/ciPollFailClosed.test.ts` alone | 63 passed, 0 failed, 0 skipped (46 before this ticket) |
| Rust | untouched — no crate changed |
