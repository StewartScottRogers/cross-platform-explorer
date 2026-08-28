---
id: CPE-1967
title: **no job in `ci.yml` carries a `timeout-minutes` at all**, and three script wrappers spawn external tools untimed — everything sits under the 6-hour Actions default
type: task
priority: Medium
status: In Progress
tags: ready
estimate: S
created: 2026-08-27
---

## Summary

Found by PR #1078's Reviewer while auditing `ci-poll.mjs`, and **widened by it**: the PR's author
flagged two jobs as lacking a timeout; the Reviewer checked all ten and found **none of them has one.**

- **`ci.yml`: 0 of 10 jobs carry a job-level `timeout-minutes`.** Only individual *steps* have caps, so
  a hung job runs to the **6-hour GitHub Actions default**.
- **`gui-smoke.yml` does cap its jobs** (15 / 30 / 15 min) — so the practice exists in this repo and
  the main CI workflow is the one that skipped it.
- **`scripts/audit-npm-projects.mjs`** spawns `npm audit` with **no timeout**.
- **`scripts/sidebar-drop-stack-overlap/check.mjs:70-76`** — its CDP `send()` has **no per-call
  timeout**, unlike `layout-guard/engine.mjs`, which has `CDP_CALL_TIMEOUT_MS`.

## Why it is worth a ticket rather than a note

This sprint spent **over an hour** unable to tell a *slow* `Server crates (windows-latest)` job from a
*hung* one, with two approved PRs blocked behind it, and settled it only by comparing start timestamps
against the same job on a sibling PR by hand. A job timeout would have answered that question in the
runner rather than in the Foreman's head.

It is also the **fail-open family** in its purest form, one layer out from where the sprint has been
fighting it: a process that never finishes never reports, and "never reported" is the one state no
verdict can classify. CPE-1906 fixed the poller's side (a hung `gh` no longer blows through the
advertised budget); this is the runner's side of the same problem.

**Deliberately not fixed in PR #1078** — that PR is about `ci-poll.mjs`, and adding workflow timeouts
there would have been unreviewable scope creep.

## Acceptance criteria

