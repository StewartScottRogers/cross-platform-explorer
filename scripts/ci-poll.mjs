#!/usr/bin/env node
// CPE-1880 — a CI poll that CANNOT be backgrounded, replacing `gh run watch` in the dispatch contract.
// CPE-1906 — and that cannot report an error, a hang, or a job that never ran as if CI had answered.
// CPE-1970 — and that cannot report a board GREEN when a guard `main` already carries never appeared
//            on it at all. `main` has NO branch protection (`branches/main/protection` → 404,
//            `rulesets` → []), so nothing stops a merge on checks that predate a guard. Measured by
//            running `coverageOf()` below over the 186 PRs merged 2026-08-14T00:00:00Z →
//            2026-08-28T11:05:17Z (inclusive at both ends; #1090 merged at exactly the upper bound)
//            against each PR's own live check rollup: 16 merged with at least one job `main` already
//            required entirely absent from their board, 168 clean, 2 fail-closed unreadable (#896 and
//            #899, whose squash commits are no longer reachable from `main`). The absent jobs:
//            `ratchet-guard` ×5 (PR #1056 among them — the merge that found this ticket),
//            `ci-verdict` ×5, `lockfile-preflight` ×2, `msrv` ×2, `ffmpeg-pin-guard` ×1,
//            `gui-smoke-linux` ×1 (#921 — and that one is NOISE, not exposure: the PR being judged
//            renamed that job itself, so the exposure count is **15**; see `coverageOf`'s header).
//            See docs/design/CI-STALENESS.md for the settings change that would make it impossible
//            rather than merely visible — it needs the repository owner, which is why this ships too.
//
// WHY THIS EXISTS (the measurement, not a vibe):
//   The Claude Code Bash tool caps a single call at `timeout: 600000` ms (10 minutes). When a command
//   outlives that cap the harness does not kill it — it AUTO-BACKGROUNDS it, and the calling agent is
//   left holding a background task whose completion notification a sub-agent can never receive. It then
//   does the only thing that follows: it waits. Forever.
//
//   `gh run watch` blocks until the run finishes. Measured on this repo (95 completed `ci.yml` runs,
//   2026-08-23 → 2026-08-26): median wall clock 58.9 min, p90 77.3 min, max 97.0 min. Of the 71 runs
//   that SUCCEEDED, **zero** finished inside 600 s — the fastest took 28.6 min. The only four runs under
//   ten minutes were all `cancelled`. So `gh run watch` on this repo is not "occasionally too slow"; it
//   is backgrounded 100% of the time. CPE-1848's dispatch contract PRESCRIBED that command, which is why
//   telling agents harder could never work — they complied, and complying is what stalled them.
//
//   A shell-level `timeout 570 gh run watch …` wrapper does NOT fix it: the harness timer spans the whole
//   compound command rather than the wrapped process, and it was observed backgrounded anyway. Do not
//   reach for that; it is the fix everyone tries and it does not hold.
//
// WHAT THIS DOES INSTEAD
//   A bounded foreground poll whose worst-case wall clock is structurally clamped well below the cap. It
//   always terminates, always prints one timestamped line per tick (satisfying the loop-timestamp
//   convention), and always ends with exactly one machine-readable `CI VERDICT:` line carrying the totals
//   sprint.md requires — `total_count`, `pending`, `mergeable`, the SHA. Re-invoke it as many times as the
//   run needs; each invocation returns.
//
//   It also mechanises the poll traps sprint.md documents in prose, so they cannot be forgotten:
//   `total_count == 0` is never read as green (it is reported, with `mergeable` alongside, because a
//   CONFLICTING PR schedules no checks at all), and `pending == 0` is only trusted once `total_count` has
//   been STABLE ACROSS TWO READS (jobs schedule in waves, so pending dips before it rises).
//
// CPE-1906 — THREE WAYS THIS TOOL USED TO FAIL OPEN, AND WHAT REPLACED THEM
//   The house rule from four separate incidents in one sprint (a bash `[ -lt ]` exit 2 read as false;
//   npm's `--json` error path read as a clean audit; a catalog job green having published nothing; CI
//   test jobs SKIPPED rather than failed): **a wrapper around an external tool must distinguish "ran and
//   found nothing" from "did not run", and fail closed on the latter.** This file broke that rule three
//   times.
//
//   1. AN ERROR READ AS PENDING. A bad token, a wrong PR number or a dead network hit `continue` with no
//      counter, burned the whole budget, and printed `CI VERDICT: pending`. The Foreman then waited for
//      something that would never arrive — and an indefinitely pending verdict is indistinguishable from
//      a slow-but-healthy run. Now: consecutive `gh` failures are counted, `MAX_CONSECUTIVE_GH_FAILURES`
//      of them ends the poll immediately, and ANY run that reaches the end with no successful read at all
//      prints `CI VERDICT: unknown` and exits 3. Exit 3 never means "wait"; it means "I could not ask".
//
//      ROUND 2 CLOSED THE OTHER HALF OF THIS, AND IT WAS THE SAME BUG ONE LAYER DOWN. Counting THROWN
//      failures only closes the path where `gh` throws. A `gh` that exits 0 and prints well-formed JSON
//      of the WRONG SHAPE threw nothing, so it sailed past the counter into `readFromPrJson`, whose
//      `Array.isArray(json?.statusCheckRollup) ? … : []` turned an absent rollup into `total_count=0` —
//      which `decideFromReads` reports as "no checks scheduled yet". Measured: `{"message":"Not
//      Found"}`, `{"data":null,"errors":[…]}` (GraphQL answers a field-level failure with HTTP 200,
//      partial `data` and an `errors` array, and `statusCheckRollup` is a NULLABLE field), `null`,
//      `"nope"` and `[1,2,3]` all printed `CI VERDICT: pending — total_count=0 … CI still pending on
//      unknown` and exited 2. That is "did not run" reported as "not finished" — and it is exactly the
//      defect CLAUDE.md already records for `audit-npm-projects.mjs`, where npm's `--json` error path
//      emits well-formed JSON with no `metadata` key and a parse-only check read an unreachable
//      registry as a clean audit. In `--run` mode it was worse than a wrong wait: a payload with no
//      `jobs` key (`{"status":"completed","conclusion":"success"}`) read as `total_count=0` +
//      `terminal` + `conclusion: success` and exited **0 — GREEN, on a board nobody ever saw**.
//      `assertReadableShape()` now rejects a payload that does not answer the question we asked, and
//      routes it through `classifyGhFailure` like any other `gh` failure → exit 3. The discrimination
//      is structural, not a heuristic: we ASK `gh` for `statusCheckRollup` (or `status,…,jobs`), and a
//      response that answered has them. A genuinely check-less PR still returns
//      `statusCheckRollup: []` — an ARRAY, plus a real `headRefOid` — so it stays a pending board.
//   2. A HUNG `gh` CALL CROSSED THE CAP. `execFileSync` had no `timeout`, so the deadline bounded the
//      LOOP but not one CALL: a single 300 s hang put the run at ~630 s and it was auto-backgrounded —
//      the exact defect this file exists to make impossible. Now every call is bounded by
//      `ghCallTimeoutMs()`, which is the smaller of `GH_CALL_TIMEOUT_MS` and the time left on the
//      deadline, floored so a near-deadline call still gets a real chance. Structural worst case is now
//      `budget + GH_MIN_CALL_TIMEOUT_MS`, not `budget + ∞`.
//   3. A SKIPPED JOB READ AS SUCCESS. The old success test was
//      `conclusion === "SUCCESS" || "NEUTRAL" || "SKIPPED" || state === "SUCCESS"`. `SKIPPED` means the
//      job DID NOT RUN — `ci.yml`'s five Rust test jobs (`backend`, `crates`, `net-e2e`, `sidecar`,
//      `msrv`) all sit behind `needs: lockfile-preflight` with no `if:`, so a preflight failure skips
//      every one of them. Folding that into "success" destroys the only evidence that the Rust suite
//      never executed, on the verdict this crew merges on, against a `main` that has no branch
//      protection at all (`branches/main/protection` → 404, `rulesets` → `[]`). See CPE-1956, and
//      `scripts/ci-verdict.mjs` (PR #1074) which closes the workflow half of the same hole.
//
//      BUT A BLANKET "SKIPPED BLOCKS" RULE IS UNUSABLE, AND THAT IS MEASURED, NOT ASSUMED. Read live off
//      PR #1068 on 2026-08-27: 21 SUCCESS, 2 FAILURE, and 1 SKIPPED —
//      `GUI smoke (windows-latest) — tauri-driver + WebdriverIO`, which carries a job-level `if:`
//      excluding `push` and `pull_request` and is therefore skipped on EVERY pull request by design
//      (CPE-1594 took it off the hot path). A rule that reds every PR gets switched off in a week, which
//      is a worse outcome than the bug.
//
//      So the discrimination is DERIVED, not recalled (CPE-1932/CPE-1933): `explainableSkipMatchers()`
//      reads `.github/workflows/*.yml` at run time, collects every job that carries a job-level `if:`
//      plus the transitive closure of jobs that `needs:` one, and treats a skip of those as by-design.
//      Any OTHER skipped check did not run and nobody asked for it to be skipped — that is a distinct
//      verdict with a distinct exit code (4), never a green one. If the workflow scan comes back empty
//      the matcher set is `null` and EVERY skip is unexplained: fail closed, loudly.
//
// USAGE
//   node scripts/ci-poll.mjs --run <run-id>            # poll one workflow run
//   node scripts/ci-poll.mjs --pr <number>             # poll a PR's whole check rollup
//   node scripts/ci-poll.mjs --pr 1031 --budget 300    # shorter budget, in seconds (clamped, never raised)
//
// EXIT CODES — and the verdict PREFIX that goes with each, one-to-one
//   0  `completed success`     every check that ran concluded success (or NEUTRAL, which GitHub itself
//                              treats as non-blocking), at least one check actually ran, and every
//                              skipped check was skipped by design
//   1  `completed failure`     at least one check FAILED (or was cancelled/timed out), or the run-level
//                              conclusion is `failure` — read the logs
//   2  `pending`               still pending when the budget ran out — this is a NORMAL, EXPECTED
//                              outcome, not an error. Report the printed `CI still pending on <SHA>`
//                              line and hand CI to the Foreman, or re-invoke.
//   3  `unknown`               COULD NOT ASK. `gh` errored, hung, returned unparseable output, or
//                              returned well-formed JSON that is not a board at all. NOT a pending
//                              board and NOT a green one — nothing was read. Do not merge and do not
//                              wait: check `gh auth status`, the run/PR id and the network, re-invoke.
//   4  `completed did-not-run` finished with NO failure, but nothing usable ran: a check was SKIPPED
//                              with no job-level `if:` to explain it (a `needs:` cascade), or every
//                              check that finished was skipped, or the board finished empty.
//      `completed unclear`     finished in a shape this poll has never seen — no failure, but no
//                              positive evidence of success either. Both are "not red, not green": do
//                              not merge, find out why.
//   5  `completed stale-checks`  nothing is red, and that is the problem: a job `main` requires
//                              produced NO check on this board, so a guard that exists on `main` never
//                              judged this PR. Rebase onto `main` and let CI re-run. (CPE-1970)
//      `completed coverage-unknown`  the coverage check itself could not be computed — `main`'s
//                              workflow files were unreadable. "Did not run", not "found nothing".
//   64 bad usage
//
//   CPE-1906 round 2 — THE PREFIX AND THE EXIT CODE ARE COMPUTED FROM ONE PREDICATE (`verdictClass`),
//   because they were computed from two and the two disagreed. `formatVerdict` branched on
//   `failedNames` while the exit branched on `failedNames || conclusion === "failure"`, so a board whose
//   only finished checks were by-design skips printed `completed skipped` and exited **1** — "at least
//   one check FAILED", with zero failures — and `completed skipped` was simultaneously the prefix for
//   exit 4, so the prefix discriminated nothing. Every branch above now comes out of the single
//   classifier, and a test pins prefix→code.
//
// The pure functions below are exported and unit-tested by `src/lib/sprintStallControls.test.ts`; the
// `main()` path only runs when this file is executed directly. `src/lib/ciPollFailClosed.test.ts` drives
// the REAL script as a subprocess against a stubbed `gh` and asserts on the verdict line and exit code,
// which is what the caller actually consumes.

import { execFileSync } from "node:child_process";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

/**
 * The Claude Code Bash tool's documented maximum `timeout`, in milliseconds. A single tool call that
 * outlives this is auto-backgrounded — which is the entire defect CPE-1880 exists to close.
 */
export const HARNESS_TOOL_TIMEOUT_MS = 600_000;

/**
 * Head-room between our worst-case wall clock and the cap. It has to absorb everything the tick loop
 * does NOT control: process start-up, the `gh` round-trip on the final tick, a slow network, and the
 * harness's own accounting. 120 s is deliberately generous — the cost of being wrong here is a stalled
 * agent, and the cost of being conservative is one extra re-invocation.
 */
export const SAFETY_MARGIN_MS = 120_000;

/** The largest wall-clock budget a poll may ever ask for. Not configurable upward — see clampBudgetMs. */
export const MAX_BUDGET_MS = HARNESS_TOOL_TIMEOUT_MS - SAFETY_MARGIN_MS;

/** Default seconds between ticks. Matches the interval the old `gh run watch` line used. */
export const DEFAULT_INTERVAL_MS = 30_000;

/** Default wall-clock budget for one invocation. */
export const DEFAULT_BUDGET_MS = MAX_BUDGET_MS;

/**
 * Ceiling on ONE `gh` invocation (CPE-1906 gap 1). A healthy `gh pr view` on this repo returns in a
 * couple of seconds; anything past a minute is hung, not slow, and waiting longer buys nothing.
 */
export const GH_CALL_TIMEOUT_MS = 60_000;

/**
 * Floor on one `gh` invocation. The per-call timeout shrinks as the deadline approaches so a hung call
 * can never push the process past the budget — but it must not shrink to zero, or the last tick could
 * not complete a read at all and every poll would end "could not ask".
 */
export const GH_MIN_CALL_TIMEOUT_MS = 5_000;

/**
 * How many `gh` failures in a row end the poll (CPE-1906 gap 2). Three, not one: a single transient
 * network blip should not abort a poll that would otherwise have succeeded on the next tick. Three in a
 * row is not a blip, and burning the remaining budget on a call that keeps failing tells the caller
 * nothing it does not already know.
 */
export const MAX_CONSECUTIVE_GH_FAILURES = 3;

