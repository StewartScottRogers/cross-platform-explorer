// CPE-1910 — the retry policy's tests. Every "should retry" / "must not retry" case below is a SHAPE
// TAKEN FROM A REAL JOB in the 2026-08-28 enumeration (job ids named per test), not an invented one:
// the whole risk in this feature is retrying something that deserved to stay red, and the two
// populations that make that easy to get wrong were both live in CI while this was written.
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, it } from "node:test";

import { classifyLog, type LogSignatureResult } from "./logSignature.js";
import {
  countDriverRespawns,
  decideSuiteRetry,
  formatRetryLogLines,
  formatRetrySummaryMarkdown,
  MAX_SUITE_ATTEMPTS,
  RESPAWN_LOG_MARKER,
  type RetryDecisionInput,
} from "./sessionRetry.js";

const HERE = path.dirname(fileURLToPath(import.meta.url));

/** The tail of a real socket death — job 98646323315, shard 2, 2026-08-27T19:44Z. tauri-driver survives
 *  and logs the refusal; the native driver behind it is gone; wdio sees UND_ERR_SOCKET and the worker
 *  dies with 1 of 14 spec files reported. Contains NO `AssertionError`. */
const SOCKET_DEATH_LOG = [
  '[0-0] ERROR webdriver: WebDriverError: Request failed with error code UND_ERR_SOCKET when running "http://127.0.0.1:4444/session" with method "POST"',
  "[0-0] Error serving connection: hyper::Error(User(Service), client error (Connect)",
  "[0-0] Caused by:",
  "[0-0]     0: tcp connect error",
  "[0-0]     1: Connection refused (os error 111))",
  "[0-0] no such element",
  "[0-0] Could not get DRI3 device",
  "[0-0] ERROR @wdio/local-runner: Failed launching test session: Error: WebDriverError: ... UND_ERR_SOCKET ...",
].join("\n");

/** A real regression that classifies as environment-signature-only — job 98661503323, shard 2. The
 *  failing case is `macro-param-prompt.smoke.ts :: running a bound {ask:suffix} macro …`; it failed on an
 *  expect-webdriverio wait, which throws a plain `Error`, so no `AssertionError` string exists anywhere.
 *  24 of the 27 shard-2 failures in the sample window looked exactly like this. THE RETRY MUST NOT FIRE. */
const REAL_REGRESSION_ENV_VERDICT_LOG = [
  "[0-0] no such element",
  "[0-0] Could not get DRI3 device",
  "[0-0] could not inhibit screen lock",
  '[0-0] FAILED macro-param-prompt.smoke.ts :: running a bound {ask:suffix} macro opens MacroParamPrompt before any dry-run confirm',
  "[0-0] Error: Expected the MacroParamPrompt dialog to be displayed after 10000ms",
].join("\n");

/** A genuine chai assertion failure — job 98713492478, shard 4 (verdict MIXED). */
const ASSERTION_FAILURE_LOG = [
  "[0-0] no such element",
  "[0-0] AssertionError: expected 0 to equal 3",
  "[0-0] AssertionError: expected '' to include 'CPE-1045-marker.txt'",
].join("\n");

function decision(over: Partial<RetryDecisionInput> & { signature: LogSignatureResult }): ReturnType<
  typeof decideSuiteRetry
> {
  return decideSuiteRetry({
    reportedSpecFiles: 1,
    expectedSpecFiles: 14,
    attemptsUsed: 1,
    maxAttempts: MAX_SUITE_ATTEMPTS,
    suiteLogReadable: true,
    ...over,
  });
}

describe("decideSuiteRetry — the socket death it exists for", () => {
  it("retries when the session died before asserting (job 98646323315's shape)", () => {
    const signature = classifyLog(SOCKET_DEATH_LOG);
    assert.equal(signature.verdict, "environment-signature-only");
    const d = decision({ signature });
    assert.equal(d.retry, true);
    assert.equal(d.code, "retry-session-died-before-asserting");
    assert.match(d.reason, /1 of 14 assigned spec file/);
  });

  it("does not retry twice — the budget is spent and the shard stays red", () => {
    const d = decision({ signature: classifyLog(SOCKET_DEATH_LOG), attemptsUsed: 2 });
    assert.equal(d.retry, false);
    assert.equal(d.code, "budget-spent");
    assert.match(d.reason, /reportable defect/);
  });
});

