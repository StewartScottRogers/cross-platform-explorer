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
| a required job produced no check at all | **16** — of which **15 exposure, 1 rename noise** (see round 2) |
| unreadable (#896, #899 — squash commits unreachable from `main`); counted fail-closed, not clean | **2** |

Absent job → merges it could not judge: `ratchet-guard` ×5 (#1053, #1055, #1057, #1054, **#1056**),
`ci-verdict` ×5 (#1073, #1075, #1076, #1077, #1078), `lockfile-preflight` ×2 (#1048, #1049), `msrv` ×2
(#1030, #1032), `ffmpeg-pin-guard` ×1 (#955), `gui-smoke-linux` ×1 (#921 — **reclassified in round 2 as
the PR renaming its own job, not an unjudged merge**). **#1056 reproduces independently.** 14 of the 15
genuine firings fall in the window's last 36 hours — this shift's own merges, exactly as the ticket
says.

*A cheaper first instrument is recorded because it was wrong.* Comparing the PR head's workflow files
against `main`'s (pure git) also produced 16, but a different 16: it missed #921 and wrongly flagged
**#1039**, whose head tree lacked `lockfile-preflight` yet whose board carried the check — GitHub
builds `pull_request` checks from the **merge ref**, so `main`'s newer workflow can apply to a branch
that does not contain it. The rollup is decisive; the git tree is a hypothesis about it. The shipped
check uses the rollup.

**2. Remedy, argued — and it needs the user.** `docs/design/CI-STALENESS.md` §2 is the decision-ready
section: the exact ruleset items (required checks, **require branches to be up to date**, do-not-allow-
bypassing, which the crew's `--admin` merges currently walk past), the suggested required-check list
derived from `main`'s PR-triggered jobs, and the cost measured on this repo: **12.86 merges/day**,
**2.90 other PRs open at the average merge** (max 10), successful `ci.yml` **median 60.9 min / p90
81.8 min** (417 successful of **790 completed** runs in the window, paged to exhaustion — round 1
reported 65.9/108.2 off a truncated 196-run list; see round 2). Consequence: merging becomes serial at
one run apiece — a queue of five drains in **~5.1 h**, the ceiling is **~23.6 merges/day at median and
17.6 at p90** against 12.86 actual. Recommendation is required-checks + up-to-date **plus a merge
queue**, which restores most of the throughput. (Round 1's §2d claimed `ci.yml`'s `paths-ignore` would
turn a required check into a permanent *pending* on ticket-only PRs; **that is false of this repo** and
§2d was rewritten — see round 2.)

**3. Shipped, because the settings change may never come.** `scripts/ci-poll.mjs` gains exit **5**:
`completed stale-checks` (a job `main` requires produced no check on this board) and `completed
coverage-unknown` (the check could not be computed — "did not run" is not "found nothing"). Every
verdict line, green ones included, now carries `coverage=ok | N-unjudged | unknown | n/a(<reason>)`,
appended after `gh_failures=` so the pinned key order is untouched. The required set is read from
**`origin/main`** via `git show`, not the working tree — a Worker's worktree *is* the PR branch, so
reading it would have reproduced the defect from inside the guard.

*Precision choice, argued at the site and in the doc:* "`main` moved at all" was rejected (**589**
commits in the window, ~41/day — it fires on nearly every merge and gets aliased away); "the newest run finished
before the guard landed" was rejected by this ticket's own evidence (#1056's run finished 67 s before
the merge). *What it misses, stated:* a guard added **inside** an existing job — a new `.test.ts` under
`Frontend`, a new ratchet under `Ratchet guard`, CPE-1936's parser fix — is invisible to any name-based
instrument, which is why §2 is still the ask. Also a locally stale `origin/main` (the poll does not
fetch; the ref+SHA is printed and `sprint.md` now says to fetch first), and a workflow that contributed
nothing (round 1 excused every such workflow; round 2 narrowed the excuse to workflows whose own
`pull_request:` trigger is path-filtered — excused silences are still **named** on stdout).

*Noise, measured:* swept over the 184 evaluable merges — **168 clean, 16 firings**. Round 1 called all
16 genuine "because all 16 name a job that still exists on `main` today"; **that test keyed on the job
id while the matcher keys on the label**, and round 2's re-classification gives **15 genuine, 1 rename
(#921)** — a true rename rate of 1 in 16 firings (6.3%) / 1 in 184 merges (0.54%).

**4. Red-proof, both directions, on exit code and stdout.** New `guard-gap` stub mode emits **one
identical rollup**; only `main` changes, via two fixture dirs asserted to differ by exactly one job:
`workflows-base-ahead/` → **exit 5**, `completed stale-checks`, guard named, `coverage=1-unjudged`;
`workflows-base/` → **exit 0**, `completed success`, `coverage=ok(1-silent)`. Plus unreadable base and empty base
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
the 15). Recorded in `docs/design/CI-STALENESS.md` §4.

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

---

## Round 2 — three measured numbers whose instrument was narrower than the claim, and two silent fail-opens

The design survived review unchanged and was re-derived independently. Everything below is a correction
to what was *reported*, plus two holes in what shipped. **The pattern in all five is the same one this
PR is about: a question narrower than the confidence placed in its answer.**

**1. The headline is 15, not 16, and "zero rename noise" was falsified by our own data.** #921 **is**
CPE-1753 — the PR that sharded the Linux GUI-smoke leg. At `ad809938^` `main` carried job id
`gui-smoke-linux` named `GUI smoke (ubuntu-latest) — tauri-driver + WebdriverIO (CPE-1171)`; at #921's
head the **same job id** is named `GUI smoke (ubuntu-latest) shard ${{ matrix.shard }} — …`, and its
board carried all four shards, the build job and the verdict, green. **The job was renamed by the PR
being judged — nothing went unjudged.** Round 1 tested "does this job still exist on `main`" by **id**
while the matcher keys on the **label**: wrong field, wrong conclusion. Re-classified by asking whether
the absent job's id existed in that PR's own head tree (`gh api .../contents/…?ref=<head>`): **15
GENUINE, 1 RENAMED-BY-THIS-PR**. True rename rate **1 of 16 firings (6.3%) / 1 of 184 merges (0.54%)**.
The shipped code behaves correctly on #921 (it blocks and names itself); only the measurement and the
noise-rate argument were wrong. Corrected in the doc's §1 table (new `classification` column), §3, the
`ci-poll.mjs` header and `coverageOf`'s header.

**2. The timing figures came from a truncated sample — inside a doc that invokes CPE-1932.** Re-taken
with `gh api --paginate` (`total_count` **793**, 793 rows returned, 790 completed / 417 success):
**median 60.9 min, p90 81.8 min**, not 65.9/108.2. The old sample's stated size — "196 completed runs"
— is not any plausible count for a fortnight and was the tell. Reproduced the old family by taking the
**newest 200 runs only**: median 63.7, p90 105.3, oldest created 2026-08-27T04:17Z, i.e. the last ~27
hours of a 14-day window — the crew's busiest, slowest stretch. §2b now states its instrument first,
and §2b's peak claim (*"at p90 it is the entire current throughput, with no slack"*) is **withdrawn**:
the real ceiling is 1440/81.8 ≈ **17.6 merges/day** against **12.86** actual, ~37% of headroom. The
recommendation (protection + merge queue) survives; the sentence selling it did not.

**3. §2d's trap did not exist, and it was the section most likely to change what the user does.**
`ci.yml`'s `paths-ignore` is on the **`push:`** trigger only — `{"push":{…,"paths-ignore":[…]},
"pull_request":{"branches":["main"]}}` — and `ci.yml:62-64` says so in its own words. Neither
PR-triggered workflow has any `pull_request:` path filter, `ci-verdict` has no `paths-ignore` to
remove, and ticket-only PRs would **not** become unmergeable. §2d is rewritten to say what is actually
true, and the claim is now **asserted rather than written down**: a new test enumerates
`.github/workflows/` at run time, pins the PR-triggered set to `ci.yml` + `gui-smoke.yml`, and reds
naming the file if either ever grows a `pull_request:` path filter. The same false premise was removed
from `ci-poll.mjs` ×2, the test file, both `nightly.yml` fixtures and §3.

**4. `workflowTriggersPullRequest` failed OPEN, silently, on legal YAML.** A `#` at **column 0**
anywhere inside `on:` hit `if (/^\S/.test(line)) return false;` and removed the **entire workflow** from
the required set — no `silentWorkflows` line, no signal. Measured on the real `ci.yml`: `true`, then
`false` with `# a comment` inserted at column 0 under `on:`, after which a one-check board scored `ok`.
`ci.yml`'s `on:` block already carries a ~60-line comment; one re-wrap and every `ci.yml` guard leaves
the required set permanently. It also failed open on `"pull_request":`, `"on":` and 4-space indent.
Rewritten as `readOnBlock`, a **tri-state**: comments stripped in both positions (CLAUDE.md rule 2 —
there was no filter at all, not merely an insufficient one), quoted keys and any indentation
understood, and an unclassifiable `on:` returns `null` → `coverage=unknown` → **exit 5**, never a quiet
`false`. Same shape one layer down: `scanWorkflowJobs` truncated the job list at a column-0 comment
inside `jobs:` (`["a"]` for the two-job case) and inside a block `needs:` list — pre-existing from
CPE-1906, but the coverage check is the first consumer for which a short list fails *open*. Closed.

**5. The whole-workflow fail-open is closed, and the narrowing is measured rather than assumed.** With
(3) established, the blanket "a PR-triggered workflow that contributed nothing is never flagged"
carve-out bought nothing and cost a whole-guard-set blind spot: a board with zero `ci.yml` checks
returned `coverage=ok`, exit 0, against the real `origin/main`. Now excused **only** when the
workflow's own `pull_request:` trigger is path-filtered. Cost measured before changing it, over all 186
merges by grouping each rollup on `checkSuite.workflowRun.workflow.name`: **0** boards missing `CI`,
**0** missing `GUI smoke` — zero added firings. Token is now `coverage=ok(N-silent)`, never a bare `ok`
when something stayed silent.

**Non-blocking, all done.** `npm audit` dropped from §2a's suggested required list, with that job's own
design note (`ci.yml:198-201`) quoted as the reason. `Guard set read from <ref>@<sha>` now appended to
**every** verdict branch, not the two the doc claimed — pinned across the stub matrix. Numbers
corrected: 12.86/day (13.29 over a round 14 days), **589** commits not 593, p90 open→merged 469.7
(linear-interpolated; 485.2 is nearest-rank, and the method is now stated); 2.90 mean / max 10 and
median 157.5 reproduce exactly. §1 now states the window is **inclusive at both ends** — #1090 merged
at exactly 11:05:17Z, so a strictly-exclusive bound gives 185.

**Red-proofs, run by hand, results written at each site.** Dropping the `on:`-block comment skip reds
exactly *"a column-0 comment inside `on:` no longer deletes the workflow from the required set"* (1
failed / 5 passed / 64 skipped). `if (false && …)` on the `jobs:` comment skip reds exactly *"a column-0
comment inside `jobs:` no longer truncates the job list"*. Restoring the blanket carve-out reds exactly
*"a PR-triggered workflow with NO path filter that contributed nothing is UNJUDGED, not excused"*.
Removing `${against}` from the `failure` branch reds the guard-set-ref test, naming `failure-and-skips`
and printing the offending line. Four changes, four distinct tests, no overlap.

**CPE-1929's two sabotages RE-RUN at 70 tests**, because round 2 changed both this rung's inputs and the
suite around it, and carrying a sabotage number forward unchanged is the same stale-evidence defect this
PR is about: disabling the rung → **3 failed / 67 passed**; forcing `coverageOf` to always answer `ok` →
**11 failed / 59 passed** (was 7/56). Still red, still on different tests. Numbers rewritten at the site.

**Gate table, re-run after the final round-2 edit.**

| check | result |
|---|---|
| `npm run check` (svelte-check + tsc) | 0 errors, 0 warnings |
| `npx vitest run` (whole root suite) | **358 files — 5290 passed, 2 skipped** |
| the 2 skipped | both in `catalogPublishLoudFailure.test.ts`, gated on `jq`, not installed on this machine — unchanged from round 1 |
| `src/lib/ciPollFailClosed.test.ts` alone | **70 passed, 0 failed, 0 skipped** (63 after round 1) |
| Rust | untouched — no crate changed |
