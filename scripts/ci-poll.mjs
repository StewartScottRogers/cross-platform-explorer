#!/usr/bin/env node
// CPE-1880 — a CI poll that CANNOT be backgrounded, replacing `gh run watch` in the dispatch contract.
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
//   It also mechanises the two poll traps sprint.md documents in prose, so they cannot be forgotten:
//   `total_count == 0` is never read as green (it is reported, with `mergeable` alongside, because a
//   CONFLICTING PR schedules no checks at all), and `pending == 0` is only trusted once `total_count` has
//   been STABLE ACROSS TWO READS (jobs schedule in waves, so pending dips before it rises).
//
// USAGE
//   node scripts/ci-poll.mjs --run <run-id>            # poll one workflow run
//   node scripts/ci-poll.mjs --pr <number>             # poll a PR's whole check rollup
//   node scripts/ci-poll.mjs --pr 1031 --budget 300    # shorter budget, in seconds (clamped, never raised)
//
// EXIT CODES
//   0  completed, conclusion success
//   1  completed, conclusion NOT success (failure/cancelled/timed_out) — read the logs
//   2  still pending when the budget ran out — this is a NORMAL, EXPECTED outcome, not an error.
//      Report the printed `CI still pending on <SHA>` line and hand CI to the Foreman, or re-invoke.
//   64 bad usage
//
// The pure functions below are exported and unit-tested by `src/lib/sprintStallControls.test.ts`; the
// `main()` path only runs when this file is executed directly.

import { execFileSync } from "node:child_process";
import { pathToFileURL } from "node:url";

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
 * Clamp a requested budget to something that cannot be auto-backgrounded.
 *
 * This is the structural guarantee: a caller may ask for LESS time, never more. There is no flag, env
 * var, or argument that raises the ceiling, so no future edit can reintroduce an unbounded wait by
 * configuration alone — it would take deleting this function.
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
  return worst;
}

/**
 * @typedef {object} CiRead One normalised observation of a run or a PR check rollup.
 * @property {boolean} terminal    the provider says the run/rollup has finished
 * @property {string|null} conclusion  success / failure / cancelled / …, once terminal
 * @property {number} totalCount   number of checks currently scheduled
 * @property {number} pending      number not yet reported
 * @property {string|null} mergeable  MERGEABLE / CONFLICTING / UNKNOWN, PR polls only
 * @property {string|null} sha      the head SHA the reading is keyed to
 */

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
  return { done: true, reason: `pending=0 with total_count stable at ${latest.totalCount} across two reads` };
}

/**
 * The single terminal line every invocation prints. One line, greppable, carrying every number
 * sprint.md requires a poll to state — so a report quoting it is self-evidently a real result rather
 * than a promise to report later.
 *
 * @param {{done: boolean, reason: string}} decision
 * @param {CiRead|null} latest
 * @param {{ticks: number, elapsedMs: number, target: string}} run
 * @returns {string}
 */
export function formatVerdict(decision, latest, run) {
  const totals = latest
    ? `total_count=${latest.totalCount} pending=${latest.pending} mergeable=${latest.mergeable ?? "n/a"} sha=${latest.sha ?? "unknown"}`
    : "total_count=n/a pending=n/a mergeable=n/a sha=unknown";
  const timing = `after ${run.ticks} tick(s) / ${Math.round(run.elapsedMs / 1000)}s`;
  if (decision.done) {
    const conclusion = latest?.conclusion ?? "unknown";
    return `CI VERDICT: completed ${conclusion} — ${totals} ${timing} — ${decision.reason}`;
  }
  const sha = latest?.sha ?? "unknown";
  return (
    `CI VERDICT: pending — ${totals} ${timing} — ${decision.reason}. ` +
    `CI still pending on ${sha} — re-invoke this poll or hand CI to the Foreman.`
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
      i += 1;
    } else {
      throw new Error(`unknown argument: ${arg}`);
    }
  }
  if (!mode) throw new Error("one of --run <run-id> or --pr <number> is required");
  return { mode, target, budgetMs: clampBudgetMs(budgetMs), intervalMs };
}

/**
 * Normalise `gh run view --json` output into a CiRead.
 * @param {any} json
 * @returns {CiRead}
 */
export function readFromRunJson(json) {
  const jobs = Array.isArray(json?.jobs) ? json.jobs : [];
  return {
    terminal: json?.status === "completed",
    conclusion: json?.conclusion ?? null,
    totalCount: jobs.length,
    pending: jobs.filter((/** @type {any} */ j) => j?.status !== "completed").length,
    mergeable: null,
    sha: json?.headSha ?? null,
  };
}