- [ ] **Enumerate, don't recall** (CPE-1932): derive the job list at run time (`git ls-files
      '.github/workflows/*.yml'`) and report every job with its current `timeout-minutes`, rather than
      fixing the ten someone remembered. There are 8 workflow files.
- [ ] **Pick each timeout from measured duration, not a round number.** Job durations are available
      from `gh api`; the whole-run median here is **58.9 min**, and per-job spreads are wide —
      `Server crates (windows-latest)` legitimately runs ~60 min while `Frontend` runs ~19 min, so one
      shared constant is wrong. **Record the measurement beside each value**, or the next person will
      not know whether a timeout that fires means "hung" or "we guessed low".
- [ ] Add per-call timeouts to `audit-npm-projects.mjs`'s spawn and `check.mjs`'s CDP `send()`. Match
      `layout-guard/engine.mjs`'s existing `CDP_CALL_TIMEOUT_MS` shape rather than inventing a second.
- [ ] **A timeout must fail loudly and distinguishably.** A job killed at its cap must not read as any
      other kind of failure, and must not read as a skip — `ci-poll.mjs` now has an exit-4 "did not
      run" verdict, and a timed-out job is a *third* thing again. Check what the rollup reports for a
      cancelled-by-timeout job and make sure the poller classifies it correctly.
- [ ] **Consider a guard** asserting every job in every workflow declares a `timeout-minutes`. This
      repo's pattern is that an enumerated invariant gets a test (`ciVerdict.test.ts`,
      `catalogPublishLoudFailure.test.ts`), and "someone adds a job without a cap" is exactly the drift
      that recreates this. If you add one, it must **derive** the job list from the parsed YAML, not a
      hard-coded list — and anchor on the parsed document, not on comment text (CPE-1933).
- [ ] **Widen the sweep past `*.mjs`.** PR #1078's audit covered `scripts/*.mjs` only;
      `scripts/release.ps1`, `new-sample-sandbox.{sh,ps1}` and `gen_samples.py` also wrap external
      tools and were never checked. Report a verdict per script.

## Notes

Filed 2026-08-27 by the sprint Foreman from PR #1078's Reviewer, which measured all ten `ci.yml` jobs
rather than accepting the two the PR named.

Related: **CPE-1906** (the poller's side of the same problem — a hung `gh` crossing the advertised
budget, PR #1078), **CPE-1956** (`ci.yml`'s silent-skip gate, PR #1074), **CPE-1932** (enumerate,
don't recall), **CPE-1171** (the gui-smoke harness, which already caps its jobs).

## Work Log

### 2026-08-28 — the enumeration

Derived at run time from `git ls-files '.github/workflows/*.yml'` (8 files), parsed with the repo's own
`parseYaml` via `discoverWorkflows`/`parseWorkflowFile` in `src/lib/workflowShellSources.ts` — never a
list of the jobs someone remembered. **28 jobs. 18 of them had no `timeout-minutes` at all**, which is
**8** more than the ten the ticket named: `ci.yml`'s 10, plus `model-snapshot.yml`'s 1,
`release.yml`'s 3 and `release-sidecar.yml`'s 4. Job-level caps go from 10 to 28.

**CORRECTION — this line first said "17 … 7 more", and the table immediately below it already had
eighteen `**none**` rows.** It is worth stating plainly rather than silently swapping the digit,
because it is this ticket's own defect, one level out: the "17" came from carrying the ticket's
premise (`ci.yml` has ten jobs, none capped) forward as a baseline instead of re-deriving it from the
merge base. `ci.yml` has **eleven** jobs, and `ci-verdict` **already carried `timeout-minutes: 10`**
— verifiable in one command,
`git show 337ac334:.github/workflows/ci.yml | grep -nE '^    timeout-minutes:'`, which returns
exactly one line (`2042`). Recall lost to enumeration inside the PR arguing that enumeration beats
recall. The counts are no longer written down anywhere unguarded: `src/lib/workflowJobTimeouts.test.ts`
now derives `ci.yml`'s job count, its job-level cap count, its step-level cap count and its comment
mentions at run time and reds if the prose and the file disagree (red-proofed: adding a twelfth job
reds two of the three legs).

| workflow | job | before | after |
|---|---|---|---|
| `catalog-freshness.yml` | `check-catalog-freshness` | 10 | 10 (unchanged) |
| `ci.yml` | `frontend` | **none** | 10 |
| `ci.yml` | `npm-audit-sweep` | **none** | 10 |
| `ci.yml` | `ratchet-guard` | **none** | 10 |
| `ci.yml` | `lockfile-preflight` | **none** | 10 |
| `ci.yml` | `backend` (3-OS matrix) | **none** | 40 |
| `ci.yml` | `crates` (3-OS matrix) | **none** | 105 |
| `ci.yml` | `net-e2e` | **none** | 25 |
| `ci.yml` | `sidecar` (3-OS matrix) | **none** | 20 |
| `ci.yml` | `ffmpeg-pin-guard` | **none** | 10 |
| `ci.yml` | `msrv` | **none** | 30 |
| `ci.yml` | `ci-verdict` | 10 | 10 (unchanged; measurement added) |
| `ffmpeg-pin-freshness.yml` | `check-pins` | 30 | 30 (unchanged) |
| `gui-smoke.yml` | `gui-smoke` | 15 | 15 (unchanged) |
| `gui-smoke.yml` | `gui-smoke-linux-build` | 30 | 30 (unchanged) |
| `gui-smoke.yml` | `gui-smoke-linux` | 30 | 30 (unchanged) |
| `gui-smoke.yml` | `gui-smoke-linux-verdict` | 15 | 15 (unchanged) |
| `gui-smoke.yml` | `layout-guard` | 15 | 15 (unchanged) |
| `gui-smoke.yml` | `launcher-contrast` | 15 | 15 (unchanged) |
| `model-snapshot.yml` | `snapshot` | **none** | 10 |
| `release-pipeline-watchdog.yml` | `notify-on-failure` | 10 | 10 (unchanged) |
| `release-sidecar.yml` | `create-release` | **none** | 10 |
| `release-sidecar.yml` | `verify-updater-pin` | **none** | 30 |
| `release-sidecar.yml` | `release-sidecar` (3-platform matrix) | **none** | 40 |
| `release-sidecar.yml` | `verify-published-manifest-sidecar` | **none** | 30 |
| `release.yml` | `release` (3-platform matrix) | **none** | 25 |
| `release.yml` | `verify-published-manifest` | **none** | 30 |
| `release.yml` | `catalog` | **none** | 10 |

### 2026-08-28 — how each number was chosen

Read-only `gh api` over the 100 most recent COMPLETED runs of each workflow, successful jobs only,
measured 2026-08-28. One shared constant would be wrong: the per-job spread is **0.1 to 68.2 minutes**.
The rule, stated once in the block above `jobs:` in `ci.yml` and applied uniformly, is

    cap = max(10, ceil_to_5(1.5 x measured max))

with `measured max` the slowest successful run (not the median — the p50-to-max spread is dominated by
cold Rust/npm caches, a normal condition here) and the slowest matrix LEG where one cap covers several.
The 10-minute floor exists because every job pays a network-bound fixed cost that is spiky on a shared
runner. The sample (`n`, min/p50/p90/max) is recorded on the comment line above every value it
justifies, per job and per matrix leg.

Correcting the ticket from measurement: it quoted `Frontend` at ~19 min; it is **n=83, max 5.9 min**.
`Server crates (windows-latest)` is confirmed at **n=50, p50 58.2 / max 68.2 min** — the largest cap in
the tree at 105, and still under a third of the 360-minute default.

**Three jobs are UNMEASURED and say so at the site** rather than borrowing the credibility of the
sample sizes beside them: `release.yml`'s `verify-published-manifest` (added 2026-08-23, f97aef8a),
`release-sidecar.yml`'s `verify-updater-pin` (2026-08-26, f1f0a4d5) and `verify-published-manifest-sidecar`
(2026-08-27, 0d9a992d). The most recent release run of either workflow is 2026-08-23T14:36Z, so all
three have literally never executed — they appear zero times, in any conclusion, across the 100 most
recent completed runs. Each carries 30 minutes derived from an explicit ANALOGUE (`ci.yml`'s `msrv`,
same job shape, measured max 17.1 min over n=75) and a note to replace it with a real measurement after
the next release cuts.

### 2026-08-28 — the guard

`src/lib/workflowJobTimeouts.test.ts`: every job in every workflow declares a cap, every cap is a
positive whole number, no cap is at or above 360 (a cap that does not bound below the default is the
status quo with a number next to it). The job list is derived from the PARSED YAML — never text. That
matters concretely: `timeout-minutes` appears in today's `ci.yml` far more often than there are job
caps, split three ways — job-level keys, an equal number of STEP-level keys (which a text scan must
not count as a job cap), and comment prose, including a fully-indented `#   timeout-minutes: 6` inside
a worked example that a naive line filter reads as a key.

**Those counts are no longer written down in prose anywhere.** A first draft of this Work Log and of
the test's own docblock quoted them as digits and got two of them wrong (see the CORRECTION above,
and "22 real keys, 10 of them step-level" — it is eleven). So the test now DERIVES every one of them:
`describe("the counts this file's rationale quotes are DERIVED from ci.yml, not recalled")` measures
`ci.yml`'s parsed job count, its job-level cap count, its step-level cap count and its comment
mentions at run time, cross-checks the text scan against the parser, and reds if the prose and the
file disagree. That is CPE-1948's rule — do not keep an unguarded second copy of a measurement —
applied to the file that was breaking it.

The one claim that cannot be derived at run time is the pre-CPE-1967 state, and it is made
REPRODUCIBLE rather than asserted: `git show 337ac334:.github/workflows/ci.yml | grep -nE '^
timeout-minutes:'` returns exactly one line. Deriving it inside the test was considered and rejected
on a measured reason — this suite runs in `ci.yml`'s `frontend` job, whose `actions/checkout@v4` has
no `fetch-depth: 0` (only `ratchet-guard` sets it), so a shallow clone has no object for that
revision and the leg would either red on every run or be written to tolerate the miss and pass
vacuously.

No allowlist and no stored count of offenders: the invariant is total today, so a ratchet would be a
standing licence to add an uncapped job. `MIN_EXPECTED_JOBS = 20` is an enumeration sanity floor, not a
ratchet — it can only cause a failure, never excuse one. The full suite (360 files / 5,385 tests) stays
green, `ratchetBaselines.test.ts` and `ratchetsDoc.test.ts` included, so nothing ratchet-shaped landed.

**Red-proofed, six sabotages, results recorded in the test file itself:** deleting `crates`' cap →
the presence test reds naming `.github/workflows/ci.yml [crates]`; `timeout-minutes: 400` → the
above-the-default test reds naming the job, 400 and 360; `timeout-minutes: "30"` → the type test reds
naming the string; inserting a twelfth job → two of the three derived-count legs red, one naming the
full job list and `expected 12 to be 11`; **removing `frontend`'s cap → 3 failed / 3 passed with the
PARSED half still passing at 11 while the TEXT half fails `expected 10 to be 11`**, which is the one
that proves the two measurements are independent rather than a number compared against itself; adding
one ordinary step-level cap → 1 failed / 5 passed, the equality leg printing the message that tells
the reader to reword the prose rather than delete the step's cap. `ci.yml` restored after every one,
`git diff --numstat` clean each time.

**The gap this guard does NOT close, now declared on its own list rather than left to be discovered:**
it never checks a cap's VALUE against the sample quoted above it. Re-run here rather than taken from
the review that asked for it — editing `crates` from 105 to **47**, which contradicts its own
`max 68.2 min` line three rows up and would kill the job on an ordinary slow run, leaves the file
**6/6 green**. Inherent: only GitHub's run history could judge a cap, and a unit test cannot query it
without becoming flaky by construction. What guards the values instead is the rule and its arithmetic
being written out per job in `ci.yml`, where a reviewer checks `1.5 x max` against the sample in the
same three lines. A declared gap beats an undeclared one.

### 2026-08-28 — the untimed spawns

* **`scripts/audit-npm-projects.mjs`** — all THREE spawns capped, not the one the ticket named: both
  `npm` calls and the `git ls-files` enumeration. `NPM_CALL_TIMEOUT_MS = 120_000`, matching
  `layout-guard/engine.mjs`'s `CDP_CALL_TIMEOUT_MS` shape (one named constant, a per-call
  `{ timeoutMs }` override, a failure naming the call) rather than a second idiom. Measured locally:
  `npm audit` 1.9-3.3 s, `npm audit fix --package-lock-only` 5.1-7.5 s; 120 s is ~16x the slowest.
  A killed call is reported in its own words — "did not run", never "ran and found nothing".
  Red-proofed both branches, and CPE-1929's second sabotage on the `audit fix` branch: with it
  disabled the sweep printed `UNAPPLIED non-major fix, measured (0)` and exited **0, green**, so the
  branch is reached and the understatement it prevents is real.
* **`scripts/dev-harness/sidebar-drop-stack-overlap/check.mjs`** — `send()` now carries
  `CDP_CALL_TIMEOUT_MS = 15000` and a per-call override, the same shape and the same number as
  `layout-guard/engine.mjs`; `Page.navigate` gets 40 s at the call site for the cold-dev-server ack,
  the same exception that engine makes for the same call.
* **`scripts/ci-poll.mjs`** — a job killed at its cap was already RED (both readers funnel anything
  that is not SUCCESS/NEUTRAL/SKIPPED into `failedNames`, exit 1) but was indistinguishable from a
  test that ran and reported. `haltedFrom` now names it: a `STOPPED rather than judged (timed out or
  cancelled)` sentence on the failure verdict plus a `halted=` field appended after `coverage=` on
  every totals line, zero included. Exit codes and the verdict ladder are unchanged.
  **Which token GitHub uses for a cap kill was NOT assumed.** Measured read-only: job-conclusion
  histograms over the 100 most recent completed runs are `ci.yml` success=1249 / cancelled=131 /
  failure=47 and `gui-smoke.yml` success=559 / skipped=92 / cancelled=30 / failure=47 — `timed_out`
  appears **zero** times in either, because before this ticket only `gui-smoke.yml` had caps and none
  was ever reached. So both `timed_out` and `cancelled` are collected as one state and the wording
  says "timed out or cancelled". A real cancelled job's shape (run 33138742329): job `cancelled`,
  in-flight step `cancelled`, later steps `skipped`. CPE-1929 sabotages on `haltedFrom`: disabled →
  3 failed / 3 passed; forced to lie → 3 failed / 3 passed. Both red, so it is reached, not shadowed.

### 2026-08-28 — the sweep past `*.mjs`, verdict per script

Enumerated with `git ls-files` over `*.ps1 *.sh *.bash *.py *.cmd *.bat` — 10 files, not the three the
ticket named.

| script | wraps | verdict |
|---|---|---|
| `scripts/gen_samples.py` | `subprocess.run` x2 (ffmpeg) | **GAP, FIXED.** Both were `check=True, capture_output=True` with no `timeout=`, so a wedged ffmpeg blocked forever with its output swallowed. `FFMPEG_TIMEOUT_S = 120` on both; `TimeoutExpired` is already caught by the existing `except Exception`, so it reaches the same `note()` + stub fallback as any other ffmpeg failure. No new failure path. |
| `scripts/release.ps1` | `git` x5 via `Invoke-Git` | **GAP, DELIBERATELY NOT CAPPED — reasoning DERIVED and recorded at the site.** `add`/`commit`/`tag` are local; `push` genuinely can hang. But the git section is attended by construction, and that load-bearing claim is derived rather than asserted (CPE-1933): `git grep -i 'release\.ps1'` returns every tracked reference, and each is a human instruction (RELEASING.md, `run.md`), prose (CLAUDE.md), a **comment** in `release.yml:139` / `release-sidecar.yml:592` / `catalog-version.sh:84` (no `run:` invokes it), history, or the test harness — and that harness, the one genuinely unattended caller, runs the real script only with `-BumpOnly`, which `exit 0`s at line ~492, **above** `Invoke-Git`'s definition. No CI path reaches a `git` call in this file. Meanwhile PowerShell has no `timeout` for a native command, so bounding one means `Start-Process` + kill, which would abort a push mid-transfer on a merely slow link and leave a tagged-but-unpushed tree. The comment names what expires the reasoning (a caller reaching past the `-BumpOnly` exit) and the right mechanism then (`http.lowSpeedLimit`). |
| `scripts/new-sample-sandbox.ps1` | nothing | **CLEAN.** `Get-Date`, `New-Item`, `Copy-Item`, `Remove-Item` — cmdlets, no external process. |
| `scripts/new-sample-sandbox.sh` | `date`, `mkdir`, `cp`, `rm` | **CLEAN.** Local coreutils on a local tree; nothing network-bound and nothing that can block indefinitely. |
| `.github/workflows/scripts/catalog-freshness-check.sh` | `date` (x3) | **CLEAN.** Local coreutils, epoch arithmetic only. Runs inside a job that now carries a cap. |
| `.github/workflows/scripts/catalog-version.sh` | `date`, `git -C … log -1` | **CLEAN.** Both local: a clock read and a commit-timestamp read against a repo already on disk. Nothing network-bound, and it runs inside a job that now carries a cap. |
| `.github/workflows/scripts/ffmpeg-anchor-check.sh` | `date` | **CLEAN.** Local coreutils; a month-end anchor calculation. Despite the name it does not invoke ffmpeg. |
| `RunClaude.cmd` | `claude` | **OUT OF SCOPE.** A launcher for an interactive session; the session IS the process, and bounding it would be bounding the user. |
| `hrdrClaudeNative.cmd` | interactive launcher | **OUT OF SCOPE.** Same. |
| `samples/text/hello.py` | nothing | **N/A.** A test fixture, not a tool wrapper; never executed by anything here. |
