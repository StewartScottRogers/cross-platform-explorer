# Merging on stale checks — the exposure, the settings that would close it, and what shipped instead

**CPE-1970.** A guard is only ever as live as the newest CI run of the PR it judges. `main` has **no
branch protection at all**, so a PR can be merged on checks that were built before a guard landed on
`main`, and that guard never judges it. Nothing marks the merged commit as unjudged, and the guard's
next run measures a `main` that already contains the unjudged change — so it reports green forever.

---

## 1. The exposure, measured

**Window** 2026-08-14T00:00:00Z → 2026-08-28T11:05:17Z, **inclusive at both ends** — 14 days
11 h 05 m, i.e. 14.462 days. The boundary is load-bearing: #1090 merged at *exactly* 11:05:17Z, so a
strictly-exclusive upper bound gives 185, not 186.
**Denominator** every PR merged into `main` in that window: **186**
(`gh pr list --state merged --search "merged:2026-08-14T00:00:00Z..2026-08-28T11:05:17Z"`).

**The instrument, and what it cannot see.** For each merged PR, its own live check rollup (`gh pr view
N --json statusCheckRollup`) was compared against the set of jobs `main` carried at the moment that PR
landed (`git show <squash-commit>^:.github/workflows/*.yml`, parsed by the shipped `scanWorkflowJobs`).
A PR counts as exposed when a job that `main` already required produced **no check at all** on that
PR's board — that job could not have judged it. Both sides are derived at run time, never recalled
(CLAUDE.md, CPE-1932). It matches jobs **by check-name label**, so it cannot distinguish "the guard did
not run" from "the guard ran under a different name" — which is exactly the distinction the exposed
list turned out to need, below.

| outcome | PRs |
|---|---|
| clean — every job `main` required produced a check | **168** |
| **at least one required job entirely absent from the board** | **16** |
| unreadable, counted as fail-closed rather than clean | **2** |

**Of those 16, 15 are exposure and 1 is rename noise.** Classified by asking whether the absent job's
**id** existed in that PR's own head tree (`gh api .../contents/.github/workflows/<file>?ref=<head>`):
15 named a job id the PR's own tree did not have — a genuinely new guard. One, **#921**, did: that PR
*is* CPE-1753, which sharded the Linux GUI-smoke leg. It kept the job id `gui-smoke-linux` and changed
its `name:` from `GUI smoke (ubuntu-latest) — tauri-driver + WebdriverIO (CPE-1171)` to the templated
`GUI smoke (ubuntu-latest) shard ${{ matrix.shard }} — …`. Its board carried all four shards, the build
job and the verdict job, all green. **Nothing went unjudged there — the job was renamed by the PR being
judged.** So the headline is **15**, and the measured rename rate is **1 of 16 firings (6.3%), 1 of the
184 evaluable merges (0.54%)**.

The two unreadable are **#896** and **#899**; their squash commits (`c0bc715c`, `9339670c`) are no
longer reachable from `main`, so no "what did `main` require then" question can be asked about them.
They are reported, not silently dropped.

**Which guard never judged which merge:**

| absent job | merges it could not judge | classification |
|---|---|---|
| `ratchet-guard` | 5 — #1053, #1055, #1057, #1054, **#1056** | genuine |
| `ci-verdict` (CPE-1956) | 5 — #1073, #1075, #1076, #1077, #1078 | genuine |
| `lockfile-preflight` | 2 — #1048, #1049 | genuine |
| `msrv` | 2 — #1030, #1032 | genuine |
| `ffmpeg-pin-guard` | 1 — #955 | genuine |
| `gui-smoke-linux` | 1 — #921 | **renamed by this PR — not exposure** |

**#1056 is in the list**, which is the independent reproduction of the ticket's anecdote: it merged at
18:36:20Z with `ratchet-guard` absent from all 22 of its checks, 53 minutes after that job landed on
`main`. It raised `bidi-render-registry` 1552 → 1553 and the guard never saw the raise.

