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
// Every path is overridable via env var so this can be pointed at a synthetic/saved report for local
// testing without a real tauri-driver run (see gui-smoke/README.md "Reading a CI run" / "Testing the
// ratchet locally"):
//   GUI_SMOKE_RESULTS_DIR    — dir of @wdio/json-reporter output files (default "./.results")
//   GUI_SMOKE_KNOWN_FAILING  — path to the known-failing list (default "./known-failing.json")
//   GUI_SMOKE_SPECS_DIR      — dir to glob *.smoke.ts in for expectedSpecCount (default "./specs")
import fs from "node:fs";
import path from "node:path";
import { caseKey, evaluate, type CaseResult, type CaseStatus, type KnownFailingFile } from "../lib/ratchet.js";

const RESULTS_DIR = process.env.GUI_SMOKE_RESULTS_DIR ?? path.resolve(process.cwd(), ".results");
const KNOWN_FAILING_PATH = process.env.GUI_SMOKE_KNOWN_FAILING ?? path.resolve(process.cwd(), "known-failing.json");
const SPECS_DIR = process.env.GUI_SMOKE_SPECS_DIR ?? path.resolve(process.cwd(), "specs");

/** Minimal shape this script reads out of one `@wdio/json-reporter` output file — a subset of the
 *  reporter's own `ResultSet` type (`specs: string[]`, `suites[].tests[]`, `suites[].hooks[]`). wdio
 *  spawns one worker per spec file here (no spec grouping in `wdio.conf.ts`), so `specs` is normally
 *  length 1; a longer array is still handled defensively by attributing the chunk's suites to every spec
 *  path it lists (the finest granularity the reporter's schema offers without grouping). */
interface RawResultChunk {
  specs: string[];
  suites?: {
    name?: string;
    tests?: { name?: string; state?: string }[];
    hooks?: { title?: string; state?: string; error?: unknown }[];
  }[];
}

/** The reporter's `state` strings, narrowed to what the ratchet reasons about. Anything unrecognised is
 *  treated as `skipped` — neither a pass (can't retire an exemption) nor a failure (can't red the job),
 *  but still proof the title exists. */
function toCaseStatus(state: string | undefined): CaseStatus {
  if (state === "passed" || state === "failed" || state === "skipped" || state === "pending") return state;
  return "skipped";
}

/** Reads every `*.json` file in `resultsDir` (one per spec-file worker) and reduces each into one
 *  `CaseResult` per test case, keyed by the spec file's basename + the `it()` title — the same key
 *  `known-failing.json` uses. Throws with a clear message if the directory is missing (never silently
 *  treats "no directory" as "zero results, fine" — that would defeat the incomplete-run guard).
 *
 *  A FAILING HOOK (`before`/`beforeEach`/`after`/`afterEach`) becomes a synthetic case named
 *  `<hook> "<title>"`, because a hook that throws usually means its suite's cases never ran at all: the
 *  cases would simply be absent, and "absent" must not read as green. The synthetic case is unlisted by
 *  construction, so it reds the job as a NEW GUI REGRESSION — and the listed cases it prevented from
 *  running additionally trip the STALE EXEMPTION clause. Both are the honest verdict for a dead suite. */
function loadCaseResults(resultsDir: string): CaseResult[] {
  if (!fs.existsSync(resultsDir)) {
    throw new Error(
      `[gui-smoke ratchet] results directory not found: ${resultsDir}\n` +
        "Run the suite first (it writes @wdio/json-reporter output there via wdio.conf.ts's `json` " +
        "reporter config), or point GUI_SMOKE_RESULTS_DIR at a saved/synthetic report directory.",
    );
  }

  const files = fs.readdirSync(resultsDir).filter((f) => f.endsWith(".json"));
  // Keyed so the defensive multi-spec path above can't double-count a case; a failure anywhere wins.
  const byKey = new Map<string, CaseResult>();

  const record = (spec: string, test: string, status: CaseStatus): void => {
    const key = caseKey(spec, test);
    const existing = byKey.get(key);
    if (!existing) byKey.set(key, { spec, test, status });
    else if (status === "failed") existing.status = "failed";
  };

  for (const file of files) {
    const raw = fs.readFileSync(path.join(resultsDir, file), "utf-8");
    let chunk: RawResultChunk;
    try {
      chunk = JSON.parse(raw) as RawResultChunk;
    } catch (err) {
      throw new Error(
        `[gui-smoke ratchet] failed to parse ${file} as JSON: ${err instanceof Error ? err.message : String(err)}`,
      );
    }

    for (const specPath of chunk.specs ?? []) {
      const spec = path.basename(specPath);
      for (const suite of chunk.suites ?? []) {
        for (const test of suite.tests ?? []) {
          if (!test.name) continue;
          record(spec, test.name, toCaseStatus(test.state));
        }
        for (const hook of suite.hooks ?? []) {
          if (!hook.error) continue;
          record(spec, `<hook> "${hook.title ?? "unnamed hook"}"`, "failed");
        }
      }
    }
  }

  return [...byKey.values()];
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
  const otherCount = results.length - passedCount - failedCount;
  // eslint-disable-next-line no-console
  console.log(
    `[gui-smoke ratchet] ${verdict.reportedSpecCount}/${expectedSpecCount} spec file(s) reported, ` +
      `${results.length} case(s) — ${passedCount} passed, ${failedCount} failed, ${otherCount} skipped/pending; ` +
      `${knownFailing.cases.length} known-failing case(s) listed.`,
  );

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
      `incomplete=${verdict.incomplete}.`,
  );
  process.exit(1);
}

main();