/**
 * Normalise `gh pr view --json statusCheckRollup,…` output into a CiRead.
 * @param {any} json
 * @returns {CiRead}
 */
export function readFromPrJson(json) {
  const rollup = Array.isArray(json?.statusCheckRollup) ? json.statusCheckRollup : [];
  const isPending = (/** @type {any} */ c) => {
    if (c?.__typename === "StatusContext") return c?.state === "PENDING" || c?.state === "EXPECTED";
    return c?.status !== "COMPLETED";
  };
  const pending = rollup.filter(isPending).length;
  return {
    // NEVER terminal, even at pending == 0. A workflow RUN has an authoritative `status: completed`;
    // a PR check rollup has no such signal — it is only ever "everything scheduled so far has
    // reported," and `gui-smoke` shards do not exist until their build job finishes. Marking this
    // terminal would short-circuit the two-read stability rule and reintroduce exactly the CPE-1863
    // dip-before-it-rises misread. Let decideFromReads() require the second read.
    terminal: false,
    conclusion:
      rollup.length === 0
        ? null
        : rollup.every((/** @type {any} */ c) => c?.conclusion === "SUCCESS" || c?.conclusion === "NEUTRAL" || c?.conclusion === "SKIPPED" || c?.state === "SUCCESS")
          ? "success"
          : "failure",
    totalCount: rollup.length,
    pending,
    mergeable: json?.mergeable ?? null,
    sha: json?.headRefOid ?? null,
  };
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

/** @param {string[]} args */
function gh(args) {
  return execFileSync("gh", args, { encoding: "utf8", maxBuffer: 32 * 1024 * 1024 });
}

async function main() {
  let opts;
  try {
    opts = parseArgs(process.argv.slice(2));
  } catch (err) {
    console.error(`ci-poll: ${err instanceof Error ? err.message : String(err)}`);
    console.error("usage: node scripts/ci-poll.mjs (--run <run-id> | --pr <number>) [--budget <s>] [--interval <s>]");
    process.exit(64);
    return;
  }

  const worst = assertNotBackgroundable(opts.budgetMs, opts.intervalMs);
  const ticks = planTickCount(opts.budgetMs, opts.intervalMs);
  const started = Date.now();
  console.log(
    stamp(
      `ci-poll: ${opts.mode} ${opts.target} — up to ${ticks} tick(s) every ${opts.intervalMs / 1000}s ` +
        `(worst case ${Math.round(worst / 1000)}s, harness cap ${HARNESS_TOOL_TIMEOUT_MS / 1000}s)`,
    ),
  );

  /** @type {CiRead[]} */
  const reads = [];
  let decision = { done: false, reason: "no reads yet" };
  let tick = 0;
  for (; tick < ticks; tick += 1) {
    /** @type {CiRead} */
    let read;
    try {
      if (opts.mode === "run") {
        read = readFromRunJson(JSON.parse(gh(["run", "view", opts.target, "--json", "status,conclusion,headSha,jobs"])));
      } else {
        read = readFromPrJson(
          JSON.parse(gh(["pr", "view", opts.target, "--json", "mergeable,headRefOid,statusCheckRollup"])),
        );
      }
    } catch (err) {
      console.log(stamp(`ci-poll: gh read failed — ${err instanceof Error ? err.message.split("\n")[0] : String(err)}`));
      if (tick < ticks - 1) sleepSync(opts.intervalMs);
      continue;
    }
    reads.push(read);
    decision = decideFromReads(reads);
    console.log(
      stamp(
        `ci-poll: total_count=${read.totalCount} pending=${read.pending} mergeable=${read.mergeable ?? "n/a"} ` +
          `sha=${(read.sha ?? "unknown").slice(0, 7)} — ${decision.reason}`,
      ),
    );
    if (decision.done) {
      tick += 1;
      break;
    }
    if (tick < ticks - 1) sleepSync(opts.intervalMs);
  }

  const latest = reads.length > 0 ? reads[reads.length - 1] : null;
  const verdict = formatVerdict(decision, latest, {
    ticks: Math.max(tick, 1),
    elapsedMs: Date.now() - started,
    target: opts.target,
  });
  console.log(verdict);

  if (!decision.done) process.exit(2);
  process.exit(latest?.conclusion === "success" ? 0 : 1);
}

// Only run the poll when this file is the process entry point — importing it (as the unit tests do)
// must never fire a real `gh` call.
if (process.argv[1] !== undefined && pathToFileURL(process.argv[1]).href === import.meta.url) {
  await main();
}
