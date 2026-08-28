// CPE-1910 — should this shard's suite be run again, because its WebDriver session died before any
// assertion got the chance to run?
//
// Pure policy over facts that ALREADY EXIST. This module deliberately contains no text classification of
// its own: it consumes `lib/logSignature.ts`'s `LogSignatureResult` verbatim (the CPE-1728 classifier
// that already tells app-defect from runner-could-not-paint) and a spec-file count read through
// `lib/resultsDir.ts` (the same chunks `lib/ratchet.ts#evaluate` derives its `incomplete` clause from).
// A second classifier keyed on the same log text would be one classifier with two answers, which is the
// shape this repo keeps removing — so if the retry ever disagrees with the printed classification, that
// is a bug in this file's policy, not a second opinion.
//
// ---------------------------------------------------------------------------------------------------
// WHAT THIS EXISTS FOR, MEASURED (2026-08-28, 71 completed `gui-smoke (ubuntu-latest) shard 2` jobs over
// the 13.5 h ending 08:47Z; the whole 100-run window is 312 shard jobs):
//
//   * The trigger is not random and not "runner resource exhaustion". EVERY ONE of those 71 jobs logs
//     `handleRunnableStart:resetFailedRestartingSession` on the transition into
//     `checkpoint-restore.smoke.ts` — an ordinary app-level reset failure that hands control to
//     CPE-1886/CPE-1955's in-process recovery. That path's first act is `DELETE /session/<id>`.
//   * In ~15% of jobs (6 of the 40 sampled after CPE-1955 merged) that DELETE kills the NATIVE driver
//     behind tauri-driver. tauri-driver itself survives — it is the process still logging
//     `Error serving connection: hyper::Error(...)  tcp connect error: Connection refused (os error
//     111)` — and it answers every later request by closing the client socket, which reaches wdio as
//     `UND_ERR_SOCKET`. So the layer is the tauri-driver -> native-WebKitWebDriver hop, NOT the app under
//     test (the session is gone before the app matters), NOT tauri-driver's front door, and NOT the
//     runner (a resource-starved runner would not pick the same spec transition 71 times out of 71).
//   * CPE-1955's bounded in-process respawn already absorbs the FIRST such death: 3 of the 31 shard-2
//     jobs sampled BEFORE it merged were fatal (1 of 14 spec files reported, a manual `gh run rerun`
//     each); 0 of the 40 after it were.
//
// So this module is NOT the primary fix — CPE-1955 is, and it works. This is the backstop for the case
// CPE-1955 documents itself as deliberately leaving red: `MAX_DRIVER_RESPAWNS` is 1, so a SECOND
// transport death in one shard, or a respawn that itself fails, still ends the shard having asserted
// nothing. That still costs the manual re-run CPE-1910 was filed about, and on an unattended run it is a
// stall.
// ---------------------------------------------------------------------------------------------------
import { type LogSignatureResult } from "./logSignature.js";

/** One extra attempt. Two attempts total.
 *
 *  Not tunable upward without re-reading the measurement above: the population this catches is a SECOND
 *  transport death inside one shard, i.e. the tail of a ~15% per-job event.
 *
 *  WHAT AN ATTEMPT COSTS, MEASURED (round 2, over all 71 completed shard-2 jobs in the window above; an
 *  earlier draft of this comment asserted "6-14 min" from nobody's stopwatch): the `Run smoke suite` step
 *  is **1-2 min** and the whole shard job **2-3 min** — job `98661503323`'s suite step took **119 s**. So
 *  a retry is genuinely cheap and lands nowhere near the 30-minute `timeout-minutes` on the shard job
 *  (`gui-smoke.yml:628`); the budget of 2 is NOT sized by runtime.
 *
 *  It is sized by what a third attempt would MEAN. A transport that dies twice inside one job is a
 *  reportable defect rather than a blip, and re-running it again would start hiding a genuinely worsening
 *  rate behind green runs — the CPE-1893 failure this ticket's own acceptance criteria forbid. That is
 *  the whole argument, and it does not move if the runtime does. */
