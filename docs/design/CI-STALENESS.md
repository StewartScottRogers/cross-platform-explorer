# Merging on stale checks — the exposure, the settings that would close it, and what shipped instead

**CPE-1970.** A guard is only ever as live as the newest CI run of the PR it judges. `main` has **no
branch protection at all**, so a PR can be merged on checks that were built before a guard landed on
`main`, and that guard never judges it. Nothing marks the merged commit as unjudged, and the guard's
next run measures a `main` that already contains the unjudged change — so it reports green forever.

---

## 1. The exposure, measured

**Window** 2026-08-14T00:00:00Z → 2026-08-28T11:05:17Z (14 days).
**Denominator** every PR merged into `main` in that window: **186**.

**The instrument.** For each merged PR, its own live check rollup (`gh pr view N --json
statusCheckRollup`) was compared against the set of jobs `main` carried at the moment that PR landed
(`git show <squash-commit>^:.github/workflows/*.yml`, parsed by the shipped `scanWorkflowJobs`). A PR
counts as exposed when a job that `main` already required produced **no check at all** on that PR's
board — that job could not have judged it. Both sides are derived at run time, never recalled
(CLAUDE.md, CPE-1932).

| outcome | PRs |
|---|---|
| clean — every job `main` required produced a check | **168** |
| **exposed — at least one required job entirely absent from the board** | **16** |
| unreadable, counted as fail-closed rather than clean | **2** |

The two unreadable are **#896** and **#899**; their squash commits (`c0bc715c`, `9339670c`) are no
longer reachable from `main`, so no "what did `main` require then" question can be asked about them.
They are reported, not silently dropped.

**Which guard never judged which merge:**

| absent job | merges it could not judge |
|---|---|
| `ratchet-guard` | 5 — #1053, #1055, #1057, #1054, **#1056** |
| `ci-verdict` (CPE-1956) | 5 — #1073, #1075, #1076, #1077, #1078 |
| `lockfile-preflight` | 2 — #1048, #1049 |
| `msrv` | 2 — #1030, #1032 |
| `ffmpeg-pin-guard` | 1 — #955 |
| `gui-smoke-linux` | 1 — #921 |

**#1056 is in the list**, which is the independent reproduction of the ticket's anecdote: it merged at
18:36:20Z with `ratchet-guard` absent from all 22 of its checks, 53 minutes after that job landed on
`main`. It raised `bidi-render-registry` 1552 → 1553 and the guard never saw the raise.

**This is not a historical curiosity — 14 of the 16 are from the last 36 hours of the window**, and
they cluster the way the mechanism predicts: a guard lands, and every PR already in flight merges
without it. Five `ratchet-guard` merges inside 41 minutes; five `ci-verdict` merges inside 50.

**A first, cheaper instrument over-reported and is recorded here so nobody re-derives it.** Comparing
the PR head's *workflow files* against `main`'s (pure git, no API) flagged 16 as well — but a
different 16. It missed #921 and wrongly flagged **#1039**, whose head tree lacked
`lockfile-preflight` yet whose board carried the check anyway: GitHub builds `pull_request` checks
from the **merge ref**, so `main`'s newer workflow can apply to a branch that does not contain it.
The rollup is the decisive source; the git tree is only a hypothesis about it.

---

## 2. The remedy that makes it impossible — a repository-settings change, so it needs you

Everything in this section is **Settings → Rules → Rulesets** (or the older Branches → Branch
protection) on `StewartScottRogers/cross-platform-explorer`. None of it can be done from a PR.

### 2a. The minimum that closes the hole

Create a ruleset targeting `main` with:

1. **Require status checks to pass before merging** — and list the checks. Suggested set, which is
   every PR-triggered job on `main` today that owns a verdict rather than feeding one:

   ```
   Frontend — type-check and test
   Lockfile pre-flight — cargo metadata --locked (no compilation)
   CI verdict — every job behind the lockfile pre-flight actually ran (CPE-1956)
   Ratchet guard — no baseline raised without a declaration
   npm audit — every npm project in the tree
   ffmpeg pin is a month-end anchor
   GUI smoke (ubuntu-latest) — verdict across all shards (CPE-1753)
   Layout guard (real-browser rects, no WebDriver) — CPE-1882
   ```

   `backend`, `crates`, `net-e2e`, `sidecar` and `msrv` are deliberately **not** listed: they are
   matrix jobs whose check names carry the OS, and `ci-verdict` already joins all five and reds if any
   of them did not run. Listing the join instead of the legs is what keeps the list stable when the
   matrix changes.

