// CPE-1910 — the I/O wrapper `npm run suite` invokes: run the WebdriverIO suite, and if this shard's
// WebDriver session died before any assertion ran, run it once more. Mirrors the "pure lib module + thin
// CLI wrapper" split `run-ratchet.ts` and `classify-log.ts` established: every decision lives in
// `lib/sessionRetry.ts` (pure, unit-tested by `test:unit`), and this file only spawns processes, reads
// and writes files, and prints.
//
// Read `lib/sessionRetry.ts`'s header first — it carries the measurement this exists for, the layer the
// socket death actually happens in, and why CPE-1955's in-process respawn is the primary fix while this
// is only its backstop.
//
// WHAT IT DOES NOT DO. It does not gate. `npm run ratchet`, the very next step in `gui-smoke.yml`,
// remains the only thing that decides whether this shard passes — a retried run's SECOND attempt is
// ratcheted exactly as a first attempt would be, against the same `known-failing.json`, with no
// allowance of any kind. A retry cannot turn a red run green; it can only replace "the transport died
// and we learned nothing" with a real result. It also never retries a genuine assertion failure: see
// `decideSuiteRetry`'s two load-bearing conditions.
//
// EXIT CODE. 0 whenever the suite RAN, whatever the suite's own exit code — the same deliberate
// swallowing the old inline `(xvfb-run … npm test | tee …) || true` step did, and for the same CPE-1594
// reason (a run with known-failing specs is expected to exit non-zero, and the ratchet is what judges
// it). Non-zero ONLY when this script itself could not do its job: the suite command could not be
// spawned, or a results file could not be read/parsed. That distinction is the point — "did not run"
// must be loud and must fail the step, never be reported as "ran and found nothing".
//
// Env overrides, same convention as the other two wrappers:
//   GUI_SMOKE_SUITE_CMD       — the suite command line (default "npm test"). Used by
//                               `lib/runSuite.integration.test.ts` to drive this real script with a stub
//                               suite; NOT a production knob.
//   GUI_SMOKE_MAX_ATTEMPTS    — override the attempt budget (default MAX_SUITE_ATTEMPTS).
//   GUI_SMOKE_RESULTS_DIR     — where the reporter writes (default "./.results"), as run-ratchet.ts.
//   GUI_SMOKE_SPECS_DIR       — spec dir for the unsharded expectation (default "./specs").
//   GITHUB_STEP_SUMMARY       — GitHub's own; when set, the loud block is appended there too.
import { spawn } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

import { DRIVER_PORTS } from "../lib/driverPorts.js";
import { classifyLog } from "../lib/logSignature.js";
import { reduceResultChunks } from "../lib/ratchet.js";
import { readResultChunks } from "../lib/resultsDir.js";
import {
  countDriverRespawns,
  decideSuiteRetry,
  formatRetryLogLines,
  formatRetrySummaryMarkdown,
  MAX_SUITE_ATTEMPTS,
  type RetryDecision,
} from "../lib/sessionRetry.js";
import { assignShardSpecs, parseShardId, SHARD_MANIFEST_PREFIX } from "../lib/shard.js";
import { listSpecFiles } from "../lib/specFiles.js";
import { waitForPortFree } from "../lib/waitForPort.js";

const RESULTS_DIR = process.env.GUI_SMOKE_RESULTS_DIR ?? path.resolve(process.cwd(), ".results");
const SPECS_DIR = process.env.GUI_SMOKE_SPECS_DIR ?? path.resolve(process.cwd(), "specs");
const SUITE_LOG = path.join(RESULTS_DIR, "suite-output.log");
const SUITE_CMD = process.env.GUI_SMOKE_SUITE_CMD ?? "npm test";

function log(line: string): void {
  // eslint-disable-next-line no-console
  console.log(line);
}

/** The attempt budget. An unparseable override is a hard error rather than a silent fall back to the
 *  default: a typo'd budget that quietly means "1" would remove this whole mechanism while every log
 *  line still said it was armed. */
function maxAttempts(): number {
  const raw = process.env.GUI_SMOKE_MAX_ATTEMPTS;
  if (raw === undefined || raw === "") return MAX_SUITE_ATTEMPTS;
  const parsed = Number(raw);
  if (!Number.isInteger(parsed) || parsed < 1) {
    throw new Error(
      `[gui-smoke session-retry] GUI_SMOKE_MAX_ATTEMPTS must be a positive integer, got ${JSON.stringify(raw)}.`,
    );
  }
  return parsed;
}

