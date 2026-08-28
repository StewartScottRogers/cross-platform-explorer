---
id: CPE-1906
title: ci-poll.mjs robustness gaps — a hung `gh` call still crosses the cap, an error reads as pending, and a usage error exits as "CI failed"
type: bug
priority: Medium
status: Doing
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
   an error any more.
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
