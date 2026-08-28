---
id: CPE-1906
title: ci-poll.mjs robustness gaps — a hung `gh` call still crosses the cap, an error reads as pending, and a usage error exits as "CI failed"
type: bug
priority: Medium
status: Done
tags: ready
estimate: S
created: 2026-08-26
---

## Summary

CPE-1880's `scripts/ci-poll.mjs` exists so a CI poll **cannot** outlive the harness's 600 s tool cap
and get auto-backgrounded — the mechanism that stalled five agents in one run. Its deadline enforcement
was verified sound across 28 interval × `gh`-cost combinations, every one landing under the cap.

Three gaps remain in the same file, all found by CPE-1880's independent reviewer and all classified
*file, don't block*.

**1. `execFileSync` has no `timeout`, so the deadline bounds the loop but not one call.**
`ci-poll.mjs:367`. The tick loop now checks the real clock before sleeping again, which is what closed
the modelled-bound hole. But a **single** `gh` invocation that hangs is unbounded: one call costing
more than the 120 s safety margin crosses the cap, and at 300 s the run reaches ~630 s and is
backgrounded — the exact failure the script exists to make impossible.
Fix is one line: `timeout: 60_000, killSignal: "SIGKILL"`.

**2. A persistent `gh` failure is indistinguishable from a pending board.**
`ci-poll.mjs:365-369`. A bad auth token, a wrong PR number, or a network failure hits `continue` with
no failure counter, burns the entire 480 s budget, and then reports `CI still pending on unknown` with
exit 2. The caller cannot tell "CI has not finished" from "I could not ask". Bail after N consecutive
failures with a distinct exit code and say which happened.

**3. A usage error exits as "CI failed".**
`ci-poll.mjs:340`. `assertNotBackgroundable` throwing — e.g. on `--interval 0` — escapes `main()` as an
unhandled rejection and exits 1, which the file's own exit-code table (line 40) defines as *CI failed*.
It should be 64, like every other bad-usage path. Bad input reported as a red build is how someone
spends an hour debugging the wrong thing.

Related: UAT independently found that `--interval 0`, a negative `--interval`, and a nonexistent file
passed to `stall-check.mjs` all produce raw Node stack traces rather than the clean one-line usage
message every other bad-input path produces. Same class; fold it in.

**4. A comment is slightly optimistic.** `stall-check.mjs:145` cites *"the lockfile already matches, so
no further action is needed"* as a safe example. Bare, that still trips `no-further-action` (soft) — it
is clean only because the mandated handoff tail excuses it. The comment should say so, since a reader
checking the claim in isolation will find it false.

## Acceptance criteria

- [x] Bound a single `gh` call, not just the loop. Red-proof it: stub a `gh` that sleeps past the
      margin and confirm the run still returns under the cap.
- [x] Distinguish "could not ask" from "not finished", with its own exit code, after a bounded number
      of consecutive failures. Red-proof with a deliberately bad PR number and a broken auth path.
- [x] Every bad-usage path exits 64 with a one-line usage message — no raw stack traces — across both
      `ci-poll.mjs` and `stall-check.mjs`. Include `--interval 0`, negative intervals, and a
      nonexistent input file.
- [x] Correct the `stall-check.mjs:145` comment.
- [x] Do not regress the deadline guarantee while doing any of this. Re-run CPE-1880's interval ×
      `gh`-cost matrix afterwards and confirm every combination still lands under 600 s.
- [x] **Added 2026-08-27:** a `SKIPPED` check must never read as success, and the skip verdict must be
      distinct from both green and red. See section 5.

## Notes

Filed 2026-08-26 from CPE-1880's independent review (rounds 1 and 2) and its UAT, all of which passed
the PR while recording these.

Also from the same review, kept here rather than given its own ticket:
`src/lib/sprintDispatchAndCiLogGuards.test.ts:60`'s negative assertion
(`not.toMatch(/To watch CI:\s*\`gh run watch/)`) is keyed to CPE-1848's **exact sentence**, not to the
command — so a re-prescription phrased any other way would pass it. The positive `Never run …`
assertion alongside it partly covers this, but the negative one is narrower than it reads. Key it to
the command.

## 5. A SKIPPED check read as SUCCESS (added 2026-08-27; found, not assumed)