/** How many spec files this attempt OWED. The shard's own assignment when sharded (derived through
 *  `lib/shard.ts`, the same partition `write-shard-manifest.ts` and the ratchet use — never re-derived
 *  here), the whole flat suite otherwise. */
function expectedSpecFiles(): { expected: number; shardIndex: number | undefined } {
  const all = listSpecFiles(SPECS_DIR);
  const id = parseShardId(process.env);
  if (!id) return { expected: all.length, shardIndex: undefined };
  return { expected: assignShardSpecs(all, id).length, shardIndex: id.shardIndex };
}

/** Distinct spec files that reported at least one case — the same number `lib/ratchet.ts#evaluate`
 *  compares for its `incomplete` clause, computed from the same chunks through the same reduction. */
function reportedSpecFiles(): number {
  const { chunks } = readResultChunks(RESULTS_DIR);
  if (chunks === undefined) return 0;
  return new Set(reduceResultChunks(chunks).map((r) => r.spec)).size;
}

/** Runs the suite once, streaming its combined output to this process's stdout AND to `SUITE_LOG` — the
 *  `tee` the workflow step used to do inline, kept because `classify-log` and the uploaded artifact both
 *  read that exact file. Resolves with the suite's own exit code; REJECTS only when the process could not
 *  be started at all, which is a genuine "did not run". */
function runSuiteOnce(): Promise<number> {
  return new Promise((resolve, reject) => {
    fs.mkdirSync(RESULTS_DIR, { recursive: true });
    const sink = fs.createWriteStream(SUITE_LOG, { flags: "w" });
    // `shell: true` so GUI_SMOKE_SUITE_CMD can be an ordinary command line ("npm test") exactly as the
    // workflow step wrote it, rather than this script inventing an argv-splitting rule of its own.
    const child = spawn(SUITE_CMD, { shell: true, stdio: ["ignore", "pipe", "pipe"] });
    const tee = (b: Buffer): void => {
      process.stdout.write(b);
      sink.write(b);
    };
    child.stdout.on("data", tee);
    child.stderr.on("data", tee);
    child.once("error", (err) =>
      reject(new Error(`[gui-smoke session-retry] could not spawn the suite (${SUITE_CMD}): ${err.message}`)),
    );
    // `close` (not `exit`) fires only once BOTH stdio streams have ended, so the log file has every byte
    // by the time it runs. `sink.end`'s callback then guarantees the bytes are flushed to disk before the
    // caller reads the file back to classify it — a race that, left open, would classify a truncated log
    // and could decide "no AssertionError anywhere" from an incomplete read.
    child.once("close", (code) => sink.end(() => resolve(code ?? 1)));
  });
}

/** Moves an attempt's evidence aside so the next attempt starts from a clean `.results/` and the ratchet
 *  judges the FINAL attempt only — while nothing is destroyed.
 *
 *  The per-spec JSON goes to `.results/attempt-<n>/`, a SUBdirectory: `readResultChunks` and the
 *  workflow's `path: gui-smoke/.results/*.json` upload are both flat, so archived chunks can neither be
 *  double-counted by the ratchet nor corrupt the verdict job's cross-shard join with a duplicate spec.
 *  The captured LOG goes to `.results/suite-output.attempt-<n>.log`, which the suite-log artifact's
 *  widened `suite-output*.log` glob DOES pick up — that log is the evidence every occurrence of this
 *  failure was actually diagnosed from, so it is the one thing that must survive to the artifact.
 *
 *  THE SHARD MANIFEST IS NOT AN ATTEMPT'S EVIDENCE AND MUST NOT MOVE. `.results/` holds two kinds of
 *  `*.json`: the per-spec reporter chunks this loop is for, and `shard-manifest-<n>-of-<t>.json`, which
 *  `npm run shard-manifest` writes ONCE, BEFORE the suite step, to declare what this shard owes. It
 *  belongs to the JOB, not to an attempt. Archiving it into `attempt-<n>/` took it out of the flat
 *  `path: gui-smoke/.results/*.json` upload, so a retried shard uploaded its results but not its
 *  manifest — and the verdict job's join then reported `MISSING SHARD: shard N never reported a
 *  manifest. Its spec files did not run`, which is false twice over (it ran, twice, and it passed).
 *  A green shard turning the gui-smoke leg red with a wrong message, on the exact scenario this script
 *  exists to handle, is worse than the `SUITE DID NOT COMPLETE` it replaces.
 *
 *  Excluded by NAME through the same `SHARD_MANIFEST_PREFIX` `lib/resultsDir.ts:54` filters on, not by
 *  shape and not by a second literal — that module had this identical hazard and this identical answer
 *  one file over, and `resultsDir.test.ts` already carries a case for the co-location. */