export const MAX_SUITE_ATTEMPTS = 2;

/** The literal substring `wdio.conf.ts#respawnTauriDriver` prints on each CPE-1955 in-process respawn.
 *
 *  NOT a claim about that file — `sessionRetry.test.ts` READS `wdio.conf.ts` and fails if this string is
 *  not present in its source, so a reworded message reds here instead of silently reporting 0 respawns
 *  forever (CPE-1933: derive provenance, never assert it).
 *
 *  RED-PROOFED, result at the site rather than only in the PR body: rewording `wdio.conf.ts`'s message
 *  to "restarting the tauri-driver process (attempt " fails `sessionRetry.test.ts` — **1 of 13** cases,
 *  "the marker it counts is really the string wdio.conf.ts prints". Reverting it returns 13/13. */
export const RESPAWN_LOG_MARKER = "respawning tauri-driver (respawn ";

/** How many times CPE-1955's in-process respawn fired during a captured suite log.
 *
 *  This number is the whole reason the summary block exists. Those respawns are, TODAY, completely
 *  silent outside ~14,000 lines of raw job log: 6 of the 40 post-CPE-1955 shard-2 jobs sampled used one,
 *  every one of them recovered, and nothing anywhere told a reader it had happened. That is exactly the
 *  "silent retries hide a worsening rate" shape CPE-1893 is open about, already live in this suite. */
export function countDriverRespawns(logText: string): number {
  let count = 0;
  let index = logText.indexOf(RESPAWN_LOG_MARKER);
  while (index !== -1) {
    count++;
    index = logText.indexOf(RESPAWN_LOG_MARKER, index + RESPAWN_LOG_MARKER.length);
  }
  return count;
}

export interface RetryDecisionInput {
  /** `lib/logSignature.ts#classifyLog`'s verdict for THIS attempt's captured output. Passed in already
   *  computed — this module never looks at the raw text for classification purposes. */
  signature: LogSignatureResult;
  /** Distinct spec files that reported at least one case, via `lib/resultsDir.ts` + `reduceResultChunks`
   *  — the same reduction `evaluate()` counts for its `incomplete` clause. */
  reportedSpecFiles: number;
  /** Spec files this shard was assigned (its own manifest), or the whole suite for an unsharded run. */
  expectedSpecFiles: number;
  /** Attempts already completed, including the one being judged. `1` on the first decision. */
  attemptsUsed: number;
  /** Normally `MAX_SUITE_ATTEMPTS`; a parameter so the tests can pin budget behaviour without editing
   *  the constant. */
  maxAttempts: number;
  /** False when the attempt's captured log could not be read at all. Fails CLOSED: see `decideSuiteRetry`. */
  suiteLogReadable: boolean;
}

export type RetryDecisionCode =
  | "retry-session-died-before-asserting"
  | "no-log-cannot-classify"
  | "budget-spent"
  | "assertion-evidence-present"
  | "no-environment-signature"
  | "suite-completed";

export interface RetryDecision {
  retry: boolean;
  code: RetryDecisionCode;
  /** One line, naming the fact that decided it, for the job log and the step summary. */
  reason: string;
}