Routed in by the Foreman while this ticket was in progress; found by PR #1074's independent Reviewer
while reviewing a different ticket. It is the sharpest instance of exactly what this ticket is about, so
it is folded in rather than split out.

`ci-poll.mjs:341`'s success test read:

    rollup.every(c => c?.conclusion === "SUCCESS" || c?.conclusion === "NEUTRAL"
                   || c?.conclusion === "SKIPPED" || c?.state === "SUCCESS")

`SKIPPED` means the job **did not run**. `ci.yml`'s five Rust test jobs (`backend`, `crates`, `net-e2e`,
`sidecar`, `msrv`) all sit behind `needs: lockfile-preflight` with **no** `if:`, so a preflight failure
skips every one of them — and `main` has no branch protection at all (`branches/main/protection` → 404,
`rulesets` → `[]`), which makes this verdict the only merge gate there is.

**Per-token verdict on that line** (asked for explicitly; answered by reading the two GraphQL shapes the
rollup actually mixes):

| token | shape it serves | verdict |
|---|---|---|
| `conclusion === "SUCCESS"` | a CheckRun that ran and passed | correct, kept |
| `conclusion === "NEUTRAL"` | a CheckRun that ran and declined to judge | **kept.** GitHub's own required-check semantics treat NEUTRAL as non-blocking and the job *did* run, so it is not a "did not run". Now **counted and printed** (`neutral=`) so it is never silently equated with a pass |
| `conclusion === "SKIPPED"` | a job that did not run | **removed.** This is the fail-open |
| `state === "SUCCESS"` | the `StatusContext` arm — a legacy commit status has `state`, not `conclusion` | **kept but tightened** to `conclusion == null && state === "SUCCESS"`. It was inert for CheckRun entries only because they happen to have no `state` field, and "inert because a field is absent" stops being true after a `gh` upgrade |

Everything else — `CANCELLED`, `TIMED_OUT`, `ACTION_REQUIRED`, `STALE`, a StatusContext `ERROR`, or a
shape this code has never seen — falls through to failure. Fail closed.

**Why a blanket "SKIPPED blocks" rule was rejected — measured, not assumed.** Read live off PR #1068 on
2026-08-27: 21 SUCCESS, 2 FAILURE, and **1 SKIPPED** — `GUI smoke (windows-latest) — tauri-driver +
WebdriverIO`, which carries a job-level `if:` excluding `push` and `pull_request` (CPE-1594 took it off
the hot path) and is therefore skipped on **every** pull request by design. A rule that reds every PR
gets switched off inside a week, which is a worse outcome than the bug.

So the discrimination is **derived** (CPE-1932/CPE-1933): `explainableSkipMatchers()` reads
`.github/workflows/*.yml` at run time, collects every job carrying a job-level `if:` plus the transitive
closure of jobs that `needs:` one, and treats a skip of those as by-design. Any other skip did not run
and nobody asked for it to be skipped → exit **4**, a distinct verdict naming every check involved. An
empty workflow scan yields `null` and makes **every** skip unexplained: fail closed.

A distinct exit code rather than folding into `1`, as the Foreman preferred: "a job was skipped" and "a
job failed" call for different responses — find the cascade versus read the logs — and collapsing them
loses the information the fix exists to preserve. A real FAILURE still outranks a skip.