function archiveAttempt(attempt: number): void {
  const dir = path.join(RESULTS_DIR, `attempt-${attempt}`);
  fs.mkdirSync(dir, { recursive: true });
  for (const file of fs.readdirSync(RESULTS_DIR)) {
    if (!file.endsWith(".json") || file.startsWith(SHARD_MANIFEST_PREFIX)) continue;
    fs.renameSync(path.join(RESULTS_DIR, file), path.join(dir, file));
  }
  if (fs.existsSync(SUITE_LOG)) {
    fs.copyFileSync(SUITE_LOG, path.join(RESULTS_DIR, `suite-output.attempt-${attempt}.log`));
  }
  log(`[gui-smoke session-retry] archived attempt ${attempt}'s results to ${dir} and its log alongside it.`);
}

/** How long to wait for the previous attempt's driver to release each fixed port. Generous because it is
 *  only ever paid when a listener really is still up — the common path is one refused connect, in
 *  single-digit milliseconds. */
const PORT_RELEASE_BUDGET_MS = 15_000;

/**
 * Between two attempts, wait until tauri-driver's two fixed ports are actually free again.
 *
 * `wdio.conf.ts`'s teardown is `killTauriDriver` → a bare, non-waiting `tauriDriver?.kill()`, and the
 * native WebDriver behind it is a grandchild nothing signals at all. So attempt 1's listeners can still
 * be up when this process's `close` fires, and attempt 2's `startTauriDriver` binds the SAME two ports.
 * Its readiness wait would then succeed against the dying listener while the real bind failed, and
 * `startTauriDriver`'s `exit` handler would take the worker down with `process.exit(1)` — the retry
 * failing for a reason that has nothing to do with what it was retrying.
 *
 * This is the cross-process twin of what `wdio.conf.ts#respawnTauriDriver` already does in-process with
 * `killAndWaitForExit`, and its comment names this exact race: *"racing that would have the readiness
 * wait below succeed against the DYING listener"*. A job-level retry cannot reach that child handle, so
 * it waits on the observable fact instead.
 *
 * Never fatal. A port that never frees is reported LOUDLY and the attempt proceeds — a bounded settle
 * guessing wrong must not be the thing that ends the run. `waitForPortFree` returns a boolean precisely so
 * "did not settle" cannot be mistaken for "settled".
 *
 * WHAT MAKES THAT SAFE IS NOT THE SAME FACT ON BOTH PORTS, and an earlier draft of this comment said it
 * was ("the attempt's own bind is the authoritative evidence"). Round 3 traced both:
 *
 *   * **4444 stale** — the new tauri-driver's OWN bind fails, it exits, and `startTauriDriver`'s `exit`
 *     handler calls `process.exit(1)`. Here the bind really is the authoritative evidence.
 *   * **4445 stale** — the new tauri-driver binds 4444 fine. The failing bind belongs to the GRANDCHILD
 *     `WebKitWebDriver`, which we never spawned and hold no handle or exit hook for, so nothing reports
 *     it; `startTauriDriver`'s second `waitForPort` then SUCCEEDS against attempt 1's dying listener.
 *     There is no authoritative bind to appeal to on this path — it is the exact failure CPE-1955's
 *     comment describes and the one this function exists to prevent.
 *
 * The conclusion survives, by a different route: on 4445 the authoritative evidence is **attempt 2 failing
 * with the same signature and the shard reding**. The budget stops the loop, the ratchet reds on an
 * incomplete run, and the WARNING below sits in the same log above it. So the cost of a settle that gave
 * up is a red shard with its cause printed — never a false green — which is what makes non-fatal the right
 * call, not an appeal to a bind that on this port nobody watches.
 */
async function settleDriverPorts(): Promise<void> {
  for (const { port, label } of DRIVER_PORTS) {
    const started = Date.now();
    const free = await waitForPortFree("127.0.0.1", port, PORT_RELEASE_BUDGET_MS);
    const took = Date.now() - started;
    if (free) {
      log(`[gui-smoke session-retry] port ${port} (${label}) is free after ${took} ms — safe to respawn.`);
    } else {
      log(
        `[gui-smoke session-retry] WARNING: port ${port} (${label}) was STILL accepting connections after ` +
          `${took} ms. Starting the next attempt anyway. If that attempt then fails, THIS line is the ` +
          "reason — a leftover listener from the previous attempt, not a new fault. On " +
          `${DRIVER_PORTS[0].port} the failure is loud (tauri-driver's own bind fails and it exits); on ` +
          `${DRIVER_PORTS[1].port} nothing reports the bind at all (the native driver is a grandchild we ` +
          "hold no handle to), so the evidence is the attempt dying with the same signature and the shard " +
          "reding on an incomplete run. Either way it reds — it never reads as a pass.",
      );
    }
  }
}

