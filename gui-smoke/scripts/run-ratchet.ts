// CPE-1594 / CPE-1677 — the I/O wrapper `npm run ratchet` actually invokes (mirrors
// `scripts/bless-demo-baselines.ts`'s "pure lib module + thin CLI wrapper" split). Reads the real
// WebdriverIO JSON reporter output + the committed known-failing list off disk, reduces them into the
// plain data `lib/ratchet.ts#evaluate` takes, prints a human-readable verdict, and exits non-zero on any
// red condition — this is the step `gui-smoke.yml`'s Linux leg runs AFTER the suite (whose own exit code
// is deliberately swallowed, see the workflow comment) so THIS script owns the job's pass/fail verdict.
//
// CPE-1677: the reduction is now per TEST CASE (`suites[].tests[]`), not per spec file. The old version
// collapsed a whole spec file to one pass/fail bit, which made every case inside an already-listed spec
// file unguarded — see `lib/ratchet.ts`'s header for the evidence.
//
// CPE-1680: the "pure lib module + thin CLI wrapper" split above wasn't actually honoured by the
// reduction itself — `toCaseStatus` (the raw-wdio-state-to-CaseStatus mapping) and the chunk-walking that
// used it lived entirely in THIS file, so `test:unit`'s `lib/**/*.test.ts` glob never collected a test
// for it; a reviewer mutation on `toCaseStatus`'s fallback (`"unknown"` -> `"skipped"`) left the whole
// suite green. That reduction now lives in `lib/ratchet.ts#reduceResultChunks`/`#toCaseStatus` as pure
// functions of already-parsed data; this file's job is now only finding the files, reading them, and
// `JSON.parse`ing them — genuinely thin.
//
// Every path is overridable via env var so this can be pointed at a synthetic/saved report for local
// testing without a real tauri-driver run (see gui-smoke/README.md "Reading a CI run" / "Testing the
// ratchet locally"):
//   GUI_SMOKE_RESULTS_DIR    — dir of @wdio/json-reporter output files (default "./.results")
//   GUI_SMOKE_KNOWN_FAILING  — path to the known-failing list (default "./known-failing.json")
//   GUI_SMOKE_SPECS_DIR      — dir to glob *.smoke.ts in for expectedSpecCount (default "./specs")
import fs from "node:fs";
import path from "node:path";
import {
  caseKey,
  evaluate,
  reduceResultChunks,
  type CaseResult,
  type KnownFailingFile,
  type RawResultChunk,
} from "../lib/ratchet.js";

const RESULTS_DIR = process.env.GUI_SMOKE_RESULTS_DIR ?? path.resolve(process.cwd(), ".results");
const KNOWN_FAILING_PATH = process.env.GUI_SMOKE_KNOWN_FAILING ?? path.resolve(process.cwd(), "known-failing.json");
const SPECS_DIR = process.env.GUI_SMOKE_SPECS_DIR ?? path.resolve(process.cwd(), "specs");

/** Reads every `*.json` file in `resultsDir` (one per spec-file worker) off disk, `JSON.parse`s each into
 *  a `RawResultChunk`, and hands the parsed chunks to `lib/ratchet.ts#reduceResultChunks` for the actual
 *  reduction into `CaseResult[]` (CPE-1680: that reduction — including `toCaseStatus`'s
 *  unknown-state-to-`"unknown"` mapping, finding #1's fix — used to live entirely in this file, where
 *  `test:unit`'s `lib/**\/*.test.ts` glob could never collect a test for it).
 *
 *  CPE-1728: a MISSING directory returns zero results (with a loud console note) instead of throwing.
 *  Before this, a run cancelled early enough that the suite never got to write even one spec's JSON
 *  (e.g. killed mid-`npm ci`/`tauri build`, or — the PR #900 case this ticket exists for — the whole job
 *  cancelled before this step ever started) meant `npm run ratchet` was either never invoked at all
 *  (skipped by the job's default step condition) or, if it was, threw a raw stack trace instead of the
 *  informative "SUITE DID NOT COMPLETE" verdict `evaluate()`'s clause 4 already produces for a run that
 *  DID start but didn't finish. Returning `[]` here feeds `evaluate()` the same "0 of N spec files
 *  reported" shape a partial run produces, so `gui-smoke.yml` wiring this step with `if: always()` now
 *  gives EVERY cancellation stage — before, during, or after the suite — one honest, uniform verdict
 *  instead of a bare GitHub "operation was canceled" the reader has to guess about. See
 *  `gui-smoke.yml`'s CPE-1728 comments and `lib/ratchet.ts`'s clause 4. Never silently treated as "zero
 *  results, fine, green" — `evaluate()`'s `incomplete` flag still fires and still reds the job. */
function loadCaseResults(resultsDir: string): CaseResult[] {
  if (!fs.existsSync(resultsDir)) {
    // eslint-disable-next-line no-console
    console.log(
      `[gui-smoke ratchet] no results directory at ${resultsDir} — the suite step likely never started or ` +
        "was killed before writing even one spec's @wdio/json-reporter output. Treating this as zero " +
        "cases reported (evaluate()'s incomplete-run clause will red the job below, honestly, instead of " +
        "this script throwing a raw error).",
    );
    return [];
  }

  const files = fs.readdirSync(resultsDir).filter((f) => f.endsWith(".json"));
  const chunks: RawResultChunk[] = [];
  for (const file of files) {
    const raw = fs.readFileSync(path.join(resultsDir, file), "utf-8");
    try {
      chunks.push(JSON.parse(raw) as RawResultChunk);
    } catch (err) {
      throw new Error(
        `[gui-smoke ratchet] failed to parse ${file} as JSON: ${err instanceof Error ? err.message : String(err)}`,
      );
    }
  }

  return reduceResultChunks(chunks);
}