**This is not a historical curiosity — 14 of the 15 are from the last 36 hours of the window**
(everything from #1030 at 2026-08-27T00:05:28Z on; only #955, 2026-08-20, is older), and
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
   ffmpeg pin is a month-end anchor
   GUI smoke (ubuntu-latest) — verdict across all shards (CPE-1753)
   Layout guard (real-browser rects, no WebDriver) — CPE-1882
   ```

   `backend`, `crates`, `net-e2e`, `sidecar` and `msrv` are deliberately **not** listed: they are
   matrix jobs whose check names carry the OS, and `ci-verdict` already joins all five and reds if any
   of them did not run. Listing the join instead of the legs is what keeps the list stable when the
   matrix changes.

   **`npm audit — every npm project in the tree` is deliberately not listed either, and an earlier
   draft of this page had it.** That job's own design note (`.github/workflows/ci.yml:198-201`) says it
   is kept off the `needs:` chain because *"`npm audit` is a live registry query, so its verdict can
   change without the tree changing. A newly-published advisory should tell the Dependency Steward to
   act; it should not retroactively block an unrelated PR's builds."* Making it a **required** check is
   precisely that retroactive block, one layer up. If you want it required, reverse that note in the
   same change rather than around it.

   `GUI smoke (windows-latest) — tauri-driver + WebdriverIO` is also not listed: it carries a job-level
   `if:` excluding `pull_request` (CPE-1594) and reports **skipped** on every PR by design — measured
   on #921's board and on #1068's. The ubuntu **verdict** job is the one that always reports.

2. **Require branches to be up to date before merging.** ← **This is the one setting that closes
   CPE-1970.** Without it, item 1 only guarantees the checks were green, not that they were green
   against a `main` containing today's guards. All 15 exposed merges had green checks.

3. **Do not allow bypassing the above settings.** Leave this OFF and `gh pr merge --admin` — which is
   what this crew's Foreman uses — walks straight past both. Turning it ON means the Foreman can no
   longer force a merge; that is the point, and it is also the setting most likely to be switched back
   off at 3am, so decide it deliberately rather than by default.

### 2b. What it costs, measured on this repo's own numbers

**The instrument, first, because the previous draft's instrument was narrower than its claim.** Run
durations come from the Actions API **paged to exhaustion** — `gh api --paginate
"…/actions/workflows/ci.yml/runs?created=<window>&per_page=100"`, whose `total_count` is **793** and
which yields 793 rows: 790 completed (417 success, 356 cancelled, 16 failure, 1 startup_failure).
Duration is `updated_at − run_started_at`; percentiles are **linear-interpolated** between order
statistics (nearest-rank is quoted where it differs). Success-only, because a cancelled run's clock
says nothing about how long the gate takes.

| measurement | value | source |
|---|---|---|
| merges to `main` | **12.86 / day** | 186 merges ÷ 14.462 days (13.29 over a round 14 days) |
| other PRs open at the average merge instant | **2.90** (max 10) | interval overlap over the same 186 |
| successful `ci.yml` run, wall clock | **median 60.9 min, p90 81.8 min** (nearest-rank p90: 82.2; the median is 60.9 either way) | **417 successful of 790 completed** `ci.yml` runs in the window |
| PR open → merged | median 157.5 min, p90 469.7 min | same 186 (nearest-rank p90: 485.2) |
| commits landing on `main` | 589 total, ~41 / day | `git rev-list --count origin/main --since=… --until=…` |

**What the previous draft reported, and why it was wrong.** It said *median 65.9 min, p90 108.2 min,
over "196 completed `ci.yml` runs in the window"* — but the window holds **790** completed runs, so
that sample was a `--limit`-capped list, not the window. Re-measuring the **newest 200 runs only**
reproduces that family (median 63.7, p90 105.3, oldest run created 2026-08-27T04:17Z): the cap covered
the last ~27 hours of a 14-day window, which is the crew's own busiest and slowest stretch. **This is
"enumerate, don't recall / fail loudly on a truncated enumeration" (CPE-1932) missed inside the section
that invokes it** — and the tell was visible without re-running anything: 196 ≠ any plausible count of
a fortnight's CI.

With *require up to date* and no merge queue, **every merge invalidates every other open PR**. So:

- **Merging becomes serial at one CI run apiece.** A queue of five ready PRs takes 5 × 60.9 min ≈
  **5.1 hours** to drain, against today's near-simultaneous merges.
- **Ceiling ≈ 23.6 merges/day** at the median run time, **17.6 at p90**, against **12.86** actual.
  So the honest reading is a **real but survivable tax**: even in the p90 case the ceiling is ~37%
  above current throughput. The earlier draft's *"at p90 it is the entire current throughput, with no
  slack"* was an artefact of the truncated sample and **is not supported** — it is withdrawn, not
  softened. What the numbers do support: the headroom is thin enough that a bad CI day eats it, which
  is the argument for 2c rather than against 2a.
- **≈ 37 extra full `ci.yml` runs per day** (2.90 invalidated PRs × 12.86 merges), each a 3-OS matrix.
  That figure is unchanged by the correction, and it is the cost that 2c actually removes.

### 2c. The version that keeps the guarantee and most of the throughput

Add **merge queue** (Settings → Rules → `Require merge queue`, or the ruleset's merge-queue rule):

- GitHub builds each entry against the projected post-merge `main`, so "up to date" is guaranteed
  *without* a human rebase.
- With a batch size above 1 it validates several PRs in **one** run — turning the 5-PR / 5.1-hour drain
  above back into roughly one run.
- Cost: `ci.yml` and `gui-smoke.yml` need a `merge_group:` trigger added (a code change, and one this
  crew can do), and a red PR inside a batch causes a bisect and a re-run of the survivors.

**Recommendation: 2a items 1 and 2, plus the merge queue in 2c, and item 3 last** once the queue is
proven — item 3 removes the escape hatch, and removing it before the queue works is how a whole shift
gets blocked.

### 2d. The trap this section used to warn about **does not exist here** — and the one that does

**Withdrawn, not softened.** An earlier draft warned that `ci.yml`'s `paths-ignore` would make every
ticket-bookkeeping-only PR permanently unmergeable once item 1 is enabled, and billed that as *"the
reason to do the settings change deliberately rather than in a hurry."* **It is false of this repo.**
`ci.yml`'s `paths-ignore` sits on its **`push:`** trigger only; the `pull_request:` trigger has no path
filter at all:

```json
{"push":{"branches":["main"],"paths-ignore":[…]},"pull_request":{"branches":["main"]}}
```

`ci.yml` says so in its own words at `.github/workflows/ci.yml:62-64`: *"the `pull_request` trigger
below carries NO path filter of its own, so any change that goes through a PR — reviewed or not — still
gets the full, unfiltered run regardless of diff."* Both PR-triggered workflows (`ci.yml`,
`gui-smoke.yml`) are unfiltered on `pull_request`, and `ci-verdict` — which the old text proposed
requiring *"which would then need its own `paths-ignore` removed"* — has no `paths-ignore` to remove.
Every check in the §2a list reports on every PR. **This claim is now asserted, not written down:**
`ciPollFailClosed.test.ts` → *"every real workflow in this repo still classifies…"* enumerates
`.github/workflows/` at run time, pins the PR-triggered set to exactly `ci.yml` + `gui-smoke.yml`, and
reds naming the file if either ever grows a `pull_request:` path filter — at which point this section
needs rewriting and the test says so.

**"PR-triggered" means `pull_request` *or* `pull_request_target` (CPE-1970 round 3).** Both run on a
pull request and both land their check runs on the rollup, so both are required to judge it; until
round 3 the classifier compared the key to `"pull_request"` exactly and a `pull_request_target`-only
workflow dropped out of the required set with no trace at all — a bare `coverage=ok`. There is no such
workflow in this repo today, and that too is asserted by the same test rather than written down here.

The event list is a literal pair, so it is one of the classifier's standing blind spots — **not "the
one"**, which is what this line said through rounds 3 and 4 while each of rounds 3, 4 and 5 added a
shape to the list in `readOnBlock`'s header. A *third* PR-scoped event classifies as "not
PR-triggered" and drops its workflow out of the required set.
**Round 4 corrected what that blind spot costs.** The earlier text here said "only that `toEqual`
would notice" — it would not. `prTriggered` is a *filter*, so a workflow classified `other` is
**removed** from the array and `toEqual(["ci.yml","gui-smoke.yml"])` still holds; it can only red on
over-inclusion, or on one of those two dropping out. Measured against the real workflow set plus one
hypothetical file, `on: pull_request_review:` and `on: pull_request_v2:` each classify `false` and
each leave that assertion **green**. The classifier, the enumeration and round 3's
`pull_request_target` text grep were all silent on the same case at once.

What notices now is a **shape** check, not a name: `readOnBlock` returns the event names it parsed,
and the same test reds on any parsed `on:` event matching `/^pull_request[_a-z0-9]*$/` that is not in
`PR_EVENTS` — with an inline positive control asserting a hypothetical `pull_request_v2` workflow
does fire it. Still uncovered, and deliberately: a PR-scoped event GitHub names something else
entirely (`merge_group`), and any workflow whose `on:` block reads `unknown`, which yields no parsed
events to check. `pull_request_review` and `pull_request_review_comment` are excluded from
`PR_EVENTS` **by decision, not omission** — they fire on a review, not on `opened`/`synchronize`, so
requiring their checks would refuse every PR nobody has reviewed yet.

**Round 5: that "still uncovered" pair was a closed list, and there was a third member — the
fail-open kind.** A YAML flow collection may span lines, and the scanner captures only the remainder
of the `on:` *line*, so on the legal `on: [push,` / `  pull_request]` the continuation was invisible
to both the classifier and the parsed `events`. It answered a confident `false`, and end to end — a
board carrying every real `ci.yml` check and a `security.yml` spelled that way — it returned
`{"state":"ok", …, "detail":"every job \`main\` requires from ci.yml produced a check here"}`: round
3's `pull_request_target` defect character for character, detail string included. `readOnBlock` now
refuses an unbalanced `[`/`{` on the `on:` line and answers `unknown`, which `coverageOf` blocks on
by name. **Read the list above as "at least these", the way `readOnBlock`'s header says and this
paragraph's predecessor did not** — a sub-list under an "at least these" heading does not inherit the
hedge, and this one was read as complete for a round.

**And it is the honest answer to "why not just grep for `pull_request`".** A raw grep sees the whole
file, so it reads a continuation line and would have caught round 5's finding the day round 4 landed;
what it cannot do is tell a trigger from a comment, and `ci.yml`'s `on:` block carries ~60 lines of
commentary. All five comment positions naming a PR-ish event inside `on:` (column 0, indented,
trailing on a block key, trailing on the `on:` line, trailing after a flow seq) red a grep and are
correctly ignored by the parse; the continuation line is the reverse. **Neither instrument dominates
— they have complementary holes**, and the round-4 write-up framed the parse as simply the better
choice without saying what it gave up, which is this ticket's own shape: an instrument narrower than
the confidence placed in it. When you replace one mechanism with another, say what the old one caught
that the new one does not. Both directions are asserted in `ciPollFailClosed.test.ts` →
*"a multi-line flow `on:` was a confident `false`…"* rather than argued here.

**What is actually true and worth knowing before you click:**

- **Direct pushes are the real disruption, not bookkeeping PRs.** Of the 589 commits that landed on
  `main` in the window, **182 carry a `(#N)` squash suffix and 407 do not** — 69% of `main`'s movement
  is a direct push, not a merge. Today nothing stops them (`branches/main/protection` → 404,
  `rulesets` → `[]`). Decide explicitly what a ruleset should do with that traffic *before* enabling
  one; that, and not a phantom path filter, is what makes this a deliberate change rather than a
  hurried one.
- **A skipped job is not a missing one.** `GUI smoke (windows-latest) — tauri-driver + WebdriverIO`
  reports **skipped** on every PR by design (CPE-1594's job-level `if:`). Require the ubuntu **verdict**
  job, which always reports; see §2a.
- **The general shape the old warning described is real, just not present here.** A required check that
  never reports *is* treated by GitHub as pending forever rather than not-applicable. If a
  `pull_request:` path filter is ever added to a PR-triggered workflow, that trap arrives with it —
  which is exactly what the test above watches for.

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
| (a) "`main` moved at all since the checks ran" | **Rejected.** **589** commits landed on `main` in the window, ~41/day; a PR that sits in the queue an hour almost always sees `main` move. It would fire on nearly all 186 and train the crew to wave it through — worse than the bug. |
| (b) "the newest check run finished before a guard landed" | **Rejected, and the ticket's own evidence is what rejects it.** #1056's run finished at 18:35:13Z and it merged at 18:36:20Z — 67 seconds later, on a board that had only just gone green. A recency check on finish time passes it. It is also only an inference. |
| (c) "a job `main` requires produced no check on this board" | **Shipped.** Definite rather than inferential — the guard is not on the board, so it cannot have judged anything. No clock reasoning. Swept over the 184 evaluable merges: **168 clean, 16 firings — 15 real, 1 rename (#921)**. A gate that reds 9% of merges gets read; one that reds 95% gets aliased away. |

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
  the guard landed reports `coverage=ok`. Mitigations: the ref and its short SHA are appended to every
  verdict line **on which the guard set was actually read** — all seven verdict classes, not the two an
  earlier draft happened to cover; `--run` mode and a still-pending board never read it and say so with
  `coverage=n/a(run-mode)` / `coverage=n/a(board-pending)`. And `.claude/commands/sprint.md` tells the
  Foreman to `git fetch origin main` at the top of each sweep.
- **A workflow that contributed nothing at all** is excused **only** when its own `pull_request:`
  trigger carries a `paths:`/`paths-ignore:` filter — i.e. only when GitHub was entitled not to run it.
  An earlier draft excused *every* silent workflow, on the §2d premise that turned out to be false, and
  that blanket rule was a **whole-guard-set blind spot**: a board with zero `ci.yml` checks returned
  `coverage=ok`, exit 0, with every `ci.yml` guard absent (reproduced against the real `origin/main`).
  Measured before narrowing it, over all 186 merges by grouping each rollup on
  `checkSuite.workflowRun.workflow.name`: **0** boards were missing `CI` and **0** were missing
  `GUI smoke`, so the narrowing costs zero firings. Excused silences are still **named** on stdout, and
  the token now reads `coverage=ok(N-silent)` rather than a bare `ok`.
- **A job `main` deleted or renamed** looks the same as a job that did not run. Fail-closed direction:
  it blocks, names itself, and is one line to read. **One occurrence in the 184 swept** — #921, above.
  An earlier draft claimed zero; it tested job existence by **id** while the matcher keys on the
  **label**, which is the wrong field and therefore the wrong conclusion.

### The red-proof, both directions

`src/lib/ciPollFailClosed.test.ts` drives the real script as a subprocess and asserts on **exit code
and stdout**, never on an internal. The `guard-gap` stub emits **one identical rollup** for both legs;
only `main` changes, via two fixture directories that differ by exactly one job (asserted, not
claimed):

- `src/lib/fixtures/workflows-base-ahead/` — `main` has `ratchet-guard` → **exit 5**,
  `CI VERDICT: completed stale-checks`, the guard named, `coverage=1-unjudged`.
- `src/lib/fixtures/workflows-base/` — `main` does not → **exit 0**, `completed success`,
  `coverage=ok(1-silent)`.

Plus: an unreadable base and an empty base both → exit 5 `completed coverage-unknown`; a red board
stays exit 1 even with an unreadable base; `--run` mode prints `coverage=n/a(run-mode)`; a pending
board prints `coverage=n/a(board-pending)`. And a derivation leg (CPE-1950) reads **this repo's real
`.github/workflows`** through the shipped functions and reconstructs #1056 against the real
`ratchet-guard` label read out of `ci.yml`, so the fixtures cannot rot into agreeing with each other
about a repo that has moved on.

**Two fail-opens the first round shipped, both closed and both red-proofed by hand.** They are recorded
here because each one was a scanner narrower than the confidence placed in it — the same shape as this
page's own retracted numbers.

- **`readOnBlock` (was `workflowTriggersPullRequest`) had no comment handling at all.** A `#` at
  **column 0** anywhere inside a workflow's `on:` block ended the scan and removed **the whole
  workflow** from the required set, silently: no `silentWorkflows` line, `coverage=ok` unchanged.
  Measured on the real `ci.yml` — `true`, and `false` for the same bytes with `# a comment` inserted at
  column 0 under `on:`, after which a one-check board scored `ok`. `ci.yml`'s `on:` block already
  carries a ~60-line comment; it is indented today, and one re-wrap would have made every `ci.yml`
  guard permanently optional. It also answered `false` for `"pull_request":`, for `"on":` and for
  four-space indentation. CLAUDE.md rule 2 is explicit that a whole-line filter is not enough; there was
  none. Now: comments stripped in both positions, quoted keys and any indentation understood, and an
  `on:` block that cannot be classified returns **`null` → `coverage=unknown` → exit 5**, never a quiet
  `false`.
- **`scanWorkflowJobs` truncated the job list at a column-0 comment inside `jobs:`** —
  `scanWorkflowJobs("jobs:\n  a:\n    name: A\n# c\n  b:\n    name: B\n")` returned `["a"]`.
  Pre-existing from CPE-1906 and harmless while the only consumer was the skip matcher (a short list
  over-blocks, which is loud); the coverage check is the first consumer for which a short list **shrinks
  what `main` requires**. Same hole in a block `needs:` list, closed with it.

Each fix was red-proofed by reverting it and running the suite: the `on:` comment skip reds exactly *"a
column-0 comment inside `on:` no longer deletes the workflow from the required set"*; the `jobs:` one
reds exactly *"a column-0 comment inside `jobs:` no longer truncates the job list"*; the silent-workflow
narrowing reds exactly *"a PR-triggered workflow with NO path filter that contributed nothing is
UNJUDGED, not excused"*; and removing `${against}` from the `failure` branch reds the guard-set-ref
test, naming `failure-and-skips`. Results are written at each site, not only here.

---

## 4. The other ratchets, checked

The ticket asks whether the same shape bypasses `gui-smoke/known-failing.json`, the hex ratchet, and
the eight allowlists. **Yes — identically, and for the same single reason:** all of them are measured
by `scripts/ratchet-baselines.mjs` under the `ratchet-guard` job, so any PR whose board lacks that job
bypasses **every** one of them at once. That is not eight holes, it is one, and it is the one exit 5
now refuses: five of the fifteen exposed merges are exactly that job. What exit 5 still cannot see is a
PR whose board **has** `ratchet-guard` but whose checks predate a change to what the measurer counts —
the "guard added inside an existing job" blind spot above.

**`bidi-render-registry`'s undeclared 1552 → 1553 raise is deliberately not backdated here.** CPE-1948
already settled that in `docs/design/RATCHETS.md`: a licence row is consumable only from inside the diff
that performs the raise, so a retroactive row would be the one row on that page that is false by the
page's own definition. The movement is recorded in that document's recount, which names this ticket as
the fix.