async function main(): Promise<void> {
  const budget = maxAttempts();
  const { expected, shardIndex } = expectedSpecFiles();
  const who = shardIndex === undefined ? "unsharded suite" : `shard ${shardIndex}`;
  log(
    `[gui-smoke session-retry] ${who}: up to ${budget} suite attempt(s); ${expected} spec file(s) expected ` +
      "to report. A retry happens ONLY when the WebDriver session dies before any assertion runs " +
      "(CPE-1910) — never on a real failure, and never as an exemption: the Ratchet step still gates.",
  );

  const attemptNotes: string[] = [];
  let driverRespawns = 0;
  let attempt = 0;
  let decision: RetryDecision;

  for (;;) {
    attempt += 1;
    log(`[gui-smoke session-retry] ---- suite attempt ${attempt} of ${budget} ----`);
    const suiteExit = await runSuiteOnce();

    // Read back what the attempt produced. A log we cannot read is NOT "no problem found" — it feeds
    // `suiteLogReadable: false`, which fails closed (no retry, loud reason).
    let logText: string | undefined;
    try {
      logText = fs.readFileSync(SUITE_LOG, "utf-8");
    } catch {
      logText = undefined;
    }
    // A results file that exists but will not parse throws out of here and fails the step, on purpose:
    // that is a corrupt artifact, and counting it as "that spec reported nothing" would read as a small
    // clean run to every clause downstream.
    const reported = reportedSpecFiles();
    const signature = classifyLog(logText ?? "");
    const respawnsThisAttempt = logText === undefined ? 0 : countDriverRespawns(logText);
    driverRespawns += respawnsThisAttempt;

    attemptNotes.push(
      `suite exited ${suiteExit}; ${reported}/${expected} spec file(s) reported; log-signature verdict ` +
        `\`${signature.verdict}\` (${signature.assertionErrorCount} AssertionError, ` +
        `${signature.environmentMarkerCount} environment marker(s)); ${respawnsThisAttempt} in-process ` +
        "driver respawn(s).",
    );
    log(`[gui-smoke session-retry] attempt ${attempt}: ${attemptNotes[attempt - 1]}`);

    decision = decideSuiteRetry({
      signature,
      reportedSpecFiles: reported,
      expectedSpecFiles: expected,
      attemptsUsed: attempt,
      maxAttempts: budget,
      suiteLogReadable: logText !== undefined,
    });
    log(`[gui-smoke session-retry] decision: ${decision.code} — ${decision.reason}`);

    if (!decision.retry) break;
    archiveAttempt(attempt);
    await settleDriverPorts();
  }

  // LOUD, and only when something actually recovered. `attempt > 1` covers a job-level retry;
  // `driverRespawns > 0` covers CPE-1955's in-process respawn, which until this ticket was invisible
  // outside ~14,000 lines of raw log even though 6 of 40 sampled shard-2 jobs used one. Reporting the
  // second is the larger half of "silent retries hide a worsening rate" (CPE-1893).
  if (attempt > 1 || driverRespawns > 0) {
    // `maxAttempts: budget` — the budget this run really used, NOT `MAX_SUITE_ATTEMPTS`. With
    // `GUI_SMOKE_MAX_ATTEMPTS=3` the constant would render "3 of 2 allowed".
    const summary = {
      shardIndex,
      attempts: attempt,
      driverRespawns,
      finalDecision: decision,
      attemptNotes,
      maxAttempts: budget,
    };
    for (const line of formatRetryLogLines(summary)) log(line);
    const summaryPath = process.env.GITHUB_STEP_SUMMARY;
    if (summaryPath) {
      fs.appendFileSync(summaryPath, `${formatRetrySummaryMarkdown(summary).join("\n")}\n\n`, "utf-8");
      log(`[gui-smoke session-retry] wrote the recovery block to the job summary (${summaryPath}).`);
    }
  } else {
    log("[gui-smoke session-retry] no session death and no driver respawn on this run — nothing to report.");
  }
}

main().catch((err: unknown) => {
  console.error(`[gui-smoke session-retry] FAILED TO RUN: ${err instanceof Error ? err.stack : String(err)}`);
  console.error(
    "[gui-smoke session-retry] This is the retry driver itself failing, NOT a test result. The step fails " +
      "rather than reporting a clean run — 'did not run' must never read as 'ran and found nothing'.",
  );
  process.exit(1);
});
