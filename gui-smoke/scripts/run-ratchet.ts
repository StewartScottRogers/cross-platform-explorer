// CPE-1594 — the I/O wrapper `npm run ratchet` actually invokes (mirrors `scripts/bless-demo-baselines.ts`'s
// "pure lib module + thin CLI wrapper" split). Reads the real WebdriverIO JSON reporter output + the
// committed known-failing list off disk, reduces them into the plain data `lib/ratchet.ts#evaluate` takes,
// prints a human-readable verdict, and exits non-zero on any red condition — this is the step
// `gui-smoke.yml`'s Linux leg runs AFTER the suite (whose own exit code is deliberately swallowed, see the
// workflow comment) so THIS script owns the job's pass/fail verdict.
//
// Every path is overridable via env var so this can be pointed at a synthetic/saved report for local
// testing without a real tauri-driver run (see gui-smoke/README.md "Reading a CI run" / "Testing the
// ratchet locally"):
//   GUI_SMOKE_RESULTS_DIR    — dir of @wdio/json-reporter output files (default "./.results")
//   GUI_SMOKE_KNOWN_FAILING  — path to the known-failing list (default "./known-failing.json")
//   GUI_SMOKE_SPECS_DIR      — dir to glob *.smoke.ts in for expectedSpecCount (default "./specs")
import fs from "node:fs";
import path from "node:path";
import { evaluate, type KnownFailingFile, type SpecResult } from "../lib/ratchet.js";

const RESULTS_DIR = process.env.GUI_SMOKE_RESULTS_DIR ?? path.resolve(process.cwd(), ".results");
const KNOWN_FAILING_PATH = process.env.GUI_SMOKE_KNOWN_FAILING ?? path.resolve(process.cwd(), "known-failing.json");
const SPECS_DIR = process.env.GUI_SMOKE_SPECS_DIR ?? path.resolve(process.cwd(), "specs");

/** Minimal shape this script reads out of one `@wdio/json-reporter` output file — a superset of the
 *  fields we actually use (see the reporter's own `ResultSet` type: `specs: string[]`, `state.failed`).
 *  wdio spawns one worker per spec file here (no spec grouping in `wdio.conf.ts`), so `specs` is
 *  normally length 1; this still handles a longer array defensively by attributing the WHOLE chunk's
 *  pass/fail state to every spec path it lists (the finest granularity the reporter's schema offers
 *  without grouping). */
interface RawResultChunk {
  specs: string[];
  state: { passed: number; failed: number; skipped: number };
}

/** Reads every `*.json` file in `resultsDir` (one per spec-file worker) and reduces each into a
 *  `SpecResult` keyed by the spec file's basename — the same key `known-failing.json` uses. Throws with
 *  a clear message if the directory is missing (never silently treats "no directory" as "zero results,
 *  fine" — that would defeat the whole incomplete-run guard). */
function loadSpecResults(resultsDir: string): SpecResult[] {
  if (!fs.existsSync(resultsDir)) {
    throw new Error(
      `[gui-smoke ratchet] results directory not found: ${resultsDir}\n` +
        "Run the suite first (it writes @wdio/json-reporter output there via wdio.conf.ts's `json` " +
        "reporter config), or point GUI_SMOKE_RESULTS_DIR at a saved/synthetic report directory.",
    );
  }

  const files = fs.readdirSync(resultsDir).filter((f) => f.endsWith(".json"));
  const results: SpecResult[] = [];

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
    const status: SpecResult["status"] = chunk.state && chunk.state.failed > 0 ? "failed" : "passed";
    for (const specPath of chunk.specs ?? []) {
      results.push({ spec: path.basename(specPath), status });
    }
  }

  return results;
}

function loadKnownFailing(knownFailingPath: string): KnownFailingFile {
  if (!fs.existsSync(knownFailingPath)) {
    throw new Error(`[gui-smoke ratchet] known-failing file not found: ${knownFailingPath}`);
  }
  return JSON.parse(fs.readFileSync(knownFailingPath, "utf-8")) as KnownFailingFile;
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
  const results = loadSpecResults(RESULTS_DIR);
  const knownFailing = loadKnownFailing(KNOWN_FAILING_PATH);
  const expectedSpecCount = countExpectedSpecs(SPECS_DIR);

  const verdict = evaluate({ results, knownFailing, expectedSpecCount });

  const passedCount = results.filter((r) => r.status === "passed").length;
  const failedCount = results.filter((r) => r.status === "failed").length;
  // eslint-disable-next-line no-console
  console.log(
    `[gui-smoke ratchet] ${results.length}/${expectedSpecCount} spec(s) reported — ` +
      `${passedCount} passed, ${failedCount} failed, ${Object.keys(knownFailing.specs ?? {}).length} known-failing listed.`,
  );

  if (verdict.ok) {
    // eslint-disable-next-line no-console
    console.log("[gui-smoke ratchet] OK — no new regressions, no stale known-failing entries, run completed.");
    process.exit(0);
  }

  for (const message of verdict.messages) {
    console.error(`[gui-smoke ratchet] ${message}`);
  }
  console.error(
    `[gui-smoke ratchet] FAILED — ${verdict.newFailures.length} new failure(s), ` +
      `${verdict.fixedButStillListed.length} stale known-failing entr${verdict.fixedButStillListed.length === 1 ? "y" : "ies"}, ` +
      `incomplete=${verdict.incomplete}.`,
  );
  process.exit(1);
}

main();