/**
 * Clamp a requested budget to something that cannot be auto-backgrounded.
 *
 * A caller may ask for LESS time, never more: no flag, env var, or argument raises this ceiling.
 *
 * But be precise about what that buys, because the first version of this comment was not. Clamping the
 * BUDGET does not by itself bound the WALL CLOCK — the review's finding. `--interval` is a separate
 * input, it drives the tick count, and `assertNotBackgroundable` only *models* the result once up front
 * using a fixed `ghCostMs` guess. If a real `gh` call costs 15 s instead of the assumed 5 s, the shipped
 * defaults run 690 s and the process is backgrounded regardless of what this function returned.
 *
 * So the clamp is the plan, and `main()`'s **deadline check is the enforcement**: the loop reads the
 * clock every tick and stops when the next sleep would cross `started + budgetMs`. That is what makes
 * the guarantee hold against a slow network, and it is also why `--interval` needs no floor of its own.
 *
 * CPE-1906 added the third leg: the deadline bounded the LOOP, but one `gh` call was unbounded, so a
 * single hang crossed the cap anyway. `ghCallTimeoutMs()` bounds the call, which is what makes
 * `boundedWallClockMs()` a real bound rather than another model.
 *
 * @param {number} requestedMs
 * @returns {number}
 */
export function clampBudgetMs(requestedMs) {
  if (!Number.isFinite(requestedMs) || requestedMs <= 0) {
    throw new RangeError(`budget must be a positive number of milliseconds, got ${requestedMs}`);
  }
  return Math.min(requestedMs, MAX_BUDGET_MS);
}

/**
 * The per-call `gh` timeout for a call starting at `nowMs` against `deadlineMs`.
 *
 * Never longer than `GH_CALL_TIMEOUT_MS`, never longer than the time actually left, never shorter than
 * `GH_MIN_CALL_TIMEOUT_MS`. The floor is what makes the last tick able to read at all; it is also the
 * only term by which the process can outlive its own deadline, which is why `boundedWallClockMs()` adds
 * exactly it and nothing else.
 *
 * @param {number} nowMs
 * @param {number} deadlineMs
 * @returns {number}
 */
export function ghCallTimeoutMs(nowMs, deadlineMs) {
  const remaining = deadlineMs - nowMs;
  return Math.max(GH_MIN_CALL_TIMEOUT_MS, Math.min(GH_CALL_TIMEOUT_MS, remaining));
}

/**
 * The STRUCTURAL worst-case wall clock, as opposed to `worstCaseWallClockMs`'s model. The loop cannot
 * start a sleep that would cross the deadline, and the last `gh` call is bounded by `ghCallTimeoutMs()`,
 * whose largest possible value once the deadline has passed is the floor. So the process ends no later
 * than this, whatever `gh` does.
 *
 * @param {number} budgetMs
 * @returns {number}
 */
export function boundedWallClockMs(budgetMs) {
  return clampBudgetMs(budgetMs) + GH_MIN_CALL_TIMEOUT_MS;
}

/**
 * Worst-case wall clock for a poll plan: every tick sleeps except the last, plus one `gh` round-trip
 * allowance per tick. Exported so a test can assert the plan stays under the harness cap rather than
 * trusting the constants to have stayed sane.
 *
 * @param {number} budgetMs
 * @param {number} intervalMs
 * @param {number} [ghCostMs] pessimistic allowance for one `gh` call
 * @returns {number}
 */
export function worstCaseWallClockMs(budgetMs, intervalMs, ghCostMs = 5_000) {
  const ticks = planTickCount(budgetMs, intervalMs);
  return ticks * ghCostMs + (ticks - 1) * intervalMs;
}

/**
 * How many ticks fit in the budget. Always at least one, so a tiny budget still produces a real read
 * and a real verdict instead of an empty return.
 *
 * @param {number} budgetMs
 * @param {number} intervalMs
 * @returns {number}
 */
export function planTickCount(budgetMs, intervalMs) {
  if (!Number.isFinite(intervalMs) || intervalMs <= 0) {
    throw new RangeError(`interval must be a positive number of milliseconds, got ${intervalMs}`);
  }
  const budget = clampBudgetMs(budgetMs);
  return Math.max(1, Math.floor(budget / intervalMs));
}

/**
 * Fail loudly if the module's own constants ever drift into backgroundable territory. Called at start-up
 * so a bad edit reds immediately instead of silently reintroducing the stall.
 *
 * `capMs` is a parameter rather than a hard-coded read of HARNESS_TOOL_TIMEOUT_MS for one reason: it is
 * the only way to exercise this assertion in a test. `clampBudgetMs` makes the failure unreachable from
 * any argument (which is the point), so a test can only reach it by shrinking the cap.
 *
 * @param {number} [budgetMs]
 * @param {number} [intervalMs]
 * @param {number} [capMs]
 */
export function assertNotBackgroundable(
  budgetMs = DEFAULT_BUDGET_MS,
  intervalMs = DEFAULT_INTERVAL_MS,
  capMs = HARNESS_TOOL_TIMEOUT_MS,
) {
  const worst = worstCaseWallClockMs(budgetMs, intervalMs);
  if (worst >= capMs) {
    throw new RangeError(
      `ci-poll would run ${worst} ms, at or past the ${capMs} ms harness cap — ` +
        `it would be auto-backgrounded, which is the exact defect CPE-1880 closed`,
    );
  }
  const bounded = boundedWallClockMs(budgetMs);
  if (bounded >= capMs) {
    throw new RangeError(
      `ci-poll's structural bound is ${bounded} ms, at or past the ${capMs} ms harness cap — ` +
        `a hung gh call would be auto-backgrounded (CPE-1906)`,
    );
  }
  return worst;
}

/**
 * Should the loop sleep for another interval, or has the wall-clock budget run out?
 *
 * Pulled out as a pure function purely so it can be tested — the bug the review found was that the
 * bound existed only as an up-front *model* (`assertNotBackgroundable` with a hard-coded `ghCostMs`),
 * never as a runtime check, so a `gh` call slower than the guess silently pushed the process past the
 * harness cap. This is the runtime check.
 *
 * @param {number} nowMs
 * @param {number} intervalMs
 * @param {number} deadlineMs  `started + budgetMs`
 * @param {number} tick        0-based index of the tick just completed
 * @param {number} ticks       planned tick count
 * @returns {boolean}
 */
export function shouldSleepAgain(nowMs, intervalMs, deadlineMs, tick, ticks) {
  if (tick >= ticks - 1) return false;
  return nowMs + intervalMs < deadlineMs;
}

/**
 * @typedef {object} CiRead One normalised observation of a run or a PR check rollup.
 * @property {boolean} terminal    the provider says the run/rollup has finished
 * @property {string|null} conclusion  success / failure / skipped / …, once terminal
 * @property {number} totalCount   number of checks currently scheduled
 * @property {number} pending      number not yet reported
 * @property {string|null} mergeable  MERGEABLE / CONFLICTING / UNKNOWN, PR polls only
 * @property {string|null} sha      the head SHA the reading is keyed to
 * @property {string[]} skippedNames  checks that reported SKIPPED — they DID NOT RUN
 * @property {string[]} failedNames   checks that reported a hard failure
 * @property {number} ranCount        finished checks that ACTUALLY RAN (finished minus skipped) — the
 *                                    difference between "everything passed" and "nothing happened"
 * @property {number} neutralCount    checks that ran and declined to judge
 * @property {number|null} oldestPendingAgeMs  age of the longest-running unfinished check
 * @property {string|null} oldestPendingName   its name — "slow or hung?" made mechanical
 * @property {string[]} checkNames     EVERY check name on the board, pending ones included — what the
 *                                     CPE-1970 coverage check compares against `main`'s required jobs
 */

/** @returns {CiRead} */
function emptyRead() {
  return {
    terminal: false,
    conclusion: null,
    totalCount: 0,
    pending: 0,
    mergeable: null,
    sha: null,
    checkNames: [],
    skippedNames: [],
    failedNames: [],
    ranCount: 0,
    neutralCount: 0,
    oldestPendingAgeMs: null,
    oldestPendingName: null,
  };
}

/**
 * Decide whether a sequence of reads justifies stopping.
 *
 * Encodes the two rules sprint.md states in prose and that a human poll keeps getting wrong:
 *   1. `total_count == 0` is a state to REPORT, never to treat as passing (a CONFLICTING PR schedules
 *      zero checks, and an empty board looks identical to a green one).
 *   2. `pending == 0` only means done once `total_count` has stopped moving — required stable across at
 *      least two reads, because jobs schedule in waves.
 *
 * @param {CiRead[]} reads chronological, most recent last
 * @returns {{done: boolean, reason: string}}
 */
export function decideFromReads(reads) {
  if (!Array.isArray(reads) || reads.length === 0) {
    return { done: false, reason: "no reads yet" };
  }
  const latest = reads[reads.length - 1];
  if (latest.terminal) {
    return { done: true, reason: `run reported completed (${latest.conclusion ?? "no conclusion"})` };
  }
  if (latest.totalCount === 0) {
    const why =
      latest.mergeable === "CONFLICTING"
        ? "total_count=0 and mergeable=CONFLICTING — GitHub cannot build a merge commit to run checks against"
        : "total_count=0 — no checks scheduled yet; an empty board is NOT a green one";
    return { done: false, reason: why };
  }
  if (latest.pending > 0) {
    return { done: false, reason: `${latest.pending} of ${latest.totalCount} checks still pending` };
  }
  const previous = reads[reads.length - 2];
  if (!previous) {
    return { done: false, reason: `pending=0 but total_count=${latest.totalCount} seen only once — needs a second read` };
  }
  if (previous.totalCount !== latest.totalCount) {
    return {
      done: false,
      reason: `pending=0 but total_count moved ${previous.totalCount}→${latest.totalCount} — jobs still scheduling`,
    };
  }
  // The previous read must ALSO have been at pending=0. Comparing only `totalCount` accepted the
  // sequence [(19,1),(19,0)] — a board that has only just gone quiet, which is precisely the moment the
  // count is about to rise, because `gui-smoke` shards do not exist until their build job finishes.
  // "Stable" has to mean the whole board sat still, not that one number matched twice.
  if (previous.pending !== 0) {
    return {
      done: false,
      reason: `pending just reached 0 (was ${previous.pending}) — total_count=${latest.totalCount} has not yet held quiet for two reads`,
    };
  }
  return {
    done: true,
    reason: `pending=0 with total_count stable at ${latest.totalCount} across two quiet reads`,
  };
}

// ── Skips: which ones somebody ASKED for, and which ones just did not run ─────────────────────────────

/**
 * Read the job graph out of one workflow file's raw text.
 *
 * Deliberately a line scan rather than a YAML parse: `scripts/` has no dependencies and this needs
 * exactly three facts per job — its id, its display `name:`, and whether it carries a job-level `if:` or
 * `needs:`. Anchored on indentation (`jobs:` at column 0, job ids at two spaces, job keys at four),
 * which is how every workflow in this repo is written and what the guard test pins.
 *
 * @param {string} source
 * @returns {Map<string, {name: string|null, conditional: boolean, needs: string[]}>}
 */