2. **Require branches to be up to date before merging.** ← **This is the one setting that closes
   CPE-1970.** Without it, item 1 only guarantees the checks were green, not that they were green
   against a `main` containing today's guards. All 16 exposed merges had green checks.

3. **Do not allow bypassing the above settings.** Leave this OFF and `gh pr merge --admin` — which is
   what this crew's Foreman uses — walks straight past both. Turning it ON means the Foreman can no
   longer force a merge; that is the point, and it is also the setting most likely to be switched back
   off at 3am, so decide it deliberately rather than by default.

### 2b. What it costs, measured on this repo's own numbers

| measurement | value | source |
|---|---|---|
| merges to `main` | **13.0 / day** | 186 merges over the window |
| other PRs open at the average merge instant | **2.90** (max 10) | interval overlap over the same 186 |
| successful `ci.yml` run, wall clock | **median 65.9 min, p90 108.2 min** | 196 completed `ci.yml` runs in the window |
| PR open → merged | median 157.7 min, p90 485.2 min | same 186 |

With *require up to date* and no merge queue, **every merge invalidates every other open PR**. So:

- **Merging becomes serial at one CI run apiece.** A queue of five ready PRs takes 5 × 65.9 min ≈
  **5.5 hours** to drain, against today's near-simultaneous merges.
- **Ceiling ≈ 21 merges/day** at the median run time, **13 at p90**. The crew currently does **13.0 a
  day**. That is not "a real tax" — at p90 it is *the entire current throughput*, with no slack.
- **≈ 38 extra full `ci.yml` runs per day** (2.90 invalidated PRs × 13.0 merges), each a 3-OS matrix.

### 2c. The version that keeps the guarantee and most of the throughput