function loadKnownFailing(knownFailingPath: string): KnownFailingFile {
  if (!fs.existsSync(knownFailingPath)) {
    throw new Error(`[gui-smoke ratchet] known-failing file not found: ${knownFailingPath}`);
  }
  const parsed = JSON.parse(fs.readFileSync(knownFailingPath, "utf-8")) as KnownFailingFile;
  if (!Array.isArray(parsed.cases)) {
    throw new Error(
      `[gui-smoke ratchet] ${knownFailingPath} has no "cases" array. Since CPE-1677 the ratchet is ` +
        `CASE-granular: the file lists individual { spec, test, reason, ticket } entries, not whole spec ` +
        `files. See gui-smoke/README.md "The ratchet".`,
    );
  }
  return parsed;
}

/** How many spec files SHOULD have run — globbed from disk (never hard-coded), so adding or removing a
 *  `specs/*.smoke.ts` file never needs a matching edit to this script. */
function countExpectedSpecs(specsDir: string): number {
  if (!fs.existsSync(specsDir)) {
    throw new Error(`[gui-smoke ratchet] specs directory not found: ${specsDir}`);
  }
  return fs.readdirSync(specsDir).filter((f) => f.endsWith(".smoke.ts")).length;
}

function main(): void {
  const results = loadCaseResults(RESULTS_DIR);
  const knownFailing = loadKnownFailing(KNOWN_FAILING_PATH);
  const expectedSpecCount = countExpectedSpecs(SPECS_DIR);

  const verdict = evaluate({ results, knownFailing, expectedSpecCount });

  const passedCount = results.filter((r) => r.status === "passed").length;
  const failedCount = results.filter((r) => r.status === "failed").length;
  const unknownCount = results.filter((r) => r.status === "unknown").length;
  const otherCount = results.length - passedCount - failedCount - unknownCount;
  // eslint-disable-next-line no-console
  console.log(
    `[gui-smoke ratchet] ${verdict.reportedSpecCount}/${expectedSpecCount} spec file(s) reported, ` +
      `${results.length} case(s) — ${passedCount} passed, ${failedCount} failed, ${otherCount} skipped/pending, ` +
      `${unknownCount} unrecognised state; ${knownFailing.cases.length} known-failing case(s) listed.`,
  );

  // CPE-1680: an unrecognised wdio state must never go quiet. Print it exactly like the failing-case
  // block below — loud AND red (verdict.ok already covers red; this covers loud) — every run it occurs,
  // not just when it happens to be the reason the job failed.
  if (verdict.unknownStates.length > 0) {
    // eslint-disable-next-line no-console
    console.log(`[gui-smoke ratchet] UNRECOGNISED-state case(s) observed this run (${verdict.unknownStates.length}):`);
    for (const key of verdict.unknownStates) console.log(`[gui-smoke ratchet]   ? ${key}`);
  }

  // Always print the per-case failing set, green or red. This is the log line CPE-1677 was filed over:
  // the OLD ratchet printed a spec-level tally that was byte-identical on a clean run and on a run with
  // a deliberately broken case inside an already-listed spec file. Printing the cases makes the flip
  // visible in the verdict itself, and makes migrating/retiring entries a copy-paste job.
  const failing = results
    .filter((r) => r.status === "failed")
    .map((r) => caseKey(r.spec, r.test))
    .sort((a, b) => a.localeCompare(b));
  if (failing.length > 0) {
    // eslint-disable-next-line no-console
    console.log(`[gui-smoke ratchet] failing case(s) observed this run (${failing.length}):`);
    for (const key of failing) console.log(`[gui-smoke ratchet]   ✖ ${key}`);
  }

  // Proven-flaky entries are exempt in BOTH directions, so they are the one thing here that could go
  // quiet. Print them, with what they actually did, on every single run — that is what keeps them
  // drainable instead of permanent.
  if (verdict.intermittentListings.length > 0) {
    // eslint-disable-next-line no-console
    console.log(
      `[gui-smoke ratchet] intermittent entr${verdict.intermittentListings.length === 1 ? "y" : "ies"} ` +
        `(${verdict.intermittentListings.length}) — exempt in both directions, still owed a fix:`,
    );
    for (const { key, statuses } of verdict.intermittentListings) {
      console.log(`[gui-smoke ratchet]   ~ ${statuses.join("/")}: ${key}`);
    }
  }

  if (verdict.ok) {
    // eslint-disable-next-line no-console
    console.log(
      "[gui-smoke ratchet] OK — every failing case is listed, every listed case still fails and still " +
        "exists, run completed.",
    );
    process.exit(0);
  }

  for (const message of verdict.messages) {
    console.error(`[gui-smoke ratchet] ${message}`);
  }
  console.error(
    `[gui-smoke ratchet] FAILED — ${verdict.newFailures.length} new failing case(s), ` +
      `${verdict.fixedButStillListed.length} now-passing entr${verdict.fixedButStillListed.length === 1 ? "y" : "ies"}, ` +
      `${verdict.unmatchedListings.length} stale entr${verdict.unmatchedListings.length === 1 ? "y" : "ies"} matching no test, ` +
      `${verdict.duplicateListings.length} duplicate entr${verdict.duplicateListings.length === 1 ? "y" : "ies"}, ` +
      `${verdict.unknownStates.length} unrecognised-state case(s), ` +
      `${verdict.unevidencedIntermittent.length} unevidenced intermittent entr${verdict.unevidencedIntermittent.length === 1 ? "y" : "ies"}, ` +
      `incomplete=${verdict.incomplete}.`,
  );
  process.exit(1);
}

main();