/**
 * The policy. Retry iff ALL of:
 *
 *   1. the attempt's log was readable at all,
 *   2. `logSignature`'s verdict is `environment-signature-only` — no `AssertionError` anywhere, and
 *   3. fewer spec files reported than this shard owed, and
 *   4. the budget is not spent.
 *
 * **Conditions 2 and 3 are both load-bearing and neither shadows the other** — they exclude two
 * different real populations, measured in the same 2026-08-28 window:
 *
 *   * Condition 2 alone would be wrong. It is TEMPTING (the ticket's own summary quotes only the
 *     `ENVIRONMENT SIGNATURE ONLY` line) and it is WRONG: 24 of the 27 shard-2 failures in that window
 *     carried that exact verdict while reporting a healthy 14/14 spec files and failing one real case —
 *     `macro-param-prompt.smoke.ts :: running a bound {ask:suffix} macro …`. A spec that fails on
 *     `waitUntil`/expect-webdriverio throws a plain `Error`, not chai's `AssertionError`, so a genuine,
 *     reproducible regression classifies as environment-signature-only. Retrying on the verdict alone
 *     would re-run real regressions — including CPE-1960's `scrollIntoView` shape, whose ~90% rate would
 *     have been laundered into ~99% by a single retry.
 *   * Condition 3 alone would be wrong too: a shard killed mid-suite by a genuine app crash also reports
 *     fewer files than it owed, and re-running that is exactly the "hide the race" move CPE-1679
 *     refused.
 *
 * Together they name one narrow thing: *the transport died and this shard asserted nothing*.
 *
 * **Both sabotages were run by hand (CPE-1929's cheap half), and they red on DIFFERENT tests — which is
 * what proves neither is shadowing the other rather than merely asserting it.** With condition 2
 * disabled (`if (false && …)`): **3 of 20** cases fail, all of them the `AssertionError` ones. With
 * condition 3 disabled: **2 of 20** fail, both of them the real-regression-carrying-an-environment-
 * verdict ones. Disjoint failure sets, so each refusal is reachable and each is the only thing standing
 * between one real population and a wrongful re-run. Re-run the pair if you reorder these branches.
 *
 * **Fails closed on missing evidence.** A retry is the PERMISSIVE action here, so "could not read the
 * log" must never reach it: `suiteLogReadable === false` returns `retry: false` with a reason that says
 * the classification did not run, rather than a silent "nothing matched, carry on". This is the
 * distinction between *ran and found nothing* and *did not run* — the family this repo has found nine
 * instances of, and a retry wrapper is a prime place for the tenth.
 */
export function decideSuiteRetry(input: RetryDecisionInput): RetryDecision {
  const { signature, reportedSpecFiles, expectedSpecFiles, attemptsUsed, maxAttempts, suiteLogReadable } =
    input;

  if (!suiteLogReadable) {
    return {
      retry: false,
      code: "no-log-cannot-classify",
      reason:
        "the suite attempt produced no readable captured log, so log-signature could not classify it. " +
        "Not retrying: a retry is the permissive action and this is 'did not run', not 'ran and found " +
        "nothing'. The Ratchet step still gates this shard normally.",
    };
  }

  if (signature.verdict === "assertion-failures" || signature.verdict === "mixed") {
    return {
      retry: false,
      code: "assertion-evidence-present",
      reason:
        `log-signature verdict is "${signature.verdict}" (${signature.assertionErrorCount} AssertionError ` +
        "occurrence(s)) — a check actually ran and failed, so this red is evidence about the app or the " +
        "spec. Never retried, at any spec-file count.",
    };
  }

  if (signature.verdict === "no-signal") {
    return {
      retry: false,
      code: "no-environment-signature",
      reason:
        "log-signature found neither an AssertionError nor any known WebDriver/runner marker, so there is " +
        "no positive evidence the transport died. Not retried: 'we cannot see a cause' is not the same " +
        "fact as 'the cause was environmental', and only the second one earns a re-run.",
    };
  }

  if (reportedSpecFiles >= expectedSpecFiles) {
    return {
      retry: false,
      code: "suite-completed",
      reason:
        `all ${expectedSpecFiles} assigned spec file(s) reported, so the session survived long enough for ` +
        "every spec to run. Whatever failed here failed on its own merits (an expect-webdriverio timeout " +
        "carries no AssertionError, so the environment-signature verdict alone does NOT make it " +
        "environmental) — never retried.",
    };
  }

  if (attemptsUsed >= maxAttempts) {
    return {
      retry: false,
      code: "budget-spent",
      reason:
        `the WebDriver session died before asserting again, but the suite-attempt budget (${maxAttempts}) ` +
        "is spent. Leaving this shard RED: a transport that dies twice in one job is a reportable defect, " +
        "not a blip.",
    };
  }

  return {
    retry: true,
    code: "retry-session-died-before-asserting",
    reason:
      `only ${reportedSpecFiles} of ${expectedSpecFiles} assigned spec file(s) reported and log-signature ` +
      "found no AssertionError anywhere — the WebDriver session died before any check ran, so this " +
      "attempt is evidence about the runner, not the app.",
  };
}