export function scanWorkflowJobs(source) {
  /** @type {Map<string, {name: string|null, conditional: boolean, needs: string[]}>} */
  const jobs = new Map();
  const lines = String(source ?? "").split(/\r?\n/);
  let inJobs = false;
  /** @type {string|null} */ let current = null;
  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i];
    if (/^jobs:\s*$/.test(line)) {
      inJobs = true;
      continue;
    }
    if (!inJobs) continue;
    // CPE-1970 round 2: a COMMENT AT COLUMN 0 is not a top-level key, and reading it as one truncated
    // the job list at the comment — `scanWorkflowJobs("jobs:\n  a:\n    name: A\n# c\n  b:\n    name: B\n")`
    // returned `["a"]`. Pre-existing from CPE-1906 and harmless while the only consumer was the skip
    // matcher (a short list over-blocks, which is loud); the coverage check is the first consumer that
    // needs it fail-closed, because a truncated job list silently SHRINKS what `main` requires.
    // Red-proofed: `if (false && …)` here reds exactly `a column-0 comment inside \`jobs:\` no longer
    // truncates the job list` — 1 failed / 74 skipped under `-t "no longer truncates the job list"`,
    // re-measured in round 3 (round 2 wrote `69 skipped`, taken at 70 tests; the file holds 75 now).
    if (/^[\t ]*#/.test(line)) continue;
    if (/^\S/.test(line)) {
      // A new top-level key ends the jobs block.
      inJobs = false;
      current = null;
      continue;
    }
    const jobStart = /^ {2}([A-Za-z0-9_-]+):\s*$/.exec(line);
    if (jobStart) {
      current = jobStart[1];
      jobs.set(current, { name: null, conditional: false, needs: [] });
      continue;
    }
    if (!current) continue;
    const entry = jobs.get(current);
    if (!entry) continue;
    const nameKey = /^ {4}name:\s*(.+?)\s*$/.exec(line);
    if (nameKey) {
      entry.name = nameKey[1].replace(/^["']|["']$/g, "");
      continue;
    }
    if (/^ {4}if:\s*\S/.test(line)) {
      // Covers `if: <expr>` and every folded/literal block header (`if: >-`, `if: |`), whose `>`/`|` is
      // itself the `\S`.
      entry.conditional = true;
      continue;
    }
    if (/^ {4}if:\s*$/.test(line)) {
      // CPE-1906 round 2: the BLOCK-MAPPING form — a bare `if:` with the expression on the following,
      // more-indented line — is legal YAML (a multi-line plain scalar) and used to yield
      // `conditional=false`. That direction fails CLOSED (the job's skips stop being explainable, so the
      // poll over-blocks at exit 4), which is the safe way to be wrong but still wrong: reformatting
      // `gui-smoke.yml`'s `if:` onto two lines would have exited 4 on every PR. A continuation line is
      // indented deeper than the four-space job-key column and is not itself a key at that column.
      let j = i + 1;
      while (j < lines.length && /^\s*$/.test(lines[j])) j += 1;
      if (j < lines.length && /^ {5,}\S/.test(lines[j])) {
        entry.conditional = true;
        i = j;
      }
      continue;
    }
    const needsInline = /^ {4}needs:\s*(.+?)\s*$/.exec(line);
    if (needsInline) {
      const raw = needsInline[1].replace(/^\[|\]$/g, "");
      entry.needs = raw
        .split(",")
        .map((s) => s.trim().replace(/^["']|["']$/g, ""))
        .filter(Boolean);
      continue;
    }
    if (/^ {4}needs:\s*$/.test(line)) {
      for (let j = i + 1; j < lines.length; j += 1) {
        // Same comment hole as the jobs block above: a `#` line between two `- job` items truncated the
        // list, which SHORTENS the transitive `needs:` closure and un-excuses a legitimately skipped job.
        if (/^[\t ]*#/.test(lines[j])) {
          i = j;
          continue;
        }
        const item = /^ {6}-\s*(.+?)\s*$/.exec(lines[j]);
        if (!item) break;
        entry.needs.push(item[1].replace(/^["']|["']$/g, ""));
        i = j;
      }
    }
  }
  return jobs;
}

/**
 * @typedef {object} SkipMatcher
 * @property {string} text   the literal to compare a skipped check's name against
 * @property {boolean} prefix  compare with `startsWith` rather than `===`
 */

/**
 * The set of check-name matchers GitHub is ALLOWED to skip: every job with a job-level `if:`, plus the
 * transitive closure of jobs that `needs:` one of those (a skipped dependency cascades, and nobody
 * should have to re-declare that).
 *
 * CPE-1906 round 2 — EXACT MATCH, NOT PREFIX, UNLESS THE NAME IS TEMPLATED. Every matcher used to be a
 * `startsWith` prefix, which is the fail-OPEN direction, and four of the six matchers this repo derives
 * are bare job ids of `name:`-less jobs (`notify-on-failure`, `verify-published-manifest-sidecar`,
 * `verify-published-manifest`, `catalog`). Measured: the prefix `"catalog"` excused BOTH
 * `"catalog-freshness nightly"` and `"catalogue rebuild"` — checks nothing had declared skippable. No
 * live collision today (all four are release-workflow jobs that never reach a PR rollup), but silently
 * excusing a future `catalog-*` job is the exact defect this file exists to remove. So a matcher is a
 * PREFIX only when it had to be: a matrix job's `name:` contains a `${{ matrix.… }}` expression, and
 * only the literal text before the first `${{` is what GitHub keeps verbatim. Everything else — an
 * explicit `name:` with no template, or a job reporting under its bare id — is compared exactly. Being
 * wrong now over-blocks (exit 4, named and diagnosable) instead of going quiet.
 *
 * @param {string[]} sources raw text of each workflow file
 * @returns {SkipMatcher[]}
 */
export function explainableSkipMatchers(sources) {
  /** @type {SkipMatcher[]} */
  const matchers = [];
  for (const source of sources) {
    const jobs = scanWorkflowJobs(source);
    /** @type {Set<string>} */
    const allowed = new Set();
    for (const [id, job] of jobs) if (job.conditional) allowed.add(id);
    // Transitive closure over `needs:` — iterate to a fixed point; the graph is a DAG and tiny.
    for (let pass = 0; pass < jobs.size + 1; pass += 1) {
      let grew = false;
      for (const [id, job] of jobs) {
        if (allowed.has(id)) continue;
        if (job.needs.some((n) => allowed.has(n))) {
          allowed.add(id);
          grew = true;
        }
      }
      if (!grew) break;
    }
    for (const id of allowed) {
      const job = jobs.get(id);
      const label = job?.name ?? id;
      const templated = label.includes("${{");
      const text = (templated ? label.split("${{")[0] : label).trim();
      if (text) matchers.push({ text, prefix: templated });
    }
  }
  return matchers;
}

/**
 * Split skipped check names into the ones a job-level `if:` explains and the ones nothing does.
 *
 * `matchers === null` means the workflow scan produced nothing — we could not ask, so EVERY skip is
 * unexplained. That is the fail-closed direction on purpose: the alternative is a tool that goes quiet
 * about jobs that did not run the moment it is run from the wrong directory.
 *
 * WHAT "EXPLAINED" ACTUALLY MEANS HERE, precisely, because the first version of this comment claimed
 * more precision than the code has: a skip is explained iff the job carries a job-level `if:` AT ALL,
 * or transitively `needs:` one that does — **whatever that `if:` says**. It is not evaluated. This
 * repo's six job-level conditions include three (`always()`, `!cancelled()`) whose jobs cannot
 * legitimately skip, so those are excused for free. That is a deliberate, bounded over-approximation,
 * not an oversight, and narrowing it is not free: the general rule has to stay "carries a condition"
 * because conditions like `github.event.workflow_run.conclusion != 'success'` skip on every HEALTHY
 * run, and separating "can legitimately be false" from "is always true" means evaluating GitHub
 * expressions against a run context — an evaluator, not a line scan. The residual fail-open is
 * therefore exactly: a job whose `if:` is a tautology is excused if it ever skips.
 *
 * @param {string[]} skippedNames
 * @param {SkipMatcher[]|null} matchers
 * @returns {{explained: string[], unexplained: string[]}}
 */
export function classifySkips(skippedNames, matchers) {
  /** @type {string[]} */ const explained = [];
  /** @type {string[]} */ const unexplained = [];
  for (const name of skippedNames ?? []) {
    const ok =
      Array.isArray(matchers) &&
      matchers.some((m) => {
        const text = String(m?.text ?? "");
        if (text.length === 0) return false;
        return m?.prefix ? String(name).startsWith(text) : String(name) === text;
      });
    (ok ? explained : unexplained).push(name);
  }
  return { explained, unexplained };
}

/**
 * Load every workflow file's text. Returns `[]` rather than throwing when the directory is missing, so
 * the caller can fail closed on an empty scan instead of crashing.
 *
 * @param {string} [dir]
 * @returns {string[]}
 */
export function readWorkflowSources(dir) {
  const root = dir ?? join(fileURLToPath(new URL("..", import.meta.url)), ".github", "workflows");
  try {
    return readdirSync(root)
      .filter((f) => f.endsWith(".yml") || f.endsWith(".yaml"))
      .map((f) => readFileSync(join(root, f), "utf8"));
  } catch {
    return [];
  }
}

// ── Guard coverage: did the checks on this board come from the job set `main` requires? (CPE-1970) ────

/**
 * @typedef {object} OnBlock
 * @property {"pull-request"|"other"|"unknown"} trigger  `unknown` = the `on:` block could not be read
 * @property {boolean} prPathFiltered  EVERY PR-scoped trigger present carries its own
 *           `paths:`/`paths-ignore:` — so GitHub was entitled to skip the workflow on some diffs
 * @property {string[]} events  the event names read out of `on:`, comments already stripped — the
 *           evidence behind `trigger`, so a caller can ask about a name this module does not know.
 *           `[]` when `trigger === "unknown"`. See `readOnBlock`'s return for the flow-form caveat.
 * @property {string} why  one clause, for the operator, when `trigger === "unknown"`
 */

/**
 * The PR-scoped GitHub event names, as a named literal rather than a condition buried in a loop —
 * because this list is a standing blind spot of the classifier and the header points at it. NOT "the
 * one": rounds 3, 4 and 5 each added a shape to that header list, so a count here would have been
 * wrong three times, and a count in a file whose own list says "AT LEAST THESE" reads as a closing
 * inventory of something explicitly left open.
 *
 * BOTH of these run on a pull request and BOTH land their check runs on the PR's rollup, which is the
 * only property `coverageOf` cares about. `pull_request_target` differs from `pull_request` in what it
 * checks out and what secrets it gets, not in whether it judges the PR.
 */
export const PR_EVENTS = ["pull_request", "pull_request_target"];

/**
 * Read one workflow's `on:` block: does it run on a PR event (`PR_EVENTS`), and if so does every such
 * trigger carry a PATH FILTER of its own?
 *
 * Only PR-triggered workflows can contribute checks to a PR's rollup, so a release-only or
 * schedule-only workflow's jobs must never count as "missing" — that would red every PR on day one,
 * which is the outcome that gets a gate switched off. The second fact is what lets `coverageOf` tell a
 * workflow that was ALLOWED to stay silent on this PR from one that simply did not run: see its
 * comment, and §2d of `docs/design/CI-STALENESS.md`.
 *
 * CPE-1970 ROUND 3 — `pull_request_target` WAS A CONFIDENT `false`, AND WITH NO TRACE. The block-key
 * loop compared the key to the string `"pull_request"` exactly, so a `pull_request_target`-only
 * workflow answered `{trigger:"other"}`, `coverageOf` `continue`d past it, and the board printed a bare
 * `coverage=ok` at exit 0 with that workflow's entire guard set absent from the required set — no
 * `unjudged` row, no `silentWorkflows` entry, nothing. Measured end to end on a board carrying all 11
 * real `ci.yml` checks and nothing from a `pull_request_target`-only `security.yml`:
 * `verdict: ok | coverage=ok`, `detail: every job \`main\` requires from ci.yml produced a check here`.
 * `pull_request_target` IS a pull-request trigger — GitHub runs it on PR events and its check runs land
 * on the rollup — so the fix widens the class rather than narrowing the claim: it is now
 * `"pull-request"`, not `"unknown"`. `unknown` would also have been fail-closed, but it is the wrong
 * answer for a well-understood event: the first `pull_request_target` workflow to land would print
 * `coverage=unknown` on every board forever, and a gate that is permanently unknown is a gate people
 * alias away. It takes `paths:`/`paths-ignore:` on exactly the same terms, so the silent-workflow
 * carve-out needed no special case. There is no live instance today
 * (`grep -rn pull_request_target .github/workflows/` → none), so this was latent, exactly as latent as
 * round 1's column-0 comment was — and it is pinned in `ciPollFailClosed.test.ts` so it stays fixed.
 *
 * CPE-1970 ROUND 2 — THIS USED TO FAIL OPEN, SILENTLY, ON LEGAL YAML, AND THE SHAPE WAS ONE REFORMAT
 * AWAY. The predecessor walked the block with `if (/^\S/.test(line)) return false;` as its terminator
 * and had NO comment handling at all, so a comment at COLUMN 0 anywhere inside `on:` ended the scan
 * early and removed the whole workflow from the required set — no `silentWorkflows` line, no
 * `coverage=` change, nothing. Measured on the real `.github/workflows/ci.yml`: it answers `true`, and
 * the same bytes with `# a comment` inserted at column 0 under `on:` answer `false`, after which
 * `coverageOf(["Frontend — type-check and test"], [ci.yml])` returns `ok` on a one-check board with
 * every other `ci.yml` guard absent. `ci.yml`'s `on:` block already carries a ~60-line comment (it is
 * indented today); one re-wrap and every `ci.yml` guard leaves the required set permanently. It also
 * answered `false` for `"pull_request":`, for `"on":`, and for four-space indentation — three more
 * legal spellings, three more whole-workflow blind spots.
 *
 * CLAUDE.md rule 2 ("anchor on code, never on prose; a whole-line-comment filter is not enough") is why
 * comments are stripped in BOTH positions — whole-line at any indent, and trailing after whitespace —
 * on the `on:` line AND on every line of its block body. Round 2 claimed both positions but applied the
 * trailing strip only to the `on:` line's inline rest, so the entirely legal
 * `on:\n  - push\n  - pull_request  # only PRs` answered `unknown` (measured). Fail-closed, but a
 * workflow spelled that way would have printed `coverage=unknown` on every board until someone deleted
 * the comment. And `stripTrailing` was a regex, `/(^|\s)#.*$/`, which cannot see quoting: on
 * `on: ["a #b", pull_request]` it cut at the `#` INSIDE the quoted scalar and ate the `pull_request`
 * after it, answering a confident `false` (measured) — in the paragraph whose job was to argue the
 * tri-state is complete. `splitInlineComment` below tracks quote state instead, so that line now reads
 * as `pull-request` and `on: ["push#1"]` as `other`, both correctly rather than merely closed.
 *
 * WHAT THIS STILL CANNOT SEE — AT LEAST THESE, and the list is open by construction, because every
 * round of review has added one. Say a shape's name here rather than letting the next reader discover
 * it; an enumeration that reads as exhaustive is how a gap reads as coverage.
 *   • THE PR-EVENT LIST IS A LITERAL PAIR (`PR_EVENTS`). A third PR-scoped event name — one GitHub
 *     adds later, or one nobody here has heard of — lands in `other` and drops its whole workflow out
 *     of the required set silently, which is the shape `pull_request_target` had until round 3.
 *     Nothing in `readOnBlock` can notice the next one: it compares against a list, and a name not on
 *     the list is simply not a PR event as far as this code is concerned.
 *     ROUND 4 — AND THE ENUMERATION TEST'S `toEqual` CANNOT NOTICE IT EITHER, which is what round 3
 *     wrote here. `prTriggered` is a FILTER: a workflow classified `other` is REMOVED from the array,
 *     so `toEqual(["ci.yml","gui-smoke.yml"])` still holds and the suite stays green. That `toEqual`
 *     can only red on OVER-inclusion, or on one of those two dropping out — never on a new file the
 *     classifier decided to ignore. Measured over the real `HEAD` workflow set plus one hypothetical
 *     file: `+review-gate.yml (on: pull_request_review:)` → `classify=false`, `toEqual` PASSES;
 *     `+future.yml (on: pull_request_v2:)` → `classify=false`, `toEqual` PASSES. Round 3's other
 *     backstop, a text grep for the `pull_request_target` LITERAL, caught only that one name.
 *     So the guard is now the name-shaped one it always claimed to be: `readOnBlock` returns the
 *     `events` it parsed, and `ciPollFailClosed.test.ts` reds on any parsed `on:` event matching
 *     `/^pull_request[_a-z0-9]*$/` that is NOT in `PR_EVENTS` — so `pull_request_v2` reds on the day
 *     it lands, by shape rather than by anyone having heard of it. What still gets through — AT LEAST
 *     THESE, inheriting the list header above rather than reading as a closed pair, which is exactly
 *     how round 4 wrote it and exactly what round 5 then found a third member of: a PR-scoped event
 *     GitHub names something else entirely (`merge_group` is the live example — it runs on a merge
 *     queue, not a PR, and is deliberately not in `PR_EVENTS`); any workflow whose `on:` block answers
 *     `unknown`, since an unread block yields no `events` to check; and — ROUND 5 — an event on the
 *     CONTINUATION LINE of a multi-line flow collection, which was invisible to `events` because the
 *     scanner only ever read the `on:` line itself. That third one was the FAIL-OPEN kind while the
 *     other two are fail-closed, and the refusal above now moves it into the second category rather
 *     than leaving it as a silent `false`. The lesson is the list's, not the item's: a sub-list under
 *     an "at least these" heading does not inherit the hedge, and this one was read as complete.
 *   • FLOW MAPPING. `on: {pull_request: {paths: ['src/**']}}` is legal and DOES carry a path filter;
 *     the inline branch reports `prPathFiltered: false` for it (measured). That over-blocks — such a
 *     workflow's silence is called unjudged rather than excused — so there is no exposure, but see the
 *     inline branch's own comment: it is false by OMISSION, not by construction.
 *   • YAML ANCHORS AND ALIASES. `on: &trig` / `on: *trig` are not resolved. Round 2 answered a
 *     confident `other` for the first (measured); they now answer `unknown`, which is fail-closed but
 *     still not understanding — a workflow written that way stops the poll rather than being read.
 *     GitHub Actions itself rejects anchors, so this is effectively unreachable.
 *   • BLOCK SCALARS. `on: >` is not understood; `unknown`.
 *   • MULTI-LINE FLOW COLLECTIONS (ROUND 5). `on: [push,\n  pull_request]` is legal, and only the
 *     `on:` LINE is captured, so the continuation is not read. Until round 5 that was a confident
 *     `false` — `trigger=other`, `events=["push"]`, the `pull_request` never seen — and end to end it
 *     reproduced round 3's `pull_request_target` defect exactly, `detail` string included. It now
 *     answers `unknown`, which is fail-closed but again not understanding: the workflow stops the poll
 *     rather than being read. A real flow parser would read it; that is the fix if one ever lands here.
 * Every example named above was RUN, not reasoned about, and the answers are the ones written next to
 * them — round 2's list claimed two shapes landed in `unknown` and one of them landed in `false`.
 *
 * WHY NOT JUST GREP THE SOURCE FOR `pull_request`, WHICH IS WHAT THE BRIEF ASKED FOR — and what the
 * parse gave up to answer it. A raw grep sees the whole file, so it reads a continuation line, and it
 * would have caught round 5's finding on the day round 4 landed. What it cannot do is tell a trigger
 * from a comment: `ci.yml`'s `on:` block carries ~60 lines of commentary, and all five comment
 * positions naming `pull_request_review` inside `on:` (column 0, indented, trailing on a block key,
 * trailing on the `on:` line, trailing after a flow seq) red a grep and are correctly ignored here.
 * NEITHER INSTRUMENT DOMINATES; they have complementary holes, and swapping one for the other traded
 * a false-positive class for a false-negative class rather than strictly improving. Round 4's
 * write-up framed the parse as simply the better choice and did not say what it gave up — which is
 * the shape this whole ticket is about, an instrument narrower than the confidence placed in it. So:
 * WHEN YOU REPLACE ONE MECHANISM WITH ANOTHER, STATE WHAT THE OLD ONE CAUGHT THAT THE NEW ONE DOES
 * NOT. The refusal above is the parse buying that case back by refusing to answer, not by reading it.
 *
 * RED-PROOFED (CLAUDE.md rule 3), result written here rather than only in the PR body: dropping
 * `isComment` from the block-body loop below reds `a column-0 comment inside \`on:\` no longer deletes
 * the workflow from the required set`, `every real workflow in this repo still classifies …` AND (new
 * in round 5) `a multi-line flow \`on:\` was a confident \`false\` …`, whose five comment positions
 * include a column-0 one — **3 failed / 10 passed / 63 skipped** under `-t "fails CLOSED"`.
 * RE-MEASURED IN ROUNDS 3, 4 AND 5: round 2 wrote `1 failed / 5 passed / 64 skipped` here, round 3
 * `2 failed / 10 passed`, and the describe has grown 70 → 75 → 76. A red-proof's counts go stale the
 * moment tests are added beside it, so re-run rather than copy one forward.
 *
 * ROUND 3'S OWN RED-PROOFS, all five run against `-t "fails CLOSED"`, each number measured rather
 * than predicted. RE-RUN IN ROUND 4, and AGAIN IN ROUND 5 — round 5 added one `it` to this describe,
 * so the filter now selects **13 of 76** (63 skipped) and every `passed` below moved by one even
 * where nothing about the sabotage changed. That is the stale-count trap the paragraph above warns
 * about, arriving on schedule for the third round running. THE FAILED SETS ARE THE CLAIM; the passed
 * counts are bookkeeping.
 *
 * AND ONE OF THE FIVE CAME BACK GREEN THE FIRST TIME ROUND 5 RAN IT — 13 passed, 0 failed — which
 * looked like a lost red-proof and was a broken harness: a scripted `replace` of the bare string
 * `prAts.every(filtered)` hits ITS OWN ENTRY IN THIS LIST, four lines below, before it reaches the
 * code. CLAUDE.md rule 2 ("anchor on code, never on prose") applies to the SABOTAGE as much as to the
 * scanner, and the failure is silent in the safe-looking direction: a green run reads as "the test
 * does not cover this" when the truth is "the test was never given anything to notice". Anchored on
 * `prPathFiltered: prAts.every(filtered)` it reds as it always did. If you automate these, assert the
 * patched file DIFFERS FROM THE ORIGINAL IN CODE, not merely that the string was found.
 *   • `PR_EVENTS` back to `["pull_request"]` → **4 failed / 9 passed**: `classifies
 *     \`pull_request_target\` …`, `the PR-event list is a literal pair …`, `\`prPathFiltered\` needs
 *     EVERY PR trigger filtered …`, `a \`#\` inside a quoted scalar …`. Note `every real workflow in
 *     this repo still classifies …` stays GREEN — this repo has no `pull_request_target` workflow, so
 *     the enumeration alone could not have caught it. That is why the four above exist. Re-measured in
 *     round 4 and still green, which is the SAME fact as round 4's finding above: an enumeration built
 *     on a filtered list cannot red on an event the filter decided to ignore. Round 3 wrote that fact
 *     correctly here and its opposite two lines up in the blind-spot bullet.
 *   • `splitInlineComment` back to `s.replace(/(^|\s)#.*$/, "$1")` → **3 failed / 10 passed** (2 in
 *     round 4): `a \`#\` inside a quoted scalar …`, `an unclassifiable \`on:\` is \`null\` …`, and now
 *     `a multi-line flow \`on:\` was a confident \`false\` …`. The THIRD is not a new property of that
 *     regex — this sabotage replaces the whole function, and round 5 moved the bracket-depth count
 *     INTO its loop, so it now disables two mechanisms at once. Said here rather than left to read as
 *     the comment stripper having grown reach it does not have.
 *   • the block-body key read off the raw line instead of `split.rest` → **1 failed / 12 passed**:
 *     `reads the legal spellings that used to answer \`false\` …`.
 *   • `prAts.every(filtered)` back to `filtered(prAts[0])` → **1 failed / 12 passed**:
 *     `\`prPathFiltered\` needs EVERY PR trigger filtered …`. (This is the one whose scripted form
 *     patched a comment; see above.)
 *   • the anchor refusal disabled (`if (false && …)`) → **1 failed / 12 passed**:
 *     `an unclassifiable \`on:\` is \`null\` …`.
 *
 * ROUND 4'S OWN RED-PROOFS, same filter, same 63 skipped — both target the new `events` field, and
 * both land on `every real workflow in this repo still classifies …`, the test round 3 correctly said
 * `PR_EVENTS` alone could not red:
 *   • `events.push(key[2])` suppressed (`if (false) events.push(…)`) → **1 failed / 12 passed**. This
 *     is the one that matters: an `events` that silently came back `[]` would leave the
 *     unknown-PR-event assertion green forever, so the test carries an inline POSITIVE CONTROL over
 *     the real files plus a hypothetical `pull_request_v2` workflow rather than trusting an empty
 *     `toEqual([])`.
 *   • the shape check narrowed back to round 3's literal (`/^pull_request_target$/`) →
 *     **1 failed / 12 passed**, `expected [] to deeply equal [ 'future.yml: pull_request_v2' ]`. The
 *     generalisation is load-bearing, not decoration: revert it and the case the blind-spot bullet is
 *     about goes silent again.
 *
 * ROUND 5'S OWN RED-PROOFS, same filter, same 63 skipped — and this is also the CPE-1929 SABOTAGE PAIR
 * for the new refusal, run rather than reasoned about. Disabling it is NOT green, so it is reachable
 * and not shadowed by the block-scalar or anchor checks in front of it:
 *   • the flow-depth refusal disabled (`if (false && inlineDepth !== 0)`) → **2 failed / 11 passed**:
 *     `a multi-line flow \`on:\` was a confident \`false\` …` and `an unclassifiable \`on:\` is
 *     \`null\` …`.
 *   • the predicate made to lie in the specific way that looks equivalent — depth counted naively over
 *     the RETURNED string (`inlineRest.match(/[[{]/g).length !== inlineRest.match(/[\]}]/g).length`)
 *     instead of in `splitInlineComment`'s quote-aware loop → **1 failed / 12 passed**,
 *     `a multi-line flow \`on:\` …`, on `on: ["a[b", pull_request]`. `rest` still carries its quotes,
 *     so the naive count answers 1 and refuses a line the classifier reads correctly today — a fix
 *     that buys a new false positive. Behaviour changes, so the guard is not shadowed on that leg
 *     either.
 *
 * Same no-dependency line-scan discipline as `scanWorkflowJobs`; `scripts/` has no `node_modules`.
 *
 * @param {string} source
 * @returns {OnBlock}
 */
export function readOnBlock(source) {
  const lines = String(source ?? "").split(/\r?\n/);
  const isBlank = (/** @type {string} */ l) => /^[\t ]*$/.test(l);
  const isComment = (/** @type {string} */ l) => /^[\t ]*#/.test(l);
  const indentOf = (/** @type {string} */ l) => (/^[\t ]*/.exec(l)?.[0] ?? "").replace(/\t/g, "        ").length;
  /** @type {(why: string) => OnBlock} */
  const unknown = (why) => ({ trigger: "unknown", prPathFiltered: false, events: [], why });
  const isPrEvent = (/** @type {string} */ k) => PR_EVENTS.includes(k);
  // `\b` will not do here: `\bpull_request\b` does NOT match inside `pull_request_target`, because `_`
  // is a word character — which is half of why the exact-string comparison went unnoticed for a round.
  const prEventRe = new RegExp(
    `(^|[^A-Za-z0-9_-])(${[...PR_EVENTS].sort((a, b) => b.length - a.length).join("|")})([^A-Za-z0-9_-]|$)`,
  );

  /**
   * Cut a YAML trailing comment off one line: a `#` that starts the line or follows whitespace AND is
   * not inside a quoted scalar. The regex this replaces (`/(^|\s)#.*$/`) could not see the quoting and
   * cut inside `["a #b", pull_request]`, deleting the trigger after it. Single quotes escape by
   * doubling (`''`), which this loop handles as close-then-reopen — the parity is what matters here.
   *
   * ROUND 5 — it also returns `depth`, the net `[`/`{` nesting of `rest`, counted in THIS loop rather
   * than by a second scan of the returned string, because `rest` still carries its quotes: on
   * `["a[b", pull_request]` a naive count answers 1 and would refuse a line the classifier reads
   * correctly today (measured — `pull-request` both before and after this change). Only the flow
   * branch looks at it; a balanced line is `0`, which is every shape that already worked.
   *
   * @param {string} s
   * @returns {{rest: string, unterminated: boolean, depth: number}}
   */
  const splitInlineComment = (s) => {
    /** @type {string} */ let quote = "";
    let depth = 0;
    for (let i = 0; i < s.length; i += 1) {
      const c = s[i];
      if (quote === '"' && c === "\\") {
        i += 1;
        continue;
      }
      if (quote) {
        if (c === quote) quote = "";
        continue;
      }
      if (c === '"' || c === "'") {
        quote = c;
        continue;
      }
      if (c === "[" || c === "{") depth += 1;
      else if (c === "]" || c === "}") depth -= 1;
      else if (c === "#" && (i === 0 || /\s/.test(s[i - 1])))
        return { rest: s.slice(0, i).trim(), unterminated: false, depth };
    }
    return { rest: s.trim(), unterminated: quote !== "", depth };
  };

  let onAt = -1;
  /** @type {string} */ let inlineRest = "";
  let inlineDepth = 0;
  for (let i = 0; i < lines.length; i += 1) {
    if (isComment(lines[i]) || isBlank(lines[i])) continue;
    const m = /^(?:on|"on"|'on')\s*:(.*)$/.exec(lines[i]);
    if (m) {
      const split = splitInlineComment(m[1]);
      if (split.unterminated) return unknown("unterminated quote on the `on:` line");
      onAt = i;
      inlineRest = split.rest;
      inlineDepth = split.depth;
      break;
    }
  }
  if (onAt < 0) return unknown("no top-level `on:` key");

  // Flow / scalar form — `on: [push, pull_request]`, `on: push`.
  //
  // `prPathFiltered: false` here is by OMISSION, not by construction, and round 2's comment said the
  // opposite: `on: {pull_request: {paths: ['src/**']}}` is legal YAML and DOES carry a path filter,
  // and this branch reports `false` for it (measured). The error is in the over-blocking direction —
  // `false` means "not entitled to be silent", so such a workflow's absence is called unjudged rather
  // than excused — so it is left as-is rather than given a flow-mapping parser. It is in the header's
  // "cannot see" list because a safe wrong answer is still a wrong answer.
  if (inlineRest) {
    if (/[>|]\s*[-+]?$/.test(inlineRest)) return unknown("`on:` uses a block scalar");
    // An anchor or alias (`on: &trig`, `on: *trig`) is not resolved. GitHub Actions rejects anchors, so
    // this is unreachable in practice; round 2 nevertheless answered a confident `other` for it, which
    // silently dropped the whole workflow. Fail closed instead.
    if (/^[&*]/.test(inlineRest)) return unknown("`on:` uses a YAML anchor or alias, which is not resolved");
    // ROUND 5 BLOCKER — A FLOW COLLECTION MAY SPAN LINES, AND THIS BRANCH ONLY EVER SEES THE FIRST ONE.
    // The `on:` scanner above captures the remainder of the `on:` LINE, so on the entirely legal
    //     on: [push,
    //       pull_request]
    // `inlineRest` was `[push,` — and BOTH `trigger` and `events` were computed from that alone.
    // Measured before this refusal: `trigger=other`, `triggersPR=false`, `events=["push"]`, i.e. a
    // confident `false` with the `pull_request` never seen. End to end with such a `security.yml`:
    // `{"state":"ok", "unjudged":[], "judgedWorkflows":["ci.yml"], "silentWorkflows":[], "detail":
    // "every job `main` requires from ci.yml produced a check here"}` — round 3's `pull_request_target`
    // defect character for character, detail string included, with that workflow's whole guard set
    // gone. `pull_request_v2` on the continuation line was equally invisible, so the `events` guard
    // added in round 4 could not have caught it either.
    // Refusing an unbalanced `inlineRest` turns the confident `false` into `unknown`, which
    // `coverageOf` blocks on by name. All 8 real workflows here take the BLOCK branch and are
    // untouched; every balanced one-line flow is `depth === 0` and reads exactly as before.
    if (inlineDepth !== 0) return unknown("`on:` uses a flow collection that does not close on its own line");
    // `events` here is a TOKEN SWEEP, not a parse: `on: ['a #b', pull_request]` yields `a`, `b`,
    // `pull_request`, because this branch never separates a flow scalar from a key — and
    // `on: {push: {paths: ['pull_request_v2/**']}}` yields `paths` and `pull_request_v2`, a token
    // swept out of a path glob rather than any event. Stated rather than hidden, and it is the safe
    // direction for the only consumer — the unknown-PR-event guard in `ciPollFailClosed.test.ts` asks
    // whether any name here looks PR-scoped and is not in `PR_EVENTS`, so an extra token reds a
    // workflow that is fine rather than passing one that is not.
    //
    // THAT IS THE OVER-REPORTING DIRECTION ONLY, AND THE SWEEP IS TWO-SIDED. Until the refusal above
    // it also UNDER-reported, and under-reporting is the fail-open kind: the continuation line's
    // `pull_request` simply was not in the string being swept. The refusal removes that input from
    // this branch rather than making the sweep see it, so the one-sided sentence is now true of what
    // actually reaches here — not of flow collections in general.
    return {
      trigger: prEventRe.test(inlineRest) ? "pull-request" : "other",
      prPathFiltered: false,
      events: inlineRest.match(/[A-Za-z_][A-Za-z0-9_-]*/g) ?? [],
      why: "",
    };
  }

  // Block form. Children are every line indented deeper than `on:` itself, comments and blanks skipped.
  /** @type {number[]} */ const body = [];
  for (let i = onAt + 1; i < lines.length; i += 1) {
    if (isBlank(lines[i]) || isComment(lines[i])) continue;
    if (indentOf(lines[i]) === 0) break;
    body.push(i);
  }
  if (body.length === 0) return unknown("`on:` has no block content");
  const childIndent = indentOf(lines[body[0]]);

  // Every PR-scoped trigger, not just the first. `prPathFiltered` is an EXCUSE for silence, so it may
  // only be true when EVERY one of them is filtered: a workflow with a path-filtered `pull_request:`
  // and an unfiltered `pull_request_target:` still runs on every diff, and stopping at the first key
  // would have excused its silence.
  /** @type {number[]} */ const prAts = [];
  /** @type {string[]} */ const events = [];
  for (const i of body) {
    if (indentOf(lines[i]) !== childIndent) continue;
    const split = splitInlineComment(lines[i]);
    if (split.unterminated) return unknown(`unterminated quote in the \`on:\` block: ${lines[i].trim()}`);
    const key = /^(?:-[\t ]*)?(["']?)([A-Za-z_][A-Za-z0-9_-]*)\1[\t ]*(:.*)?$/.exec(split.rest);
    if (!key) return unknown(`unrecognised line in the \`on:\` block: ${lines[i].trim()}`);
    events.push(key[2]);
    if (isPrEvent(key[2])) prAts.push(i);
  }
  if (prAts.length === 0) return { trigger: "other", prPathFiltered: false, events, why: "" };

  const filtered = (/** @type {number} */ prAt) => {
    for (const i of body) {
      if (i <= prAt) continue;
      if (indentOf(lines[i]) <= childIndent) break;
      if (/^[\t ]*(["']?)paths(-ignore)?\1[\t ]*:/.test(lines[i])) return true;
    }
    return false;
  };
  return { trigger: "pull-request", prPathFiltered: prAts.every(filtered), events, why: "" };
}

/**
 * Does this workflow run on `pull_request` at all? `null` = the `on:` block could not be classified,
 * which the caller must fail closed on rather than read as "no".
 *
 * @param {string} source
 * @returns {boolean|null}
 */
export function workflowTriggersPullRequest(source) {
  const on = readOnBlock(source);
  if (on.trigger === "unknown") return null;
  return on.trigger === "pull-request";
}

/**
 * Read `.github/workflows/*.yml` out of a GIT REVISION rather than off disk — by default
 * `origin/main`, the branch the PR is going to be merged into.
 *
 * WHY NOT THE WORKING TREE, which `readWorkflowSources()` above already gives us for free. Because the
 * working tree is exactly the stale copy this whole ticket is about. A Worker polls its own PR from its
 * own worktree, and that worktree IS the PR branch: on PR #1056 it did not contain `ratchet-guard` at
 * all, so a coverage check reading it would have computed a required-job set with no `ratchet-guard` in
 * it, found nothing missing, and printed green — reproducing the defect from inside the guard built to
 * catch it. The question is "what does **main** require of this PR", and only main can answer it.
 *
 * NO FETCH. A poll must not have side effects on the repo, and a `git fetch` inside a bounded-wall-clock
 * tool is one more thing that can hang. The consequence is stated rather than hidden: a locally STALE
 * `origin/main` under-reports (it cannot see a guard landed since the last fetch), so the verdict line
 * prints the ref and its short SHA and the runbook says to `git fetch origin main` before the last poll.
 * Being stale here fails OPEN, which is why it is printed on every single verdict rather than mentioned
 * in a comment.
 *
 * `CI_POLL_BASE_WORKFLOWS` is a TEST SEAM and nothing else, exactly like `CI_POLL_GH_SCRIPT`: it names a
 * directory of workflow files to read instead of asking git, so the subprocess tests can drive a base
 * that is deliberately ahead of the stubbed rollup (the #1056 shape) without depending on this repo's
 * live history.
 *
 * @param {string} [ref] git revision to read the workflows out of
 * @returns {{ref: string, sha: string|null, files: {file: string, text: string}[]}|null} null = could
 *          not read; the caller must fail closed on it, never treat it as "nothing to check"
 */
export function readBaseWorkflowSources(ref = "origin/main") {
  const seam = process.env.CI_POLL_BASE_WORKFLOWS;
  if (seam) {
    try {
      const files = readdirSync(seam)
        .filter((f) => f.endsWith(".yml") || f.endsWith(".yaml"))
        .map((f) => ({ file: f, text: readFileSync(join(seam, f), "utf8") }));
      return { ref: `seam:${seam}`, sha: null, files };
    } catch {
      return null;
    }
  }
  // Resolve the repo out of this file's own location so the answer does not depend on where the caller
  // stood — with a `process.cwd()` fallback because a bundler (Vitest/Vite transforms this module when a
  // unit test imports it) can hand back an `import.meta.url` that is not a `file:` URL, and
  // `fileURLToPath` throws on those. Measured: without the fallback every in-process call returned null,
  // i.e. "could not read `main`" — the fail-closed direction, but wrong, and it would have made the
  // derivation leg of this feature's own test suite unrunnable.
  const cwd = (() => {
    try {
      return fileURLToPath(new URL("..", import.meta.url));
    } catch {
      return process.cwd();
    }
  })();
  const git = (/** @type {string[]} */ args) =>
    execFileSync("git", args, {
      cwd,
      encoding: "utf8",
      maxBuffer: 32 * 1024 * 1024,
      timeout: GH_MIN_CALL_TIMEOUT_MS,
      stdio: ["ignore", "pipe", "pipe"],
    });
  /** @type {string} */ let listing;
  /** @type {string|null} */ let sha = null;
  try {
    listing = git(["ls-tree", "--name-only", `${ref}:.github/workflows`]);
  } catch {
    return null;
  }
  try {
    sha = git(["rev-parse", "--short", ref]).trim() || null;
  } catch {
    sha = null;
  }
  /** @type {{file: string, text: string}[]} */ const files = [];
  for (const file of listing.split(/\r?\n/).filter((f) => /\.ya?ml$/.test(f))) {
    try {
      files.push({ file, text: git(["show", `${ref}:.github/workflows/${file}`]) });
    } catch {
      // One unreadable file out of many is still a partial answer; an EMPTY answer is what fails closed.
    }
  }
  return { ref, sha, files };
}

/**
 * The check-name matcher for one workflow job, on the same rule `explainableSkipMatchers` settled: a
 * PREFIX only when the `name:` is templated by a matrix expression (GitHub keeps the literal text before
 * the first `${{` and appends the matrix values), otherwise an EXACT match.
 *
 * @param {string} label
 * @returns {{text: string, prefix: boolean}}
 */
function jobNameMatcher(label) {
  const templated = String(label).includes("${{");
  return { text: (templated ? String(label).split("${{")[0] : String(label)).trim(), prefix: templated };
}

/**
 * @typedef {object} Coverage
 * @property {"ok"|"unjudged"|"unknown"|"n/a"} state
 * @property {{file: string, label: string}[]} unjudged  jobs `main` requires that produced NO check here
 * @property {string[]} judgedWorkflows  PR-triggered workflow files that contributed at least one check
 * @property {string[]} silentWorkflows  PR-triggered workflow files that contributed none AND whose own
 *                                       `pull_request:` trigger carries a path filter, so they were
 *                                       ALLOWED to stay silent — named on stdout, not flagged
 * @property {string} detail  one human sentence, always printed
 */

/**
 * ANSWER THE QUESTION THE MERGE PROCEDURE ACTUALLY ASKS: is there a job `main` requires that never
 * appeared on this PR's board at all?
 *
 * THE PRECISION CHOICE, AND WHY IT IS THIS ONE (CPE-1970). Three candidate rules were on the table.
 *
 *   (a) "`main` moved at all since the PR's checks ran." Cheap, and useless here. Measured on this
 *       repo (`git rev-list --count origin/main --since=2026-08-14T00:00:00Z
 *       --until=2026-08-28T11:05:17Z`): 589 commits landed on `main` in the window, ~41/day. A PR that
 *       sits in the queue for an hour almost always sees `main` move, so this rule fires on nearly
 *       every merge and trains the crew to wave it through — a worse outcome than the bug.
 *   (b) "The PR's newest check run FINISHED before a guard landed." This is the rule the ticket's own
 *       evidence rules out. PR #1056's run finished at 18:35:13Z and it merged at 18:36:20Z — one
 *       minute later. A recency check on finish time would have passed it. Worse, it is an inference:
 *       it says the guard *probably* did not judge the PR.
 *   (c) "A job `main` requires produced no check on this board." Definite, not inferential — the guard
 *       is not on the board, so it cannot have judged anything. It needs no clock reasoning, and it is
 *       the instrument the ticket independently arrived at. This is the one implemented here.
 *
 * WHAT IT MISSES, said plainly because a guard whose blind spot is undocumented reads as coverage.
 *   · **A guard added INSIDE an existing job.** A new `src/lib/anything.test.ts` runs under the same
 *     `Frontend — type-check and test` check; a new ratchet registered in `ratchet-baselines.mjs` runs
 *     under the same `Ratchet guard` check; CPE-1936's `shellScriptLines` parser fix changed what every
 *     workflow scan can see without touching a job name. The check IS on the board, so this verdict is
 *     silent. That is the larger class by count and NO name-based instrument can see it — only
 *     re-running the PR's checks against `main`'s head can, which is what branch protection's "require
 *     branches to be up to date" buys and why `docs/design/CI-STALENESS.md` still asks for it.
 *   · **A locally stale `origin/main`.** See `readBaseWorkflowSources` — under-reports, so the ref and
 *     its SHA are printed on every verdict.
 *   · **A workflow that contributed NOTHING** is excused ONLY when its own `pull_request:` trigger
 *     carries a `paths:`/`paths-ignore:` filter, i.e. when GitHub was entitled not to run it on this
 *     diff. ROUND 2 NARROWED THIS FROM A BLANKET CARVE-OUT, and the blanket version was justified by a
 *     claim that is FALSE OF THIS REPO: `ci.yml`'s `paths-ignore` sits on its **`push:`** trigger only,
 *     and neither PR-triggered workflow here (`ci.yml`, `gui-smoke.yml`) has any path filter on
 *     `pull_request:` — `ci.yml` says so in its own words at `ci.yml:62-64`. So the blanket version
 *     bought nothing and cost a WHOLE-GUARD-SET blind spot: a board with zero `ci.yml` checks returned
 *     `ok`, exit 0, with every `ci.yml` guard absent (reproduced against the real `origin/main`).
 *     Measured before narrowing it, over all 186 merges in the window via each PR's rollup grouped by
 *     `checkSuite.workflowRun.workflow.name`: **0** boards were missing `CI` and **0** were missing
 *     `GUI smoke`. The new rule therefore adds zero firings to the sweep below and closes the hole. The
 *     excused (path-filtered) case is still real in general and still named on stdout, and the
 *     `coverage=` token now says `ok(N-silent)` rather than a bare `ok`.
 *   · **A job `main` deleted or renamed** looks the same as a job that did not run. Fail-closed
 *     direction: it blocks, names itself, and is one line to read.
 *
 * THE NOISE, MEASURED RATHER THAN HOPED FOR — AND ROUND 1 MEASURED IT WITH THE WRONG INSTRUMENT.
 * Swept over the 184 merged PRs (2026-08-14 → 2026-08-28) this rule could be evaluated on: **168 clean,
 * 16 firings**. Round 1 called all 16 genuinely-new guards and claimed "zero deletion/rename noise" —
 * but it tested whether the absent job still existed **by id**, while the matcher keys on the **label**.
 * Wrong field, wrong conclusion. Re-classified by asking whether the absent job's id existed in that
 * PR's OWN head tree (`gh api .../contents/.github/workflows/<file>?ref=<head>`): **15 genuinely new
 * guards, 1 renamed by the PR being judged** — #921 (CPE-1753) kept the job id `gui-smoke-linux` and
 * changed its `name:` from `GUI smoke (ubuntu-latest) — tauri-driver + WebdriverIO (CPE-1171)` to the
 * templated `GUI smoke (ubuntu-latest) shard ${{ matrix.shard }} — …`; its board carried all four
 * shards, the build job and the verdict, green. Nothing went unjudged there. True rename rate: **1 of
 * 16 firings (6.3%), 1 of 184 merges (0.54%)** — the gate is right to block it (it cannot tell the two
 * apart, and says so), but it is noise, not exposure. The argument for rule (c) over rule (a) is
 * unchanged: (c) reds 9% of merges, (a) would have red essentially all 184. A gate that reds 9% is one
 * people read; a gate that reds 95% is one they alias away.
 *
 * @param {string[]} checkNames every check name on the PR's rollup, verbatim
 * @param {{file: string, text: string}[]|null} baseFiles workflow sources from `main`; null = could not
 *        read, which is "did not run", not "found nothing"
 * @returns {Coverage}
 */
export function coverageOf(checkNames, baseFiles) {
  if (!Array.isArray(baseFiles) || baseFiles.length === 0) {
    return {
      state: "unknown",
      unjudged: [],
      judgedWorkflows: [],
      silentWorkflows: [],
      detail:
        "could not read `main`'s workflow files, so the set of jobs that must judge this PR is unknown — " +
        "this is 'did not run', not 'nothing to check'. Run the poll from inside the repo with an " +
        "`origin/main` ref present (`git fetch origin main`).",
    };
  }
  const names = (checkNames ?? []).map((n) => String(n));
  /** @type {{file: string, label: string}[]} */ const unjudged = [];
  /** @type {string[]} */ const judgedWorkflows = [];
  /** @type {string[]} */ const silentWorkflows = [];
  for (const { file, text } of baseFiles) {
    const on = readOnBlock(text);
    if (on.trigger === "unknown") {
      // FAIL CLOSED, not quiet. A workflow whose `on:` we cannot classify might be the one carrying the
      // guard, and returning "not PR-triggered" would drop its whole job set out of the required set
      // with no trace — the exact fail-open round 2 found in `workflowTriggersPullRequest`.
      return {
        state: "unknown",
        unjudged: [],
        judgedWorkflows: [],
        silentWorkflows: [],
        detail:
          `could not classify the \`on:\` block of ${file} (${on.why}), so whether its jobs must judge ` +
          `this PR is unknown — 'did not run', not 'nothing to check'. Read that workflow's \`on:\` key.`,
      };
    }
    if (on.trigger !== "pull-request") continue;
    const jobs = scanWorkflowJobs(text);
    if (jobs.size === 0) {
      // Same reason: a PR-triggered workflow with no readable `jobs:` block is a required set we could
      // not compute, and an empty required set is trivially satisfied by any board.
      return {
        state: "unknown",
        unjudged: [],
        judgedWorkflows: [],
        silentWorkflows: [],
        detail:
          `${file} triggers on \`pull_request\` but no jobs could be read out of it, so the set of ` +
          `guards it must contribute is unknown — 'did not run', not 'nothing to check'.`,
      };
    }
    /** @type {{file: string, label: string}[]} */ const absent = [];
    let present = 0;
    for (const [id, job] of jobs) {
      const m = jobNameMatcher(job.name ?? id);
      if (!m.text) continue;
      const hit = names.some((n) => (m.prefix ? n.startsWith(m.text) : n === m.text));
      if (hit) present += 1;
      else absent.push({ file, label: job.name ?? id });
    }
    // A workflow that contributed NO check at all is excused ONLY if EVERY one of its PR-scoped
    // triggers (`PR_EVENTS`) carries a path filter — then GitHub was entitled not to run it on this
    // diff. Without one there is no legitimate reason for its absence, so its jobs are exactly as
    // unjudged as any other missing guard. See this function's header for why the blanket carve-out was
    // wrong and what it measured.
    // Red-proofed: restoring the blanket `if (present === 0)` reds exactly `a PR-triggered workflow
    // with NO path filter that contributed nothing is UNJUDGED, not excused` — 1 failed / 4 passed /
    // 70 skipped under `-t "narrow ON PURPOSE"`, re-measured in round 3 (round 2's `69 skipped` was
    // taken when the file held 70 tests; it holds 75 now).
    if (present === 0 && on.prPathFiltered) silentWorkflows.push(file);
    else {
      if (present > 0) judgedWorkflows.push(file);
      unjudged.push(...absent);
    }
  }
  if (unjudged.length > 0) {
    return {
      state: "unjudged",
      unjudged,
      judgedWorkflows,
      silentWorkflows,
      detail:
        `${unjudged.length} job(s) that \`main\` requires produced NO check on this board: ` +
        `${unjudged.map((u) => `${u.label} (${u.file})`).join(", ")}. Those guards did not judge this ` +
        `PR — its checks predate them. Rebase onto \`main\` and let CI re-run before merging.`,
    };
  }
  return {
    state: "ok",
    unjudged,
    judgedWorkflows,
    silentWorkflows,
    detail:
      `every job \`main\` requires from ${judgedWorkflows.join(", ") || "(no workflow)"} produced a check here` +
      (silentWorkflows.length > 0
        ? `; ${silentWorkflows.join(", ")} contributed none and is path-filtered on every PR trigger it declares, so it was allowed to`
        : ""),
  };
}

/**
 * The one token the `coverage=` field on the totals line carries. Always printed — including
 * `n/a`, because "the coverage check did not run" must never look the same as "it ran and found
 * nothing", which is the house rule this whole file is built around.
 *
 * `ok(N-silent)` is the same rule one notch finer: a clean board where N PR-triggered workflows
 * legitimately contributed nothing is NOT the same fact as a clean board where every workflow reported,
 * and a bare `ok` for both is the shape that let a whole missing workflow read as coverage.
 *
 * @param {Coverage|null|undefined} coverage
 * @returns {string}
 */
export function formatCoverage(coverage) {
  const c = coverage ?? { state: "unknown", unjudged: [], silentWorkflows: [], detail: "" };
  if (c.state === "unjudged") return `coverage=${c.unjudged.length}-unjudged`;
  if (c.state === "unknown") return "coverage=unknown";
  if (c.state === "n/a") return `coverage=n/a${c.detail ? `(${c.detail})` : ""}`;
  const silent = c.silentWorkflows?.length ?? 0;
  return silent > 0 ? `coverage=ok(${silent}-silent)` : "coverage=ok";
}

// ── Reads ────────────────────────────────────────────────────────────────────────────────────────────

/**
 * The error `assertReadableShape` throws. A distinct name (rather than a bare `Error`) is what lets
 * `classifyGhFailure` report "unexpected payload shape" instead of "gh exited non-zero" — the caller's
 * next move for a wrong-shaped 200 is different from the next move for a dead `gh`.
 */
export class GhPayloadShapeError extends Error {
  /** @param {string} message */
  constructor(message) {
    super(message);
    this.name = "GhPayloadShapeError";
  }
}

/**
 * Refuse a `gh` payload that did not answer the question we asked (CPE-1906 round 2).
 *
 * THE HOUSE RULE, ONE LAYER DOWN. "A wrapper around an external tool must distinguish 'ran and found
 * nothing' from 'did not run', and fail closed on the latter." Round 1 applied it to a `gh` that THREW.
 * A `gh` that exits 0 with well-formed JSON of the wrong shape throws nothing, and the readers below are
 * written defensively (`Array.isArray(json?.x) ? … : []`) precisely so a formatter can never crash — so
 * an API error payload became `total_count=0`, which `decideFromReads` calls "no checks scheduled yet".
 * Measured, all exit 0 out of `gh`: `{"message":"Not Found","documentation_url":…}` (REST), `{"data":
 * null,"errors":[…]}` (GraphQL answers a field-level failure with HTTP 200 and a partial `data`, and
 * `statusCheckRollup` is a NULLABLE field), `null`, `"nope"`, `[1,2,3]` — every one of them printed
 * `CI VERDICT: pending — total_count=0 … CI still pending on unknown` and exited 2. And in `--run` mode
 * a payload with no `jobs` key exited **0, green**.
 *
 * The test is structural rather than a heuristic, which is what makes it safe against a genuinely
 * check-less PR: we ASK `gh` for these fields, so a response that answered has them. An empty board
 * still returns `statusCheckRollup: []` — an ARRAY — and a real `headRefOid`; the wrong-shape payloads
 * have no such key at all, which is why they print `sha=unknown mergeable=n/a` alongside
 * `total_count=0`, a combination no real board produces.
 *
 * @param {unknown} json
 * @param {"run"|"pr"} mode
 * @returns {any} the same payload, so callers can chain
 */
export function assertReadableShape(json, mode) {
  const shown = (() => {
    try {
      return JSON.stringify(json)?.slice(0, 200) ?? String(json);
    } catch {
      return String(json);
    }
  })();
  if (json === null || typeof json !== "object" || Array.isArray(json)) {
    throw new GhPayloadShapeError(
      `gh returned JSON that is not an object (${typeof json}) — not a board at all: ${shown}`,
    );
  }
  const obj = /** @type {any} */ (json);
  if (mode === "pr") {
    if (!Array.isArray(obj.statusCheckRollup)) {
      throw new GhPayloadShapeError(
        "gh returned JSON with no `statusCheckRollup` array — a real PR rollup always has one (an " +
          `empty board is []), so this is an error payload, not a board: ${shown}`,
      );
    }
    return obj;
  }
  if (!Array.isArray(obj.jobs) || typeof obj.status !== "string") {
    throw new GhPayloadShapeError(
      "gh returned JSON with no `jobs` array and/or no `status` string — both were requested, so a " +
        `response that answered has them; this one did not: ${shown}`,
    );
  }
  return obj;
}

/**
 * @param {unknown} v
 * @param {number} nowMs
 * @returns {number|null}
 */
function ageMs(v, nowMs) {
  if (typeof v !== "string" || v.length === 0) return null;
  const t = Date.parse(v);
  if (!Number.isFinite(t)) return null;
  return Math.max(0, nowMs - t);
}

/**
 * The longest-running unfinished check, by start time.
 *
 * CPE-1906 — WHY THIS IS REPORTED AT ALL. On 2026-08-27 a shift spent over an hour on a single
 * `Server crates (windows-latest)` job with two approved PRs blocked behind it, and the only way to tell
 * "slow" from "hung" was to hand-compare start timestamps against a sibling PR's identical job. The
 * number is already in the payload the poll fetches; printing it makes that judgement mechanical.
 * What is deliberately NOT done is thresholding it — "over N minutes means hung" would be a number
 * invented rather than measured, and this repo's own median run is 58.9 min, so any plausible threshold
 * fires constantly. Report the age and the name; the caller compares against a sibling and decides.
 *
 * @param {any[]} items
 * @param {(item: any) => unknown} startedAt
 * @param {number} nowMs
 * @returns {{ageMs: number|null, name: string|null}}
 */
function oldestOf(items, startedAt, nowMs) {
  /** @type {{ageMs: number|null, name: string|null}} */
  let best = { ageMs: null, name: null };
  for (const item of items) {
    const age = ageMs(startedAt(item), nowMs);
    if (age === null) continue;
    if (best.ageMs === null || age > best.ageMs) {
      best = { ageMs: age, name: String(item?.name ?? item?.context ?? "(unnamed check)") };
    }
  }
  return best;
}

/**
 * Normalise `gh run view --json` output into a CiRead.
 *
 * The run-level `conclusion` is NOT trusted on its own: GitHub reports a run as `success` when jobs were
 * skipped by a `needs:` cascade, which is CPE-1906's whole point. Skipped jobs are counted separately
 * and the caller decides.
 *
 * @param {any} json
 * @param {number} [nowMs]
 * @returns {CiRead}
 */
export function readFromRunJson(json, nowMs = Date.now()) {
  const jobs = Array.isArray(json?.jobs) ? json.jobs : [];
  const pendingJobs = jobs.filter((/** @type {any} */ j) => j?.status !== "completed");
  const conclusionOf = (/** @type {any} */ j) => String(j?.conclusion ?? "").toLowerCase();
  const skippedNames = jobs
    .filter((/** @type {any} */ j) => conclusionOf(j) === "skipped")
    .map((/** @type {any} */ j) => String(j?.name ?? "(unnamed job)"));
  const failedNames = jobs
    .filter((/** @type {any} */ j) => {
      const c = conclusionOf(j);
      return c !== "" && c !== "success" && c !== "neutral" && c !== "skipped";
    })
    .map((/** @type {any} */ j) => String(j?.name ?? "(unnamed job)"));
  const oldest = oldestOf(pendingJobs, (/** @type {any} */ j) => j?.startedAt ?? j?.createdAt, nowMs);
  const runConclusion = json?.conclusion ?? null;
  return {
    ...emptyRead(),
    terminal: json?.status === "completed",
    // Downgrade a run GitHub called `success` when a job never ran; promote nothing.
    conclusion:
      failedNames.length > 0
        ? "failure"
        : String(runConclusion ?? "").toLowerCase() === "success" && skippedNames.length > 0
          ? "skipped"
          : runConclusion,
    totalCount: jobs.length,
    pending: pendingJobs.length,
    sha: json?.headSha ?? null,
    checkNames: jobs.map((/** @type {any} */ j) => String(j?.name ?? "(unnamed job)")),
    skippedNames,
    failedNames,
    ranCount: jobs.filter(
      (/** @type {any} */ j) => j?.status === "completed" && conclusionOf(j) !== "skipped",
    ).length,
    neutralCount: jobs.filter((/** @type {any} */ j) => conclusionOf(j) === "neutral").length,
    oldestPendingAgeMs: oldest.ageMs,
    oldestPendingName: oldest.name,
  };
}

/**
 * Normalise `gh pr view --json statusCheckRollup,…` output into a CiRead.
 *
 * CPE-1906 — the success test, token by token. The old line was
 *   `conclusion === "SUCCESS" || conclusion === "NEUTRAL" || conclusion === "SKIPPED" || state === "SUCCESS"`
 * and each `||` had a different justification, only three of which survive:
 *   · `SUCCESS`  — a CheckRun that ran and passed. Kept.
 *   · `NEUTRAL`  — a CheckRun that ran and declined to judge. GitHub's own required-check semantics
 *                  treat NEUTRAL as non-blocking, and it DID run, so it is not a "did not run". Kept —
 *                  but counted and printed, because silently equating it with a pass is how the SKIPPED
 *                  hole got in.
 *   · `SKIPPED`  — the job DID NOT RUN. Removed. This is the fail-open; skipped checks are now collected
 *                  by name and adjudicated against the workflows' own `if:` clauses.
 *   · `state === "SUCCESS"` — the StatusContext arm. StatusContext (a legacy commit status: an external
 *                  CI, a deploy bot) has `state`, not `conclusion`, so this is the only way such an
 *                  entry can pass. It is inert for CheckRun entries, which have no `state` field — but
 *                  relying on "inert because the field happens to be absent" is exactly the kind of
 *                  accident that stops being true after a `gh` upgrade, so it is now explicitly gated on
 *                  the entry having no `conclusion` at all. Same behaviour on today's payloads, no
 *                  reliance on absence.
 * Anything else — CANCELLED, TIMED_OUT, ACTION_REQUIRED, STALE, a StatusContext `ERROR`/`FAILURE`, or a
 * shape this code has never seen — falls through to failure. Fail closed.
 *
 * @param {any} json
 * @param {number} [nowMs]
 * @returns {CiRead}
 */
export function readFromPrJson(json, nowMs = Date.now()) {
  const rollup = Array.isArray(json?.statusCheckRollup) ? json.statusCheckRollup : [];
  const nameOf = (/** @type {any} */ c) => String(c?.name ?? c?.context ?? "(unnamed check)");
  const isPending = (/** @type {any} */ c) => {
    if (c?.__typename === "StatusContext") return c?.state === "PENDING" || c?.state === "EXPECTED";
    return c?.status !== "COMPLETED";
  };
  const isSkipped = (/** @type {any} */ c) => c?.conclusion === "SKIPPED";
  const isNeutral = (/** @type {any} */ c) => c?.conclusion === "NEUTRAL";
  const passed = (/** @type {any} */ c) =>
    c?.conclusion === "SUCCESS" || isNeutral(c) || (c?.conclusion == null && c?.state === "SUCCESS");

  const pendingChecks = rollup.filter(isPending);
  const finished = rollup.filter((/** @type {any} */ c) => !isPending(c));
  const skippedNames = finished.filter(isSkipped).map(nameOf);
  const failedNames = finished.filter((/** @type {any} */ c) => !isSkipped(c) && !passed(c)).map(nameOf);
  const ran = finished.filter((/** @type {any} */ c) => !isSkipped(c));
  const oldest = oldestOf(pendingChecks, (/** @type {any} */ c) => c?.startedAt ?? c?.createdAt, nowMs);

  /** @type {string|null} */
  let conclusion = null;
  if (rollup.length > 0) {
    if (failedNames.length > 0) conclusion = "failure";
    else if (ran.length === 0) conclusion = "skipped";
    else conclusion = "success";
  }

  return {
    ...emptyRead(),
    // NEVER terminal, even at pending == 0. A workflow RUN has an authoritative `status: completed`;
    // a PR check rollup has no such signal — it is only ever "everything scheduled so far has
    // reported," and `gui-smoke` shards do not exist until their build job finishes. Marking this
    // terminal would short-circuit the two-read stability rule and reintroduce exactly the CPE-1863
    // dip-before-it-rises misread. Let decideFromReads() require the second read.
    terminal: false,
    conclusion,
    totalCount: rollup.length,
    pending: pendingChecks.length,
    mergeable: json?.mergeable ?? null,
    sha: json?.headRefOid ?? null,
    checkNames: rollup.map(nameOf),
    skippedNames,
    failedNames,
    ranCount: ran.length,
    neutralCount: finished.filter(isNeutral).length,
    oldestPendingAgeMs: oldest.ageMs,
    oldestPendingName: oldest.name,
  };
}

// ── Verdict lines ────────────────────────────────────────────────────────────────────────────────────

/**
 * @param {CiRead|null} latest
 * @param {number} [ghFailures] total failed `gh` reads this invocation — appended, never inserted
 * @param {Coverage|null} [coverage] CPE-1970 guard-coverage state — appended after `gh_failures`
 */
function totalsOf(latest, ghFailures = 0, coverage = null) {
  // CPE-1906 round 2: `gh_failures` is APPENDED at the end, after `sha=`. The interface pin asserts the
  // presence and RELATIVE ORDER of the pre-existing keys, so a new key may only ever go on the end.
  // Why it exists: a poll could take one good read, then fail two more (below the bail threshold), hit
  // the deadline and print a plain `pending` verdict that said nothing at all about the failures —
  // "still pending" and "still pending, and I stopped being able to ask" are different situations.
  //
  // CPE-1970 appends `coverage=` after it under the same rule. It is printed on EVERY verdict, including
  // the ones where the check did not apply, because a coverage check that goes quiet when it could not
  // run is indistinguishable from one that ran and found nothing — the exact defect family this file
  // exists to close, and the reason `n/a` carries its own parenthesised reason.
  const tail =
    ` gh_failures=${Number.isFinite(ghFailures) ? ghFailures : 0}` +
    ` ${formatCoverage(coverage ?? { state: "unknown", unjudged: [], judgedWorkflows: [], silentWorkflows: [], detail: "" })}`;
  if (!latest) {
    return (
      "total_count=n/a pending=n/a oldest_pending_min=n/a skipped=n/a neutral=n/a mergeable=n/a sha=unknown" + tail
    );
  }
  // Every field is read defensively. This is a FORMATTER on the failure path — it runs when something
  // has already gone wrong — so it must degrade to `n/a` rather than throw and take the verdict line
  // down with it. A poll that crashes while printing why it could not read CI is the fail-open again,
  // one layer up.
  const age = latest.oldestPendingAgeMs == null ? "n/a" : String(Math.round(latest.oldestPendingAgeMs / 60_000));
  return (
    `total_count=${latest.totalCount ?? "n/a"} pending=${latest.pending ?? "n/a"} oldest_pending_min=${age} ` +
    `skipped=${latest.skippedNames?.length ?? "n/a"} neutral=${latest.neutralCount ?? "n/a"} ` +
    `mergeable=${latest.mergeable ?? "n/a"} sha=${latest.sha ?? "unknown"}` +
    tail
  );
}

/**
 * THE single predicate for "what did CI actually say" — the verdict prefix and the exit code both come
 * out of here, and nothing else decides either (CPE-1906 round 2).
 *
 * WHY THIS FUNCTION EXISTS. There used to be two predicates for "is this red": `formatVerdict` branched
 * on `failedNames`, and the exit code branched on `failedNames || conclusion === "failure"`. They
 * disagreed on two real shapes. (i) A board whose only finished checks were skips the workflows
 * explain: it printed `CI VERDICT: completed skipped — … Skipped by design: …` and exited **1**, which
 * this file's own table defines as "at least one check FAILED" — with zero failures. (ii) A `--run`
 * whose run-level `conclusion` is `failure` with no failing job and one unexplained skip: it printed
 * the "neither red nor green" sentence and exited 1. And because `completed skipped` was the prefix for
 * BOTH exit 4 and that exit 1, the prefix discriminated nothing — a caller grepping the line could not
 * tell which had happened.
 *
 * The ladder, in order, and every rung is "fail closed":
 *   pending      not done — the budget ran out.
 *   failure      a named failing check, OR a run-level `failure` conclusion. Red outranks everything:
 *                the caller's next move is the logs whatever else is true.
 *   did-not-run  an unexplained skip; or nothing that finished actually RAN (`ranCount === 0`, e.g.
 *                every finished check was a by-design skip — explained, but it verified nothing); or a
 *                board that finished with no checks on it at all.
 *   success      a positive `success` conclusion with something having run. `--run` mode reports
 *                `skipped` when GitHub said `success` and at least one job skipped; by the time it
 *                reaches here the skips have been adjudicated (unexplained ones exited above) and
 *                `ranCount > 0`, so it is a pass — never "skipped folded into success" ahead of the
 *                adjudication, which is the hole CPE-1906 opened this file to close.
 *   unclear      done, nothing failed, and no positive evidence of success. Not a pass. Exit 4 with a
 *                DIFFERENT prefix, because "I do not recognise this board" is not "a job did not run".
 *
 * CPE-1970 inserted TWO rungs between `did-not-run` and `success`, and the placement is the argument:
 *   stale-checks      every check that ran passed, but a job `main` requires produced no check at all,
 *                     so a guard that exists on `main` never judged this PR. Below `failure` and
 *                     `did-not-run` because those are more specific facts about the board and the
 *                     caller's next move for them (read the logs / find out why nothing ran) outranks
 *                     "rebase and re-run". Above `success` because it is emphatically not a pass.
 *   coverage-unknown  the coverage check could not be computed. "Did not run" is not "found nothing";
 *                     fail closed. Same exit code, different prefix, because the caller's move differs
 *                     (rebase vs. fix your checkout).
 * Both sit AFTER the `pending` rung on purpose: jobs enter a rollup in waves, so "absent" only means
 * anything once `decideFromReads` has seen the board hold quiet across two reads.
 *
 * @param {{done: boolean, reason: string}} decision
 * @param {CiRead|null} latest
 * @param {string[]} [unexplainedSkips]
 * @param {Coverage|null} [coverage]
 * @returns {{kind: "pending"|"failure"|"did-not-run"|"stale-checks"|"coverage-unknown"|"success"|"unclear", code: number, why: string}}
 */
export function verdictClass(decision, latest, unexplainedSkips = [], coverage = null) {
  if (!decision?.done) return { kind: "pending", code: 2, why: "the budget ran out before CI finished" };
  const failed = latest?.failedNames ?? [];
  if (failed.length > 0) return { kind: "failure", code: 1, why: "a check reported a hard failure" };
  if (latest?.conclusion === "failure") {
    return { kind: "failure", code: 1, why: "the run-level conclusion is `failure`" };
  }
  if ((unexplainedSkips?.length ?? 0) > 0) {
    return { kind: "did-not-run", code: 4, why: "a check was skipped and no job-level `if:` explains it" };
  }
  if ((latest?.totalCount ?? 0) === 0) {
    return { kind: "did-not-run", code: 4, why: "the board finished with no checks on it at all" };
  }
  if (typeof latest?.ranCount === "number" && latest.ranCount === 0) {
    return { kind: "did-not-run", code: 4, why: "every check that finished was SKIPPED — nothing ran" };
  }
  // CPE-1929's two green sabotages, run by hand on 2026-08-28 because a new refusal is exactly where a
  // shadowed guard hides, and the numbers belong here rather than in a PR body. Suite:
  // `src/lib/ciPollFailClosed.test.ts`. Round 1, at 63 tests:
  //   · disable the rung (`if (false && …)`)             → 3 failed / 60 passed
  //   · force the predicate to lie (`coverageOf` always returning `ok`) → 7 failed / 56 passed
  // RE-RUN in round 2, at 70 tests, because the round-2 fixes changed both this rung's inputs and the
  // suite around it — a sabotage number carried forward unchanged is the same stale-evidence defect
  // this PR is about:
  //   · disable the rung                                 → 3 failed / 67 passed
  //   · force `coverageOf` to always answer `ok`         → 11 failed / 59 passed
  // RE-RUN AGAIN in round 3, at 75 tests, for the same reason — round 3 widened what `coverageOf`
  // classifies as PR-triggered, which is one of this rung's inputs:
  //   · disable the rung                                 → 3 failed / 72 passed
  //   · force `coverageOf` to always answer `ok`         → 12 failed / 63 passed
  // Both red. The 3 are a STRICT SUBSET of the 12 (`REFUSES the #1056 board`, `names the guard that did
  // not judge it`, `counts the gap on the machine-readable totals line`), which is the shape that says
  // the rung is reached rather than shadowed: sabotaging only the rung still reds, so no earlier check
  // in the ladder is answering this question first.
  if (coverage?.state === "unjudged") {
    return {
      kind: "stale-checks",
      code: 5,
      why: `${coverage.unjudged.length} job(s) \`main\` requires produced no check on this board`,
    };
  }
  if (coverage?.state === "unknown") {
    return { kind: "coverage-unknown", code: 5, why: "the guard-coverage check could not be computed" };
  }
  if (latest?.conclusion === "success" || latest?.conclusion === "skipped") {
    return { kind: "success", code: 0, why: "every check that ran concluded success" };
  }
  return {
    kind: "unclear",
    code: 4,
    why: `the board finished with conclusion=${latest?.conclusion ?? "none"} and no check reported a failure`,
  };
}

/**
 * The single terminal line every invocation prints. One line, greppable, carrying every number
 * sprint.md requires a poll to state — so a report quoting it is self-evidently a real result rather
 * than a promise to report later.
 *
 * @param {{done: boolean, reason: string}} decision
 * @param {CiRead|null} latest
 * @param {{ticks: number, elapsedMs: number, target: string}} run
 * @param {{unexplainedSkips?: string[], explainedSkips?: string[], ghFailures?: number, coverage?: Coverage|null, baseRef?: string|null}} [skips]
 * @returns {string}
 */
export function formatVerdict(decision, latest, run, skips = {}) {
  const coverage = skips.coverage ?? null;
  const totals = totalsOf(latest, skips.ghFailures ?? 0, coverage);
  const timing = `after ${run.ticks} tick(s) / ${Math.round(run.elapsedMs / 1000)}s`;
  const unexplained = skips.unexplainedSkips ?? [];
  const explained = skips.explainedSkips ?? [];
  const byDesign = explained.length > 0 ? ` Skipped by design: ${explained.join(", ")}.` : "";
  // CPE-1970 round 2: appended to EVERY branch below, not just `stale-checks` and `success`. The doc
  // claimed "the ref and its short SHA are printed on every verdict line" while two branches printed it
  // — a claim about output that the output did not make. `src/lib/ciPollFailClosed.test.ts` now pins it
  // across the whole stub matrix rather than on the two lines someone remembered. Red-proofed: dropping
  // `${against}` from the `failure` branch alone reds that test, naming `failure-and-skips` and printing
  // the offending line. It is appended wherever the guard set was actually READ — `--run` mode and a
  // still-pending board never read it, and both say so with `coverage=n/a(…)`.
  const against = skips.baseRef ? ` Guard set read from ${skips.baseRef}.` : "";
  // ONE predicate, shared with main()'s exit code. Never re-derive "is this red" here.
  const klass = verdictClass(decision, latest, unexplained, coverage);
  if (klass.kind === "failure") {
    const failed = latest?.failedNames ?? [];
    const detail =
      failed.length > 0
        ? `Failed: ${failed.join(", ")}.`
        : `No individual check reported a failure, but the run-level conclusion is \`failure\` — ` +
          `treat it as red and read the run's own log.`;
    return `CI VERDICT: completed failure — ${totals} ${timing} — ${decision.reason}. ${detail} Do not merge.${against}`;
  }
  if (klass.kind === "did-not-run") {
    const detail =
      unexplained.length > 0
        ? `${unexplained.length} check(s) DID NOT RUN and no job-level \`if:\` explains it: ` +
          `${unexplained.join(", ")}. Almost certainly a \`needs:\` cascade off an earlier failure.`
        : `${klass.why} — so nothing here verified this commit.${byDesign}`;
    return (
      `CI VERDICT: completed did-not-run — ${totals} ${timing} — ${decision.reason}. ${detail} ` +
      `This is neither red nor green. Do not merge; find out why they did not run.${against}`
    );
  }
  if (klass.kind === "stale-checks") {
    return (
      `CI VERDICT: completed stale-checks — ${totals} ${timing} — ${decision.reason}. ` +
      `Nothing on this board is red, and that is the problem: ${coverage?.detail ?? klass.why}` +
      `${against} This PR's checks CANNOT have judged those guards. Do not merge on this board.`
    );
  }
  if (klass.kind === "coverage-unknown") {
    return (
      `CI VERDICT: completed coverage-unknown — ${totals} ${timing} — ${decision.reason}. ` +
      `Nothing failed, but ${coverage?.detail ?? klass.why} — so this poll cannot say whether ` +
      `\`main\`'s guards judged this PR. That is "did not run", not "nothing to check". Do not merge.${against}`
    );
  }
  if (klass.kind === "unclear") {
    return (
      `CI VERDICT: completed unclear — ${totals} ${timing} — ${decision.reason}. ${klass.why} — a shape ` +
      `this poll has never seen, so it is NOT a pass. Do not merge; read the board by hand.${byDesign}${against}`
    );
  }
  if (klass.kind === "success") {
    return `CI VERDICT: completed success — ${totals} ${timing} — ${decision.reason}.${byDesign}${against}`;
  }
  const sha = latest?.sha ?? "unknown";
  const age =
    latest && latest.oldestPendingAgeMs != null
      ? ` Oldest pending check: "${latest.oldestPendingName}", running ` +
        `${Math.round(latest.oldestPendingAgeMs / 60_000)}m — compare that against the same job on a ` +
        `sibling PR to tell slow from hung.`
      : "";
  return (
    `CI VERDICT: pending — ${totals} ${timing} — ${decision.reason}. ` +
    `CI still pending on ${sha} — re-invoke this poll or hand CI to the Foreman.${age}${against}`
  );
}

/**
 * The verdict for "I could not ask" (CPE-1906 gap 2). Deliberately does NOT report `pending` as its
 * state, and deliberately says what the caller should do, because the whole failure being closed here is
 * a caller reading an error as "wait longer".
 *
 * @param {{kind: string, message: string, count: number}} failure
 * @param {CiRead|null} lastGood
 * @param {{ticks: number, elapsedMs: number, target: string}} run
 * @returns {string}
 */
export function formatErrorVerdict(failure, lastGood, run) {
  const timing = `after ${run.ticks} tick(s) / ${Math.round(run.elapsedMs / 1000)}s`;
  const seen = lastGood
    ? `Last successful read: ${totalsOf(lastGood, failure.count)} — but it is STALE and must not be merged on.`
    : "No successful read was ever obtained.";
  return (
    `CI VERDICT: unknown — could not ask GitHub (${failure.kind}) — ${totalsOf(null, failure.count)} ${timing} — ` +
    `${failure.count} consecutive \`gh\` failure(s), last: ${failure.message}. ` +
    `This is NOT a pending board and NOT a green one: nothing was read, so do not merge and do not ` +
    `wait on it. Check \`gh auth status\`, the run/PR id, and the network, then re-invoke. ${seen}`
  );
}

/**
 * @param {string[]} argv
 * @returns {{mode: "run"|"pr", target: string, budgetMs: number, intervalMs: number}}
 */
export function parseArgs(argv) {
  /** @type {"run"|"pr"|null} */ let mode = null;
  let target = "";
  let budgetMs = DEFAULT_BUDGET_MS;
  let intervalMs = DEFAULT_INTERVAL_MS;
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    const next = argv[i + 1];
    if (arg === "--run" || arg === "--pr") {
      if (!next) throw new Error(`${arg} needs a value`);
      mode = arg === "--run" ? "run" : "pr";
      target = next;
      i += 1;
    } else if (arg === "--budget") {
      if (!next) throw new Error("--budget needs a value in seconds");
      budgetMs = clampBudgetMs(Number(next) * 1000);
      i += 1;
    } else if (arg === "--interval") {
      if (!next) throw new Error("--interval needs a value in seconds");
      intervalMs = Number(next) * 1000;
      // CPE-1906 gap 3: `--interval 0` used to sail through here and blow up two frames later inside
      // `assertNotBackgroundable`, escaping main() as an unhandled rejection with a raw stack trace and
      // exit 1 — which this file's own table defines as "CI failed". Bad input reported as a red build
      // is how somebody spends an hour debugging the wrong thing. Validate it where it is read.
      if (!Number.isFinite(intervalMs) || intervalMs <= 0) {
        throw new Error(`--interval must be a positive number of seconds, got ${next}`);
      }
      i += 1;
    } else {
      throw new Error(`unknown argument: ${arg}`);
    }
  }
  if (!mode) throw new Error("one of --run <run-id> or --pr <number> is required");
  return { mode, target, budgetMs: clampBudgetMs(budgetMs), intervalMs };
}

/** @param {string} label */
function stamp(label) {
  const now = new Date();
  const local = now.toLocaleTimeString(undefined, { hour12: false });
  const utc = now.toISOString().slice(11, 19);
  return `${local} (${utc}Z) ${label}`;
}

/** @param {number} ms */
function sleepSync(ms) {
  // Busy-free blocking sleep with no dependency: Atomics.wait on a throwaway buffer.
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

/**
 * Run one `gh` invocation, bounded (CPE-1906 gap 1).
 *
 * `CI_POLL_GH_SCRIPT` is a TEST SEAM and nothing else: it names a Node script to run in place of the
 * real `gh`, so `src/lib/ciPollFailClosed.test.ts` can drive an erroring / hanging / garbage-emitting
 * `gh` and assert on the verdict line and the exit code — the two things a caller consumes. A PATH shim
 * would have been the obvious alternative and does not work here: Node refuses to spawn a `.cmd`
 * without `shell: true` (the CVE-2024-27980 fix), so a Windows developer could not run the test at all.
 *
 * @param {string[]} args
 * @param {number} timeoutMs
 * @returns {string}
 */
function gh(args, timeoutMs) {
  const stub = process.env.CI_POLL_GH_SCRIPT;
  const file = stub ? process.execPath : "gh";
  const argv = stub ? [stub, ...args] : args;
  return execFileSync(file, argv, {
    encoding: "utf8",
    maxBuffer: 32 * 1024 * 1024,
    timeout: timeoutMs,
    killSignal: "SIGKILL",
    stdio: ["ignore", "pipe", "pipe"],
  });
}

/**
 * Turn whatever `gh` threw into a short, classified reason. The kind is what makes the verdict line
 * actionable — "timed out" and "gh exited non-zero" call for different things from the caller.
 *
 * @param {unknown} err
 * @returns {{kind: string, message: string}}
 */
export function classifyGhFailure(err) {
  const e = /** @type {any} */ (err);
  const first = (e?.message ? String(e.message) : String(err)).split("\n")[0].slice(0, 300);
  // CPE-1906 round 2. Matched on `name` rather than `instanceof` so it survives a payload thrown across
  // a module realm; the kind is separate from "unparseable output" because the caller's move differs —
  // unparseable means a proxy or a banner, wrong-shape means the id, the token or the API is wrong.
  if (e?.name === "GhPayloadShapeError") return { kind: "unexpected payload shape", message: first };
  if (e instanceof SyntaxError) return { kind: "unparseable output", message: first };
  if (e?.killed === true || e?.signal === "SIGKILL" || e?.code === "ETIMEDOUT") {
    return { kind: "timed out", message: `gh exceeded its per-call timeout — ${first}` };
  }
  if (e?.code === "ENOENT") return { kind: "gh not found", message: first };
  const stderr = typeof e?.stderr === "string" && e.stderr.trim() ? e.stderr.trim().split("\n")[0].slice(0, 300) : "";
  return { kind: "gh exited non-zero", message: stderr || first };
}

async function main() {
  let opts;
  let ticks;
  let worst;
  try {
    opts = parseArgs(process.argv.slice(2));
    // Inside the same try as parseArgs on purpose (CPE-1906 gap 3): these throw RangeError on bad input
    // too, and a usage error must exit 64 whichever validator catches it — never 1, which this file's
    // own table defines as "CI failed".
    worst = assertNotBackgroundable(opts.budgetMs, opts.intervalMs);
    ticks = planTickCount(opts.budgetMs, opts.intervalMs);
  } catch (err) {
    console.error(`ci-poll: ${err instanceof Error ? err.message : String(err)}`);
    console.error("usage: node scripts/ci-poll.mjs (--run <run-id> | --pr <number>) [--budget <s>] [--interval <s>]");
    process.exit(64);
    return;
  }

  const started = Date.now();
  console.log(
    stamp(
      `ci-poll: ${opts.mode} ${opts.target} — up to ${ticks} tick(s) every ${opts.intervalMs / 1000}s ` +
        `(worst case ${Math.round(worst / 1000)}s, structural bound ` +
        `${Math.round(boundedWallClockMs(opts.budgetMs) / 1000)}s, harness cap ${HARNESS_TOOL_TIMEOUT_MS / 1000}s)`,
    ),
  );

  // THE enforcement of the whole premise. `ticks` is only a plan; this is the bound. Every sleep is
  // gated on the real clock, so a slow `gh` call (or a slow network, or a machine under load from six
  // sibling agents) eats into the budget instead of silently pushing the run past the harness cap. The
  // modelled worst case above assumed a 5 s `gh` round-trip; at 15 s the shipped defaults would have run
  // 690 s and been backgrounded — exactly the failure this script exists to make impossible.
  const deadline = started + opts.budgetMs;

  /** @type {CiRead[]} */
  const reads = [];
  let decision = { done: false, reason: "no reads yet" };
  let tick = 0;
  let stoppedOnDeadline = false;
  let consecutiveFailures = 0;
  let totalFailures = 0;
  /** @type {{kind: string, message: string}|null} */ let lastFailure = null;
  let bailedOnFailures = false;
  for (; tick < ticks; tick += 1) {
    /** @type {CiRead} */
    let read;
    try {
      const budgetForCall = ghCallTimeoutMs(Date.now(), deadline);
      // `assertReadableShape` sits INSIDE this try on purpose (CPE-1906 round 2): a `gh` that exits 0
      // with a payload that is not a board has not answered, which means the same thing to the caller
      // as a `gh` that threw. Same counter, same bail, same exit 3 — one path, not two.
      if (opts.mode === "run") {
        const json = assertReadableShape(
          JSON.parse(gh(["run", "view", opts.target, "--json", "status,conclusion,headSha,jobs"], budgetForCall)),
          "run",
        );
        read = readFromRunJson(json);
      } else {
        const json = assertReadableShape(
          JSON.parse(gh(["pr", "view", opts.target, "--json", "mergeable,headRefOid,statusCheckRollup"], budgetForCall)),
          "pr",
        );
        read = readFromPrJson(json);
      }
    } catch (err) {
      lastFailure = classifyGhFailure(err);
      consecutiveFailures += 1;
      totalFailures += 1;
      console.log(
        stamp(
          `ci-poll: gh read failed (${consecutiveFailures}/${MAX_CONSECUTIVE_GH_FAILURES}, ` +
            `${lastFailure.kind}) — ${lastFailure.message}`,
        ),
      );
      if (consecutiveFailures >= MAX_CONSECUTIVE_GH_FAILURES) {
        bailedOnFailures = true;
        tick += 1;
        break;
      }
      if (!shouldSleepAgain(Date.now(), opts.intervalMs, deadline, tick, ticks)) {
        stoppedOnDeadline = true;
        tick += 1;
        break;
      }
      sleepSync(opts.intervalMs);
      continue;
    }
    consecutiveFailures = 0;
    reads.push(read);
    decision = decideFromReads(reads);
    const age = read.oldestPendingAgeMs === null ? "n/a" : `${Math.round(read.oldestPendingAgeMs / 60_000)}m`;
    console.log(
      stamp(
        `ci-poll: total_count=${read.totalCount} pending=${read.pending} oldest_pending=${age} ` +
          `skipped=${read.skippedNames.length} mergeable=${read.mergeable ?? "n/a"} ` +
          `sha=${(read.sha ?? "unknown").slice(0, 7)} — ${decision.reason}` +
          (read.oldestPendingName ? ` (oldest: ${read.oldestPendingName})` : ""),
      ),
    );
    if (decision.done) {
      tick += 1;
      break;
    }
    if (!shouldSleepAgain(Date.now(), opts.intervalMs, deadline, tick, ticks)) {
      stoppedOnDeadline = true;
      tick += 1;
      break;
    }
    sleepSync(opts.intervalMs);
  }
  if (stoppedOnDeadline) {
    console.log(stamp(`ci-poll: wall-clock budget reached — stopping before the next tick would cross it`));
  }

  const run = { ticks: Math.max(tick, 1), elapsedMs: Date.now() - started, target: opts.target };
  const latest = reads.length > 0 ? reads[reads.length - 1] : null;

  // COULD NOT ASK, and it is never allowed to look like anything else. Two ways in: N failures in a row,
  // or a poll that ran its whole budget and never once got an answer. Both mean the same thing to the
  // caller — nothing was read — so both get the same unmistakable verdict rather than `pending`.
  if (bailedOnFailures || (reads.length === 0 && totalFailures > 0)) {
    const failure = lastFailure ?? { kind: "unknown", message: "no detail" };
    console.log(formatErrorVerdict({ ...failure, count: consecutiveFailures || totalFailures }, latest, run));
    process.exit(3);
  }

  const sources = readWorkflowSources();
  // Fail closed: an empty scan is "I could not check", not "nothing to explain".
  const matchers = sources.length === 0 ? null : explainableSkipMatchers(sources);
  const { explained, unexplained } = classifySkips(latest?.skippedNames ?? [], matchers);

  // CPE-1970 — guard coverage. Three reasons it may not apply, and each one is PRINTED rather than
  // implied, because a check that goes quiet reads as a check that passed:
  //   · `--run` mode polls ONE workflow run, whose job list cannot answer "did every job main requires
  //     across ALL workflows appear". Out of scope, not a pass.
  //   · a board that is still pending has jobs yet to be scheduled, so "absent" means nothing yet.
  //   · a `gh` that never answered leaves no check names to compare (handled above by the exit-3 path).
  /** @type {Coverage} */
  let coverage;
  /** @type {string|null} */ let baseRef = null;
  if (opts.mode !== "pr") {
    coverage = { state: "n/a", unjudged: [], judgedWorkflows: [], silentWorkflows: [], detail: "run-mode" };
  } else if (!decision.done) {
    coverage = { state: "n/a", unjudged: [], judgedWorkflows: [], silentWorkflows: [], detail: "board-pending" };
  } else {
    const base = readBaseWorkflowSources();
    baseRef = base ? `${base.ref}${base.sha ? `@${base.sha}` : ""}` : null;
    coverage = coverageOf(latest?.checkNames ?? [], base?.files ?? null);
    if (coverage.silentWorkflows.length > 0) {
      console.log(
        stamp(
          `ci-poll: coverage — ${coverage.silentWorkflows.join(", ")} contributed no check to this ` +
            `board and is path-filtered on every PR trigger it declares, so GitHub was entitled not to run it; ` +
            `not treated as missing`,
        ),
      );
    }
  }

  console.log(
    formatVerdict(decision, latest, run, {
      explainedSkips: explained,
      unexplainedSkips: unexplained,
      ghFailures: totalFailures,
      coverage,
      baseRef,
    }),
  );

  // ONE predicate for the line above and the code below — see `verdictClass`. There used to be two, and
  // they disagreed: a board of nothing but by-design skips printed `completed skipped` and exited 1
  // ("at least one check FAILED") with zero failures.
  process.exit(verdictClass(decision, latest, unexplained, coverage).code);
}

// Only run the poll when this file is the process entry point — importing it (as the unit tests do)
// must never fire a real `gh` call.
if (process.argv[1] !== undefined && pathToFileURL(process.argv[1]).href === import.meta.url) {
  // A crash inside main() must not escape as a raw stack trace with exit 1 ("CI failed"). An internal
  // fault means the same thing to the caller as a dead `gh`: nothing was determined. Exit 3.
  try {
    await main();
  } catch (err) {
    console.error(`ci-poll: internal error — ${err instanceof Error ? err.message : String(err)}`);
    console.log(
      `CI VERDICT: unknown — ci-poll itself failed before reaching a verdict. Nothing was read; do not ` +
        `merge and do not wait on it. Re-invoke, and if it repeats this is a bug in scripts/ci-poll.mjs.`,
    );
    process.exit(3);
  }
}