Add **merge queue** (Settings → Rules → `Require merge queue`, or the ruleset's merge-queue rule):

- GitHub builds each entry against the projected post-merge `main`, so "up to date" is guaranteed
  *without* a human rebase.
- With a batch size above 1 it validates several PRs in **one** run — turning the 5-PR / 5.5-hour drain
  above back into roughly one run.
- Cost: `ci.yml` and `gui-smoke.yml` need a `merge_group:` trigger added (a code change, and one this
  crew can do), and a red PR inside a batch causes a bisect and a re-run of the survivors.

**Recommendation: 2a items 1 and 2, plus the merge queue in 2c, and item 3 last** once the queue is
proven — item 3 removes the escape hatch, and removing it before the queue works is how a whole shift
gets blocked.

### 2d. One trap in this repo specifically, before you click anything

`ci.yml` carries a **`paths-ignore`** covering `Ticketing/Tickets/**`, `Ticketing/Sprints/**` and
`.claude/**`. A required status check that **never reports** is treated by GitHub as *pending forever*,
not as *not applicable* — so the moment item 1 is enabled, every ticket-bookkeeping-only PR becomes
permanently unmergeable. Fix it in the same change, either by moving the path filtering from the
workflow trigger down to a first `changed-files` job that always reports, or by leaving `ci.yml`'s
checks out of the required list and requiring only `ci-verdict`, which would then need its own
`paths-ignore` removed. **This is the reason to do the settings change deliberately rather than in a
hurry.**

---

## 3. What shipped instead, because the settings change may never come

`scripts/ci-poll.mjs` — the poll this crew's merge procedure actually consults — now emits a
**staleness verdict**, exit code **5**, prefix `completed stale-checks`, on a board where nothing is
red *and* a job `main` requires produced no check at all. Its sibling `completed coverage-unknown`
(also 5) fires when the check could not be computed, because "did not run" is not "found nothing".
Every verdict line, green ones included, now carries a `coverage=` field.

### The precision choice, argued

Three rules were candidates. The shipped one is (c).

| rule | verdict |
|---|---|
| (a) "`main` moved at all since the checks ran" | **Rejected.** 593 commits landed on `main` in the 14-day window, ~42/day; a PR that sits in the queue an hour almost always sees `main` move. It would fire on nearly all 186 and train the crew to wave it through — worse than the bug. |
| (b) "the newest check run finished before a guard landed" | **Rejected, and the ticket's own evidence is what rejects it.** #1056's run finished at 18:35:13Z and it merged at 18:36:20Z — 67 seconds later, on a board that had only just gone green. A recency check on finish time passes it. It is also only an inference. |
| (c) "a job `main` requires produced no check on this board" | **Shipped.** Definite rather than inferential — the guard is not on the board, so it cannot have judged anything. No clock reasoning. Swept over the 184 evaluable merges: **168 clean, 16 firings**, and all 16 name a job that still exists on `main` today, i.e. zero deletion/rename noise. A gate that reds 9% of merges gets read; one that reds 95% gets aliased away. |

### What it misses — stated because an undocumented blind spot reads as coverage

- **A guard added *inside* an existing job.** A new `src/lib/*.test.ts` runs under the same
  `Frontend — type-check and test` check; a new ratchet registered in `scripts/ratchet-baselines.mjs`
  runs under the same `Ratchet guard` check; CPE-1936's `shellScriptLines` parser fix changed what
  every workflow scan can see without touching a job name. The check **is** on the board, so exit 5 is
  silent. **This is the larger class by count and no name-based instrument can see it.** Only
  re-running the PR's checks against `main`'s head can — which is exactly what §2's *require branches
  to be up to date* buys, and why §2 is still the ask.
- **A locally stale `origin/main`.** The poll reads the required-job set from `origin/main` via `git
  show` and deliberately **does not fetch** (a merge gate must not have side effects, and a `git fetch`
  is one more thing that can hang inside a wall-clock-bounded tool). A clone that has not fetched since
  the guard landed reports `coverage=ok`. Mitigations: the ref and its short SHA are printed on every
  verdict line, and `.claude/commands/sprint.md` now tells the Foreman to `git fetch origin main` at the
  top of each sweep.
- **A workflow that contributed nothing at all** is not flagged, deliberately — see the `paths-ignore`
  trap in §2d; without that carve-out every bookkeeping PR would red. The silent workflows are **named**
  on stdout instead of dropped.
- **A job `main` deleted or renamed** looks the same as a job that did not run. Fail-closed direction:
  it blocks, names itself, and is one line to read. Zero occurrences in the 184 swept.

### The red-proof, both directions

`src/lib/ciPollFailClosed.test.ts` drives the real script as a subprocess and asserts on **exit code
and stdout**, never on an internal. The `guard-gap` stub emits **one identical rollup** for both legs;
only `main` changes, via two fixture directories that differ by exactly one job (asserted, not
claimed):

- `src/lib/fixtures/workflows-base-ahead/` — `main` has `ratchet-guard` → **exit 5**,
  `CI VERDICT: completed stale-checks`, the guard named, `coverage=1-unjudged`.
- `src/lib/fixtures/workflows-base/` — `main` does not → **exit 0**, `completed success`,
  `coverage=ok`.

Plus: an unreadable base and an empty base both → exit 5 `completed coverage-unknown`; a red board
stays exit 1 even with an unreadable base; `--run` mode prints `coverage=n/a(run-mode)`; a pending
board prints `coverage=n/a(board-pending)`. And a derivation leg (CPE-1950) reads **this repo's real
`.github/workflows`** through the shipped functions and reconstructs #1056 against the real
`ratchet-guard` label read out of `ci.yml`, so the fixtures cannot rot into agreeing with each other
about a repo that has moved on.

---

## 4. The other ratchets, checked

The ticket asks whether the same shape bypasses `gui-smoke/known-failing.json`, the hex ratchet, and
the eight allowlists. **Yes — identically, and for the same single reason:** all of them are measured
by `scripts/ratchet-baselines.mjs` under the `ratchet-guard` job, so any PR whose board lacks that job
bypasses **every** one of them at once. That is not eight holes, it is one, and it is the one exit 5
now refuses: five of the sixteen exposed merges are exactly that job. What exit 5 still cannot see is a
PR whose board **has** `ratchet-guard` but whose checks predate a change to what the measurer counts —
the "guard added inside an existing job" blind spot above.

**`bidi-render-registry`'s undeclared 1552 → 1553 raise is deliberately not backdated here.** CPE-1948
already settled that in `docs/design/RATCHETS.md`: a licence row is consumable only from inside the diff
that performs the raise, so a retroactive row would be the one row on that page that is false by the
page's own definition. The movement is recorded in that document's recount, which names this ticket as
the fix.
