#!/usr/bin/env node
// CPE-1880 — a CI poll that CANNOT be backgrounded, replacing `gh run watch` in the dispatch contract.
// CPE-1906 — and that cannot report an error, a hang, or a job that never ran as if CI had answered.
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
// EXIT CODES
//   0  completed, every check that ran concluded success (or NEUTRAL, which GitHub itself treats as
//      non-blocking), and every skipped check was skipped by design
//   1  completed, at least one check FAILED (or was cancelled/timed out) — read the logs
//   2  still pending when the budget ran out — this is a NORMAL, EXPECTED outcome, not an error.
//      Report the printed `CI still pending on <SHA>` line and hand CI to the Foreman, or re-invoke.
//   3  COULD NOT ASK. `gh` errored, hung, or returned unparseable output. This is NOT a pending board and
//      NOT a green one — nothing was read. Do not merge on it and do not wait on it: check `gh auth
//      status`, the run/PR id and the network, then re-invoke.
//   4  completed with NO failures, but one or more checks were SKIPPED without a job-level `if:` to
//      explain it — i.e. they did not run, almost certainly a `needs:` cascade off an earlier failure.
//      Not a red build and NOT a green one. Do not merge; find out why they were skipped.
//   64 bad usage
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
 * @property {number} neutralCount    checks that ran and declined to judge
 * @property {number|null} oldestPendingAgeMs  age of the longest-running unfinished check
 * @property {string|null} oldestPendingName   its name — "slow or hung?" made mechanical
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
    skippedNames: [],
    failedNames: [],
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
      entry.conditional = true;
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
 * The set of check-name prefixes GitHub is ALLOWED to skip: every job with a job-level `if:`, plus the
 * transitive closure of jobs that `needs:` one of those (a skipped dependency cascades, and nobody
 * should have to re-declare that).
 *
 * Prefixes rather than exact names because a matrix job's `name:` contains a `${{ matrix.… }}`
 * expression; the literal text before the first template expression is the part GitHub keeps verbatim.
 * A job with no `name:` reports under its id.
 *
 * @param {string[]} sources raw text of each workflow file
 * @returns {string[]}
 */
export function explainableSkipMatchers(sources) {
  /** @type {string[]} */
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
      const prefix = label.split("${{")[0].trim();
      if (prefix) matchers.push(prefix);
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
 * @param {string[]} skippedNames
 * @param {string[]|null} matchers
 * @returns {{explained: string[], unexplained: string[]}}
 */
export function classifySkips(skippedNames, matchers) {
  /** @type {string[]} */ const explained = [];
  /** @type {string[]} */ const unexplained = [];
  for (const name of skippedNames ?? []) {
    const ok = Array.isArray(matchers) && matchers.some((m) => m.length > 0 && String(name).startsWith(m));
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

// ── Reads ────────────────────────────────────────────────────────────────────────────────────────────

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
    skippedNames,
    failedNames,
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
    skippedNames,
    failedNames,
    neutralCount: finished.filter(isNeutral).length,
    oldestPendingAgeMs: oldest.ageMs,
    oldestPendingName: oldest.name,
  };
}

// ── Verdict lines ────────────────────────────────────────────────────────────────────────────────────

/** @param {CiRead|null} latest */
function totalsOf(latest) {
  if (!latest) {
    return "total_count=n/a pending=n/a oldest_pending_min=n/a skipped=n/a neutral=n/a mergeable=n/a sha=unknown";
  }
  // Every field is read defensively. This is a FORMATTER on the failure path — it runs when something
  // has already gone wrong — so it must degrade to `n/a` rather than throw and take the verdict line
  // down with it. A poll that crashes while printing why it could not read CI is the fail-open again,
  // one layer up.
  const age = latest.oldestPendingAgeMs == null ? "n/a" : String(Math.round(latest.oldestPendingAgeMs / 60_000));
  return (
    `total_count=${latest.totalCount ?? "n/a"} pending=${latest.pending ?? "n/a"} oldest_pending_min=${age} ` +
    `skipped=${latest.skippedNames?.length ?? "n/a"} neutral=${latest.neutralCount ?? "n/a"} ` +
    `mergeable=${latest.mergeable ?? "n/a"} sha=${latest.sha ?? "unknown"}`
  );
}

/**
 * The single terminal line every invocation prints. One line, greppable, carrying every number
 * sprint.md requires a poll to state — so a report quoting it is self-evidently a real result rather
 * than a promise to report later.
 *
 * @param {{done: boolean, reason: string}} decision
 * @param {CiRead|null} latest
 * @param {{ticks: number, elapsedMs: number, target: string}} run
 * @param {{unexplainedSkips?: string[], explainedSkips?: string[]}} [skips]
 * @returns {string}
 */
export function formatVerdict(decision, latest, run, skips = {}) {
  const totals = totalsOf(latest);
  const timing = `after ${run.ticks} tick(s) / ${Math.round(run.elapsedMs / 1000)}s`;
  const unexplained = skips.unexplainedSkips ?? [];
  const explained = skips.explainedSkips ?? [];
  if (decision.done) {
    const failed = latest?.failedNames ?? [];
    if (failed.length > 0) {
      return (
        `CI VERDICT: completed failure — ${totals} ${timing} — ${decision.reason}. ` +
        `Failed: ${failed.join(", ")}. Read the logs; do not merge.`
      );
    }
    if (unexplained.length > 0) {
      return (
        `CI VERDICT: completed skipped — ${totals} ${timing} — ${decision.reason}. ` +
        `${unexplained.length} check(s) DID NOT RUN and no job-level \`if:\` explains it: ` +
        `${unexplained.join(", ")}. This is neither red nor green — almost certainly a \`needs:\` ` +
        `cascade off an earlier failure. Do not merge; find out why they were skipped.`
      );
    }
    const conclusion = latest?.conclusion ?? "unknown";
    const note = explained.length > 0 ? ` Skipped by design: ${explained.join(", ")}.` : "";
    return `CI VERDICT: completed ${conclusion} — ${totals} ${timing} — ${decision.reason}.${note}`;
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
    `CI still pending on ${sha} — re-invoke this poll or hand CI to the Foreman.${age}`
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
    ? `Last successful read: ${totalsOf(lastGood)} — but it is STALE and must not be merged on.`
    : "No successful read was ever obtained.";
  return (
    `CI VERDICT: unknown — could not ask GitHub (${failure.kind}) — ${totalsOf(null)} ${timing} — ` +
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
      if (opts.mode === "run") {
        read = readFromRunJson(
          JSON.parse(gh(["run", "view", opts.target, "--json", "status,conclusion,headSha,jobs"], budgetForCall)),
        );
      } else {
        read = readFromPrJson(
          JSON.parse(gh(["pr", "view", opts.target, "--json", "mergeable,headRefOid,statusCheckRollup"], budgetForCall)),
        );
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

  console.log(formatVerdict(decision, latest, run, { explainedSkips: explained, unexplainedSkips: unexplained }));

  if (!decision.done) process.exit(2);
  if ((latest?.failedNames.length ?? 0) > 0 || latest?.conclusion === "failure") process.exit(1);
  if (unexplained.length > 0) process.exit(4);
  if (latest?.conclusion === "success") process.exit(0);
  // Anything else — no conclusion, a shape this code has never seen — is not a pass. Fail closed.
  process.exit(1);
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