describe("decideSuiteRetry — what it must NEVER retry", () => {
  // The red-proof's unit half. `/code-review`-style reasoning is not enough here: this is the case that
  // would quietly launder a real regression, so it is pinned against the exact log shape that produced
  // 24 of 27 shard-2 failures on the day this was written.
  it("does NOT retry a real regression that classifies as environment-signature-only (job 98661503323)", () => {
    const signature = classifyLog(REAL_REGRESSION_ENV_VERDICT_LOG);
    assert.equal(
      signature.verdict,
      "environment-signature-only",
      "precondition: this genuinely-failing run carries the environment verdict, which is exactly why the " +
        "verdict alone cannot be the retry trigger",
    );
    const d = decision({ signature, reportedSpecFiles: 14, expectedSpecFiles: 14 });
    assert.equal(d.retry, false);
    assert.equal(d.code, "suite-completed");
  });

  it("does NOT retry a genuine AssertionError (job 98713492478)", () => {
    const signature = classifyLog(ASSERTION_FAILURE_LOG);
    assert.equal(d0(signature).retry, false);
    assert.equal(d0(signature).code, "assertion-evidence-present");
    function d0(s: LogSignatureResult): ReturnType<typeof decideSuiteRetry> {
      // Incomplete AND carrying an assertion: the completeness condition would have said "retry", so this
      // proves the assertion condition is doing real work rather than being shadowed by it.
      return decision({ signature: s, reportedSpecFiles: 1, expectedSpecFiles: 14 });
    }
  });

  it("does NOT retry CPE-1960's shape — a scrollIntoView regression that failed a real check", () => {
    // CPE-1960 wheeled a scroll into an open menu; ~90% of runs, and an ASSERTION failure. A retry here
    // would have turned a 90% signal into a 99% one and hidden a real, root-causable defect.
    const signature = classifyLog(
      ["[0-0] AssertionError: expected the context menu to still be open", "[0-0] no such element"].join("\n"),
    );
    const d = decision({ signature, reportedSpecFiles: 3, expectedSpecFiles: 14 });
    assert.equal(d.retry, false);
    assert.equal(d.code, "assertion-evidence-present");
  });

  it("does NOT retry a clean, complete run", () => {
    const d = decision({ signature: classifyLog("all good"), reportedSpecFiles: 14, expectedSpecFiles: 14 });
    assert.equal(d.retry, false);
    assert.equal(d.code, "no-environment-signature");
  });

  it("does NOT retry an INCOMPLETE run with no environment signature at all", () => {
    // The dangerous near-miss: few spec files reported, but nothing in the log says the transport died.
    // A shard killed by a genuine app crash looks like this, and re-running it is the "hide the race"
    // move CPE-1679 refused. Proves the environment-signature condition is doing real work and is not
    // shadowed by the completeness condition.
    const d = decision({ signature: classifyLog("worker exited"), reportedSpecFiles: 1, expectedSpecFiles: 14 });
    assert.equal(d.retry, false);
    assert.equal(d.code, "no-environment-signature");
  });
});

describe("decideSuiteRetry — fails closed on missing evidence", () => {
  it("refuses to retry when the captured log could not be read, and says the classifier did not run", () => {
    const d = decision({ signature: classifyLog(""), suiteLogReadable: false });
    assert.equal(d.retry, false);
    assert.equal(d.code, "no-log-cannot-classify");
    assert.match(d.reason, /'did not run', not 'ran and found nothing'/);
  });

  it("an empty log on its own is 'no-signal', never a retry", () => {
    // Distinct from the case above: here the log WAS readable and simply held nothing. `classifyLog`
    // returns `no-signal`, which is not `environment-signature-only`, so no retry — without this, an
    // empty log would look like "no AssertionError anywhere" and retry forever.
    const signature = classifyLog("");
    assert.equal(signature.verdict, "no-signal");
    assert.equal(decision({ signature }).retry, false);
  });
});

describe("countDriverRespawns", () => {
  it("counts each CPE-1955 in-process respawn", () => {
    const text = [
      "[0-0] [gui-smoke] the WebDriver transport looks gone — respawning tauri-driver (respawn 1 of 1, CPE-1955)",
      "[0-0] something else",
      "[0-0] [gui-smoke] the WebDriver transport looks gone — respawning tauri-driver (respawn 1 of 1, CPE-1955)",
    ].join("\n");
    assert.equal(countDriverRespawns(text), 2);
    assert.equal(countDriverRespawns("nothing here"), 0);
  });

  // CPE-1933: DERIVED, not claimed. `RESPAWN_LOG_MARKER` asserts a fact about a DIFFERENT file, so this
  // reads that file at run time instead of trusting a comment. Red-proofed by editing the message in
  // `wdio.conf.ts` and watching this fail (result recorded in CPE-1910's Work Log). Without it, a
  // reworded respawn message would silently report 0 respawns forever — and a silent recovery is the
  // exact defect this ticket's summary block exists to stop.
  it("the marker it counts is really the string wdio.conf.ts prints", () => {
    const conf = fs.readFileSync(path.join(HERE, "..", "wdio.conf.ts"), "utf-8");
    assert.ok(
      conf.includes(RESPAWN_LOG_MARKER),
      `wdio.conf.ts no longer contains ${JSON.stringify(RESPAWN_LOG_MARKER)}. If the respawn message was ` +
        "reworded, update RESPAWN_LOG_MARKER in lib/sessionRetry.ts to match — otherwise every job " +
        "summary silently reports 0 in-process driver respawns.",
    );
  });
});

describe("the loud block", () => {
  const summary = {
    shardIndex: 2,
    attempts: 2,
    driverRespawns: 1,
    finalDecision: decision({ signature: classifyLog("clean"), reportedSpecFiles: 14, expectedSpecFiles: 14 }),
    attemptNotes: ["attempt one died", "attempt two completed"],
  };

  it("names both counts in the markdown, so neither recovery can go quiet", () => {
    const md = formatRetrySummaryMarkdown(summary).join("\n");
    assert.match(md, /job-level suite retries used \| \*\*1\*\*/);
    assert.match(md, /in-process tauri-driver respawns \(CPE-1955\) \| \*\*1\*\*/);
    assert.match(md, /shard 2/);
    assert.match(md, /Ratchet step still owns this shard's pass\/fail verdict/);
  });

  it("names both counts in the plain job log too", () => {
    const text = formatRetryLogLines(summary).join("\n");
    assert.match(text, /job-level suite retries used \.+ 1/);
    assert.match(text, /in-process driver respawns \.+ 1/);
  });
});
