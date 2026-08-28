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
// CPE-1753: the suite now runs SHARDED across parallel jobs, so this script has three modes — chosen by
// env vars, because a CI step cannot pass flags through `npm run` without quoting games:
//
//   (unsharded)  no shard env vars. Exactly the pre-CPE-1753 behaviour: every spec file expected, the
//                whole known-failing.json in scope. What a local `npm run ratchet` still does.
//   SHARD        GUI_SMOKE_SHARD_INDEX + GUI_SMOKE_SHARD_TOTAL. Verifies THIS shard's own subset: the
//                expectation is the shard's assigned spec files, and known-failing.json is narrowed to
//                entries whose `spec` this shard owns. Fast, local feedback that reds the shard job.
//                Deliberately CANNOT see the whole picture — an entry naming a spec in another shard, or
//                naming no spec file at all, is out of its scope, and it says so in the log rather than
//                pretending. The join below is what checks those.
//   JOIN         GUI_SMOKE_EXPECT_SHARDS. Runs in `gui-smoke-linux-verdict` over every shard's merged
//                `.results/`. This is the authoritative gate: the FULL known-failing.json (so the stale-
//                exemption clause finally sees every entry against every shard's results — it can only
//                run here, since a global exemption list cannot be judged from one shard), the FULL
//                live-globbed spec count, and — the thing that makes sharding safe — an assertion that
//                every shard index 1..GUI_SMOKE_EXPECT_SHARDS actually reported a manifest. A missing
//                shard is RED, never a smaller expectation. See lib/shard.ts's header.
//
// Both the shard mode and the join mode gate. Neither can mask the other: a shard red fails its own job,
// a join red fails the verdict job, and the workflow run fails if either does.
//
// Every path is overridable via env var so this can be pointed at a synthetic/saved report for local
// testing without a real tauri-driver run (see gui-smoke/README.md "Reading a CI run" / "Testing the
// ratchet locally"):
//   GUI_SMOKE_RESULTS_DIR    — dir of @wdio/json-reporter output files (default "./.results")
//   GUI_SMOKE_KNOWN_FAILING  — path to the known-failing list (default "./known-failing.json")
//   GUI_SMOKE_SPECS_DIR      — dir to glob *.smoke.ts in for expectedSpecCount (default "./specs")
//   GUI_SMOKE_SHARD_INDEX    — 1-based shard number (shard mode; see lib/shard.ts)
//   GUI_SMOKE_SHARD_TOTAL    — how many shards the suite is split into (shard mode)
//   GUI_SMOKE_EXPECT_SHARDS  — how many shard manifests the join must find (join mode)
import fs from "node:fs";
import path from "node:path";
import {
  caseKey,
  evaluate,
  reduceResultChunks,
  type CaseResult,
  type KnownFailingFile,
  type ShardCoverageInput,
} from "../lib/ratchet.js";
import { readResultChunks } from "../lib/resultsDir.js";
import {
  assignShardSpecs,
  isShardManifest,
  parseExpectedShards,
  parseShardId,
  SHARD_MANIFEST_PREFIX,
  type ShardId,
  type ShardManifest,
} from "../lib/shard.js";
import { listSpecFiles } from "../lib/specFiles.js";

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
  // CPE-1910: the readdir/manifest-filter/JSON.parse half now lives in `lib/resultsDir.ts`, shared with
  // `scripts/run-suite.ts`'s retry decision — which has to answer this script's own "how many spec files
  // reported?" question BEFORE this script runs. One implementation, two callers, and (because it is now
  // under `lib/`) unit-tested by `test:unit` for the first time.
  const { chunks } = readResultChunks(resultsDir);
  if (chunks === undefined) {
    // eslint-disable-next-line no-console
    console.log(
      `[gui-smoke ratchet] no results directory at ${resultsDir} — the suite step likely never started or ` +
        "was killed before writing even one spec's @wdio/json-reporter output. Treating this as zero " +
        "cases reported (evaluate()'s incomplete-run clause will red the job below, honestly, instead of " +
        "this script throwing a raw error).",
    );
    return [];
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

/** CPE-1753 — reads every `shard-manifest-*.json` the shards uploaded, for the JOIN mode's clause-9 check.
 *
 *  A file that matches the name but not the SHAPE is not silently skipped and not coerced: it is reported
 *  loudly and left OUT of the manifest list, so the shard it belonged to shows up as MISSING rather than
 *  as "reported, contents unreadable". Any other treatment (guessing the index from the filename, say)
 *  would let a truncated artifact count as a shard that ran.
 *
 *  Returns the manifests plus the names of anything rejected, so `main` can print both. */
function loadShardManifests(resultsDir: string): { manifests: ShardManifest[]; rejected: string[] } {
  if (!fs.existsSync(resultsDir)) return { manifests: [], rejected: [] };
  const manifests: ShardManifest[] = [];
  const rejected: string[] = [];
  for (const file of fs.readdirSync(resultsDir).filter((f) => f.startsWith(SHARD_MANIFEST_PREFIX))) {
    const full = path.join(resultsDir, file);
    try {
      const parsed: unknown = JSON.parse(fs.readFileSync(full, "utf-8"));
      if (isShardManifest(parsed)) manifests.push(parsed);
      else rejected.push(file);
    } catch {
      rejected.push(file);
    }
  }
  return { manifests, rejected };
}

/** Narrows `known-failing.json` to the entries a single shard can legitimately judge — those naming a
 *  spec file THIS shard ran. Everything else is out of its scope by construction: an entry for another
 *  shard's spec would look like a STALE EXEMPTION here (its case never ran in this job), which would red
 *  every shard for a perfectly healthy list.
 *
 *  The dropped entries are RETURNED, not swallowed, and `main` prints the count and the reason. Silent
 *  narrowing is how a bound turns into a blind spot: entries naming a spec file that no longer exists at
 *  all belong to NO shard and would be checked by nobody if the join did not carry the full list. */
function scopeKnownFailingToShard(
  knownFailing: KnownFailingFile,
  shardSpecs: string[],
): { scoped: KnownFailingFile; deferredToJoin: number } {
  const owned = new Set(shardSpecs);
  const cases = (knownFailing.cases ?? []).filter((c) => owned.has(c.spec));
  return {
    scoped: { ...knownFailing, cases },
    deferredToJoin: (knownFailing.cases ?? []).length - cases.length,
  };
}

/** Printed on EVERY shard run, green or red. Two things a reader cannot otherwise get: which spec files
 *  this shard actually owned (the partition is deterministic but not obvious by eye), and — the standing
 *  lesson about bounded checks — exactly how many known-failing entries this verdict did NOT look at, so
 *  a narrowed check never reads as "covered everything". */
function printShardScope(shardId: ShardId, shardSpecs: string[], totalSpecs: number, deferredToJoin: number): void {
  // eslint-disable-next-line no-console
  console.log(
    `[gui-smoke ratchet] SHARD MODE — shard ${shardId.shardIndex} of ${shardId.shardTotal}: verifying ` +
      `${shardSpecs.length} of ${totalSpecs} spec file(s) [${shardSpecs.join(", ")}].`,
  );
  // eslint-disable-next-line no-console
  console.log(
    `[gui-smoke ratchet] SHARD MODE — ${deferredToJoin} known-failing entr${deferredToJoin === 1 ? "y" : "ies"} ` +
      `name spec files this shard did NOT run and are NOT checked here (including any entry naming a spec ` +
      `file that no longer exists, which belongs to no shard at all). They are checked by the ` +
      `gui-smoke-linux-verdict job over every shard's merged results — a global exemption list cannot be ` +
      `judged from one shard.`,
  );
}

/** Printed on EVERY join run. The roster is the evidence for "every shard reported": a reader should be
 *  able to see 1,2,3,4 of 4 in the log without reconstructing it from which jobs happen to be green. */
function printJoinScope(expectedShardTotal: number, manifests: ShardManifest[], rejected: string[]): void {
  const seen = manifests.map((m) => m.shardIndex).sort((a, b) => a - b);
  // eslint-disable-next-line no-console
  console.log(
    `[gui-smoke ratchet] JOIN MODE — expecting ${expectedShardTotal} shard(s) per ` +
      `GUI_SMOKE_EXPECT_SHARDS (the workflow file, NOT the downloaded artifacts); manifests received from ` +
      `shard(s): ${seen.length > 0 ? seen.join(", ") : "(none)"}.`,
  );
  for (const rejectedFile of rejected.sort()) {
    console.error(
      `[gui-smoke ratchet] JOIN MODE — ${rejectedFile} matched the shard-manifest name but is not a valid ` +
        `manifest (truncated download or hand-edited). NOT counted as a shard that reported — its index ` +
        `will show up below as a MISSING SHARD, which is the honest reading.`,
    );
  }
}

function main(): void {
  const shardId = parseShardId(process.env);
  const expectedShardTotal = parseExpectedShards(process.env);

  const results = loadCaseResults(RESULTS_DIR);
  const fullKnownFailing = loadKnownFailing(KNOWN_FAILING_PATH);
  const allSpecs = listSpecFiles(SPECS_DIR);

  let knownFailing = fullKnownFailing;
  let expectedSpecCount = allSpecs.length;
  let shards: ShardCoverageInput | undefined;

  if (shardId) {
    const shardSpecs = assignShardSpecs(allSpecs, shardId);
    if (shardSpecs.length === 0) {
      // Refused here rather than left to `evaluate()`'s `expectedSpecCount < 1` clause, which would print
      // a true but baffling "INVALID EXPECTED SPEC COUNT" for what is really a matrix that is too wide.
      console.error(
        `[gui-smoke ratchet] shard ${shardId.shardIndex} of ${shardId.shardTotal} was assigned ZERO spec ` +
          `files: the matrix has more shards than specs/ has spec files (${allSpecs.length}). A shard that ` +
          `runs nothing must never report a happy "0 of 0 reported" — shrink the matrix in ` +
          `.github/workflows/gui-smoke.yml and GUI_SMOKE_EXPECT_SHARDS together.`,
      );
      process.exit(1);
    }
    const scoping = scopeKnownFailingToShard(fullKnownFailing, shardSpecs);
    knownFailing = scoping.scoped;
    expectedSpecCount = shardSpecs.length;
    printShardScope(shardId, shardSpecs, allSpecs.length, scoping.deferredToJoin);
  } else if (expectedShardTotal !== undefined) {
    const { manifests, rejected } = loadShardManifests(RESULTS_DIR);
    shards = { expectedShardTotal, manifests, expectedSpecs: allSpecs };
    printJoinScope(expectedShardTotal, manifests, rejected);
  }

  const verdict = evaluate({ results, knownFailing, expectedSpecCount, shards });

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
      `${verdict.missingShards.length} missing shard(s)${verdict.missingShards.length > 0 ? ` [${verdict.missingShards.join(", ")}]` : ""}, ` +
      `${verdict.shardPlanProblems.length} shard-plan problem(s), ` +
      `incomplete=${verdict.incomplete}.`,
  );
  process.exit(1);
}

main();