Complementary to `scripts/ci-verdict.mjs` (PR #1074), which closes the *workflow* half of the same hole
by emitting a real `FAILURE` that this line could not swallow. Not touched here.

## Work Log

**2026-08-27 — worked and shipped.**

Fixes, all in `scripts/ci-poll.mjs` unless noted:

1. **Error reads as pending → exit 3, `CI VERDICT: unknown`.** `gh` failures are counted;
   `MAX_CONSECUTIVE_GH_FAILURES = 3` in a row ends the poll immediately, and *any* run that finishes with
   zero successful reads and at least one failure takes the same path. The line says which kind of
   failure (`gh exited non-zero` / `timed out` / `unparseable output` / `gh not found`), says nothing was
   read, and says what to do. Neither `CI VERDICT: pending` nor `CI still pending on …` is reachable from
   a **thrown** `gh` failure any more. *(Round 2 correction: as first written this said "from an error",
   which was measurably false — a `gh` that exits 0 with a wrong-shaped payload throws nothing and went
   straight to the pending verdict. See the round-2 entry below; the claim is now true of every error
   path, but only because the shape guard was added, not because this sentence was right.)*
2. **Hung call crosses the cap → bounded per call.** `execFileSync` now carries
   `timeout: ghCallTimeoutMs(now, deadline)` + `killSignal: "SIGKILL"`, the per-call budget being
   `clamp(deadline - now, 5 s, 60 s)`. The structural worst case is now `budget + 5 s`
   (`boundedWallClockMs`) and takes neither `ghCostMs` nor `--interval` as an input — the model is no
   longer load-bearing. `assertNotBackgroundable` checks the structural bound too.
3. **Usage errors exit 64, never 1.** `--interval` is validated where it is read; `parseArgs`,
   `assertNotBackgroundable` and `planTickCount` share one `try` that exits 64 with a one-line message
   plus usage. A crash escaping `main()` now exits **3** ("nothing was determined"), never 1 ("CI
   failed"), and never a raw stack trace. `stall-check.mjs` gets the same for a nonexistent input file
   and a bad `--prior`.
4. **`stall-check.mjs`'s `no-further-action` comment corrected** — the cited example is clean *in
   context* (the mandated handoff tail excuses a soft match), not in isolation; the test pins both halves.
5. **`SKIPPED` never counts as success** — section 5 above.
6. **Job age reported** — `oldest_pending_min=` plus the oldest pending check's *name*, in both the tick
   lines and the verdict. Deliberately **not** thresholded: "over N minutes means hung" would be an
   invented number, and this repo's median run is 58.9 min, so any plausible threshold fires constantly.
   Reporting the number and the name is what makes the sibling-PR comparison mechanical.
7. **`sprintDispatchAndCiLogGuards.test.ts`'s narrow negative assertion re-keyed** (from the Notes) — it
   pinned CPE-1848's exact sentence, so a re-prescription phrased any other way passed. Now keyed to the
   *command*, scoped to the blockquoted instruction block so the surrounding history may keep naming it.
8. **`scripts/organize-done.mjs`** — same defect class, found by the wrapper sweep below and fixed while
   here: a failed auto-commit printed "auto-commit skipped: …" to **stdout** and exited **0** *after* the
   files had already been renamed, so "files moved, nothing committed" was indistinguishable from a clean
   archive. Now stderr and a non-zero exit. Its `git diff --cached --quiet` also could not tell exit 1
   ("staged changes") from exit 128 ("git broke") — now read from `status` explicitly.

**Red-proofs** (each sabotage applied, tests run, sabotage reverted):

| sabotage | result |
|---|---|
| hide `skippedNames` (reproduces the old fold-into-success) | **4 skip tests red**; the verdict returns to `completed success` on a rollup with five skipped Rust jobs |
| remove `timeout` from `gh()` | a `--budget 3` run was still going at **60 s** when an external kill fired (exit 124), against an 8 s structural bound; the vitest case reds at **120 042 ms > 45 000 ms** |
| `if (false)` on the could-not-ask branch | **4 error tests red**; the verdict returns verbatim to `CI VERDICT: pending — … no reads yet. CI still pending on unknown — re-invoke this poll or hand CI to the Foreman.` |

**Interval × gh-cost matrix re-run** (acceptance criterion 5): 200 combinations — 4 budgets × 10
intervals (5…120 s, including CPE-1880's hostile 17 s) × 5 `gh` costs (1…60 s). Every one lands under
600 s. The assertion is now on `boundedWallClockMs`; the test also pins that the old *model* DOES cross
the cap at a 60 s `gh` call, so the matrix is not vacuous.

**Other `scripts/` external-tool wrappers, enumerated** with `git ls-files 'scripts/*.mjs'` and
`git ls-files 'scripts/**/*.mjs'` (9 scripts; `scripts/ci-verdict.mjs` confirmed absent from this
worktree — unmerged in PR #1074, not touched):

| script | external tool | verdict |
|---|---|---|
| `audit-npm-projects.mjs` | `git ls-files`, `npm audit`, `npm audit fix` | FAIL-CLOSED (residual: no spawn `timeout`) |
| `ci-poll.mjs` | `gh` | was FAIL-OPEN — **fixed here** |
| `organize-done.mjs` | `git rev-parse/add/diff/commit` | was FAIL-OPEN — **fixed here** |
| `ratchet-baselines.mjs` | `git diff/show/rev-parse` | FAIL-CLOSED — reds explicitly when the base cannot be resolved or measured |
| `stall-check.mjs` | none | CLEAN |
| `dev-harness/layout-guard/cases.mjs` | none | CLEAN |
| `dev-harness/layout-guard/engine.mjs` | `spawn(chrome)` + CDP | FAIL-CLOSED — per-call CDP timeouts |
| `dev-harness/layout-guard/run.mjs` | `spawn(npm)`, `spawn(taskkill)` | FAIL-CLOSED |
| `dev-harness/sidebar-drop-stack-overlap/check.mjs` | `spawn(chrome)`, `spawn(npm)` | FAIL-CLOSED for results, but its CDP `send` has no per-call timeout — the CPE-1882 fix `engine.mjs` received and this older prototype did not. Not wired into CI, so the blast radius is a developer's terminal |

**Cross-cutting, not fixed here:** every FAIL-CLOSED script above except `run.mjs`/`engine.mjs` spawns
without a `timeout`, and `npm-audit-sweep` / `ratchet-guard` carry no `timeout-minutes`, so their ceiling
is Actions' 6-hour default. Their error *classification* is sound; their answer to "the tool hung" is to
not answer at all. Worth its own ticket.

Related: **CPE-1880** (the scripts), **CPE-1907** (the stall detector over-flagging this app's own
background vocabulary).

---

**2026-08-27 — round 2 (review findings on PR #1078).**

**BLOCKER — a `gh` that exits 0 with the wrong shape was still read as "pending", and in `--run` mode as
GREEN.** Round 1 counted *thrown* `gh` failures. A well-formed JSON payload of the wrong shape throws
nothing, so it walked past the counter into the readers, whose deliberately defensive
`Array.isArray(json?.statusCheckRollup) ? … : []` turned an absent rollup into `total_count=0` — which
`decideFromReads` reports as *"no checks scheduled yet"*. Structurally the same defect CLAUDE.md already
records for `audit-npm-projects.mjs` (npm's `--json` error path is well-formed JSON with no `metadata`
key), re-emitted one layer down inside the guard built to close that class. Not hypothetical: GraphQL's
`statusCheckRollup` is nullable and GitHub answers a field-level failure with HTTP 200 plus a partial
`data` and an `errors` array.

Resolution **(a)** — reject the payload, do not soften the claim. Measured, `gh` exit 0 in every row:

| payload | before | after |
|---|---|---|
| `--pr` `{"message":"Not Found","documentation_url":…}` | `pending — total_count=0 … CI still pending on unknown`, **exit 2** | `unknown — could not ask GitHub (unexpected payload shape)`, **exit 3** |
| `--pr` `{"data":null,"errors":[{"message":"Could not resolve"}]}` | pending, exit 2 | unknown, exit 3 |
| `--pr` `null` / `"nope"` / `[1,2,3]` | pending, exit 2 | unknown, exit 3 |
| `--pr` `{…,"statusCheckRollup":null}` | pending, exit 2 | unknown, exit 3 |
| `--run` `{"status":"completed","conclusion":"success"}` (no `jobs`) | **`completed success`, exit 0 — GREEN** | unknown, exit 3 |
| `--run` `{…,"jobs":[]}` | `completed success`, **exit 0** | `completed did-not-run`, exit 4 |
| `--pr` `{…,"statusCheckRollup":[]}` — a REAL check-less PR | pending, exit 2 | **pending, exit 2 (unchanged)** |
| `--pr` a real green rollup | `completed success`, exit 0 | **unchanged** |
| `--run` 20 jobs ran + 1 by-design skip | `completed success`, exit 0 | **unchanged** |

The `--run` row in bold is why (a) beat (b): this was not merely a wrong wait, it was a **green verdict
on a board nobody ever saw**, on a repo whose `main` has no branch protection. `assertReadableShape()`
demands the fields we actually asked `gh` for — `statusCheckRollup` as an ARRAY for `--pr`, both `jobs`
(array) and `status` (string) for `--run` — and throws `GhPayloadShapeError`, which goes through the
existing `classifyGhFailure` → counter → exit 3 path as its own kind, `unexpected payload shape`. The
discrimination is structural rather than a heuristic, which is what makes it safe against a genuinely
check-less PR: an empty board still has the array and a real `headRefOid`; `sha=unknown mergeable=n/a`
beside `total_count=0` is a combination no real board produces. For `--run` the guard is deliberately
stricter than the review's suggested "no `jobs` **and** no `status`", because "`status` but no `jobs`" is
precisely the row that exited 0.

**Finding 1 — two predicates for "is this red", now one.** `formatVerdict` branched on `failedNames`;
the exit branched on `failedNames || conclusion === "failure"`. Measured disagreements: a board whose
only finished checks were by-design skips printed `CI VERDICT: completed skipped — … Skipped by design:
…` and exited **1** ("at least one check FAILED") with zero failures; and `completed skipped` was
*simultaneously* the exit-4 prefix, so the prefix discriminated nothing. New: `verdictClass(decision,
latest, unexplainedSkips)` is the single classifier, and both the line and `process.exit` read it.
Prefix→code is now one-to-one — `completed success`→0, `completed failure`→1, `pending`→2, `unknown`→3,
`completed did-not-run`/`completed unclear`→4 — pinned by a test that runs the whole stub matrix and
fails if any prefix maps to two codes. The exit-4 prefix was renamed off `completed skipped` for the
same reason. Greenness now also requires that something actually RAN (`ranCount`), so an all-skipped
board is exit 4 rather than 1; a run with 20 successes and one by-design skip stays exit 0 (measured).

**Finding 2 — prefix matching on bare job ids was the fail-open direction.** Four of the six matchers
this repo derives are ids of `name:`-less jobs. Measured on the pre-fix code: the prefix `"catalog"`
explained `"catalog"`, `"catalog-freshness nightly"` **and** `"catalogue rebuild"`. After: only
`"catalog"`; the other two are unexplained. A matcher is now `{text, prefix}` and is a prefix **only**
when the job's `name:` contains a template expansion GitHub fills in at run time — everything else is
compared exactly. The repo's live matcher set is unchanged in effect: `GUI smoke (windows-latest) —
tauri-driver + WebdriverIO` is still explained, `MSRV check` still is not.

**Finding 3 — the `if:` block-mapping form: fixed, and the comment corrected.** `if:` alone on its line
with the expression indented beneath is legal YAML and scanned as `conditional=false` (measured). It
fails *closed*, but reformatting `gui-smoke.yml:121` onto two lines would then have exited 4 on every
PR. The scanner now looks ahead one non-blank line for a deeper-indented continuation; `if: >-` already
worked (the `>` satisfies the same-line test) and still does. The over-claiming half is **documented
rather than tightened**, at the site: "explained" means the job carries a job-level `if:` *at all*,
unevaluated — including `always()` / `!cancelled()`, whose jobs cannot legitimately skip. Narrowing is
not free and would not be safe by inspection: the general rule must stay "carries a condition" because
conditions like `github.event.workflow_run.conclusion != 'success'` skip on every HEALTHY run, so
separating "can legitimately be false" from "is always true" needs a GitHub-expression evaluator rather
than a line scan. The residual fail-open is now stated exactly — a job whose `if:` is a tautology is
excused if it ever skips — instead of being implied away.

**Finding 4 — a pending verdict was silent about `gh` failures it survived.** `gh_failures=N` is now on
the totals line, **appended after `sha=`**, so the interface pin (presence + relative order) is
untouched; the pin was extended to require the new key last. Measured: one good read then failures that
never reach the 3-in-a-row bail → `pending … gh_failures=5`, exit 2, where the line previously mentioned
them nowhere.

**Finding 5 — the `.gitattributes` pin, and the guard that could not see it.** `src/lib/fixtures/ghStub.mjs`
matched neither `scripts/*.mjs` nor `scripts/**/*.mjs` and checked out CRLF (131) against an LF blob.
Rather than add a third directory rule, the pin is now `*.mjs text eol=lf` — and generalising the guard
from a `scripts/` tree walk to a `git ls-files` enumeration immediately found a **third** unpinned file
nobody had named, `sidecar/agent-board/clickthrough.mjs` (344 CRLF). Both worktree copies normalised. A
second guard now asserts `git check-attr eol` reports `lf` for every tracked `.mjs`, because the CRLF
check alone passes on a Linux runner whatever `.gitattributes` says.

**Red-proofs** — the pre-fix script was loaded from its own HEAD blob into a scratch file and run
against every input above; it answered the wrong way in every row of the table (including
`conditional=false` on the block-mapping `if:`, and the three-way `catalog` over-match). The
`.gitattributes` widening was red-proofed by the new guard failing on `clickthrough.mjs` before the pin
was broadened. **Endorsed and left alone:** no threshold on job age (a repo-wide constant is a category
error twice over — the 58.9-min median is whole-run wall clock, and the useful comparison is per-job);
the `CI_POLL_GH_SCRIPT` seam (unreachable in production, implies no privilege a PATH shim does not).
**Not ours:** the residual spawn/`timeout-minutes` gap is **CPE-1967**.

## Closed 2026-08-27 — what the gauntlet actually proved

Merged as PR #1078, **fully green — 24 of 24 checks, no failures** — after two rounds. This is the tool
the Foreman merges on, so the record matters more than usual.

**Three fail-open paths fixed, and the sweep found a fourth elsewhere.** An error now produces
**exit 3** (`unknown — could not ask GitHub`) rather than `pending`, a hung `gh` can no longer cross the
advertised budget, and an unexplained skip produces **exit 4** (`completed did-not-run`). `scripts/organize-done.mjs`
was found fail-open in the same class and fixed: a failed auto-commit printed to stdout and exited **0**
*after renaming files*, and `git diff --cached --quiet` could not tell exit 1 from exit **128** — the
same exit-code confusion as the bash `[ -lt ]` returning 2 earlier that day.

**The Foreman's routing was wrong and the worker refused it, correctly.** Having found
`ci-poll.mjs:341` counting `SKIPPED` as success, I routed *"treat SKIPPED as not-success."* The worker
measured first: **`GUI smoke (windows-latest)` is SKIPPED on every PR** by its own job-level `if:`, so a
blanket rule would have reddened every board forever — not fail-closed, **broken**, and reverted within
a day taking the real fix with it. What shipped derives the distinction from the workflow files: a job
with an `if:` **plus its transitive `needs:` closure** is by-design; anything else did not run; an empty
scan treats every skip as unexplained. **The discrimination was the work.** Collapsing the category only
looks like rigour.

**Round 1's Reviewer found a SEVENTH fail-open of the day — inside the fix for the class.** The file
claimed *"neither `CI VERDICT: pending` nor `CI still pending on …` is reachable from an error any
more."* False: a `gh` that exits **0** returning well-formed JSON of the **wrong shape** (a GraphQL
partial `{"data":null,"errors":[…]}`, `null`, a bare string) landed on `total_count=0` → pending.
Structurally identical to the `audit-npm-projects.mjs` bug CLAUDE.md already records.

**And round 2 found it was worse than reported.** The review's matrix covered `--pr` mode, where the
wrong shape exits 2 — never green. In **`--run` mode**, `{"status":"completed","conclusion":"success"}`
with **no `jobs` array** read as terminal + success + `total_count=0` and exited **0 — GREEN**, on a repo
with no branch protection. That removed the document-it option outright.

**Two red predicates were deciding "is this red" independently** — `formatVerdict` branched on
`failedNames` while the exit code branched on `failedNames || conclusion === "failure"` — so an
all-by-design-skipped board printed `completed skipped` and exited **1**, which the file's own table
defines as "at least one check FAILED", with zero failures. Unified behind one `verdictClass()`, pinned
by a test that fails if any prefix maps to two exit codes.

**Job age is reported and deliberately NOT thresholded.** The median run here is **58.9 min** *whole-run
wall clock*, so any threshold under it fires on healthy jobs and any over it fires on nothing — and the
useful comparison is per-job (`Frontend` at 19m is alarming; `Server crates (windows-latest)` at 19m is
normal). Printing the age **and the name** hands the caller the diagnostic without minting an unmeasured
constant. That is the hour the Foreman lost comparing timestamps by hand, fixed properly.

**Prefix matching on bare job ids was fail-open**: `"catalog"` excused `"catalog-freshness nightly"` and
`"catalogue rebuild"`. Now exact unless the job's `name:` carries a `${{ … }}`.

**Postscript, the same evening:** the Foreman then read `pend=0 fail=0` off a hand-rolled `jq` for a PR
whose board was **empty** after a force-push — the very case this tool already refuses. Recorded in
`history.md`. **Use the tool.**