export interface RetrySummaryInput {
  /** 1-based, or `undefined` for an unsharded local run. */
  shardIndex: number | undefined;
  /** Attempts actually executed. */
  attempts: number;
  /** CPE-1955 in-process tauri-driver respawns, summed over every attempt. */
  driverRespawns: number;
  /** The decision that ended the last attempt. */
  finalDecision: RetryDecision;
  /** Per-attempt one-liners, in order. */
  attemptNotes: string[];
  /** The budget this run ACTUALLY ran on — `run-suite.ts`'s `maxAttempts()`, which is
   *  `GUI_SMOKE_MAX_ATTEMPTS` when set and `MAX_SUITE_ATTEMPTS` otherwise.
   *
   *  Round 2: these two formatters used to print `MAX_SUITE_ATTEMPTS` directly while the loop ran on the
   *  override, so `GUI_SMOKE_MAX_ATTEMPTS=3` rendered the impossible "3 of 2 allowed". A summary that
   *  contradicts the run it is summarising is worse than one that omits the number. */
  maxAttempts: number;
}

/** Markdown for `$GITHUB_STEP_SUMMARY`. Emitted whenever ANYTHING recovered — a job-level retry OR a
 *  CPE-1955 in-process respawn — because a recovery nobody is told about is indistinguishable from a run
 *  that never had a problem, and that is how a worsening rate hides (CPE-1893). Deliberately loud
 *  (a heading, a warning glyph, explicit counts) and deliberately NOT a gate: it never changes an exit
 *  code, so it can be read as evidence without being argued with. */
export function formatRetrySummaryMarkdown(input: RetrySummaryInput): string[] {
  const { shardIndex, attempts, driverRespawns, finalDecision, attemptNotes, maxAttempts } = input;
  const who = shardIndex === undefined ? "GUI smoke suite" : `GUI smoke shard ${shardIndex}`;
  const lines: string[] = [
    `### ⚠️ ${who} — WebDriver session recovery happened on this run (CPE-1910)`,
    "",
    `| what | count |`,
    `| --- | --- |`,
    `| suite attempts run | **${attempts}** of ${maxAttempts} allowed |`,
    `| job-level suite retries used | **${attempts - 1}** |`,
    `| in-process tauri-driver respawns (CPE-1955) | **${driverRespawns}** |`,
    "",
  ];
  for (const [i, note] of attemptNotes.entries()) {
    lines.push(`- **attempt ${i + 1}** — ${note}`);
  }
  lines.push(
    "",
    `**Outcome:** ${finalDecision.reason}`,
    "",
    "This block appears *only* when something had to be recovered. A retry is not an exemption: the " +
      "Ratchet step still owns this shard's pass/fail verdict, a genuine assertion failure is never " +
      "retried, and a rising count here is a regression in the runner even while the job stays green — " +
      "see `gui-smoke/lib/sessionRetry.ts` for the measured rates this is sized against.",
  );
  return lines;
}

/** The same facts for the plain job log, where most readers actually look first. */
export function formatRetryLogLines(input: RetrySummaryInput): string[] {
  const { shardIndex, attempts, driverRespawns, finalDecision, maxAttempts } = input;
  const who = shardIndex === undefined ? "suite" : `shard ${shardIndex}`;
  return [
    "==================================================================================",
    `[gui-smoke session-retry] ${who.toUpperCase()} RECOVERED FROM A WEBDRIVER SESSION DEATH (CPE-1910)`,
    `[gui-smoke session-retry]   suite attempts run ................ ${attempts} of ${maxAttempts}`,
    `[gui-smoke session-retry]   job-level suite retries used ...... ${attempts - 1}`,
    `[gui-smoke session-retry]   in-process driver respawns ........ ${driverRespawns} (CPE-1955)`,
    `[gui-smoke session-retry]   final decision .................... ${finalDecision.code}`,
    `[gui-smoke session-retry]   ${finalDecision.reason}`,
    "==================================================================================",
  ];
}
