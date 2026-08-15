// CPE-1594 / CPE-1677 — the gui-smoke Linux leg's blocking gate. Pure, no I/O (mirrors the
// `compare.ts`/`compare.test.ts` split — see `scripts/run-ratchet.ts` for the filesystem-aware wrapper
// a CI step actually runs).
//
// The problem CPE-1594 fixed: `gui-smoke.yml`'s Linux leg was `continue-on-error: true` — it produced a
// real, readable "33 passed, 7 failed" result every run, but nothing ever turned that into a pass/fail
// verdict, so it never gated anything and the failing tail grew silently. This module is the missing
// verdict function: given the suite's own JSON results, the committed list of cases that are ALLOWED to
// fail today, and how many spec files SHOULD have run, decide whether the run is green.
//
// The problem CPE-1677 fixed: the first version of this module worked at **spec-file** granularity, so
// `known-failing.json` exempted whole files. `samples.smoke.ts` has 46 cases and was listed for 22 of
// them (CPE-1507's preview-settle tail) — which meant the other 24 guarded NOTHING. The CPE-1639 worker
// proved it: they deliberately broke the font-preview case inside that file, ran the real job, and the
// baseline run and the broken run printed the byte-identical verdict ("38 passed, 3 failed, 3
// known-failing listed — OK"). Both runs passed. Only the raw per-test log showed the flip. So the
// ratchet now works at **case** granularity: the unit of exemption is one `it()` inside one spec file,
// identified by `spec` + `test` title (see `caseKey`).
//
// EIGHT distinct failure modes, each with its own message so a future engineer knows exactly what to do
// without re-deriving the ratchet's rules from scratch:
//   1. NEW GUI REGRESSION      — a CASE failed that isn't in known-failing.json. This is the clause that
//      makes a regression inside a partially-failing spec file visible at the gate (CPE-1677).
//   2. RATCHET (one-way)       — a case in known-failing.json now PASSES. Per the QA charter a surface
//      may leave the failing column exactly once; leaving a passing case listed hides a future
//      regression on it (it would silently rejoin known-failing instead of going red), and the QA
//      burndown depends on the list DRAINING rather than accumulating.
//   3. STALE EXEMPTION         — a listed case matched NO test in this run. Test titles are strings and
//      they drift; without this clause, renaming a test would silently lose both its exemption and its
//      coverage in one move (the rename reads as "case gone" = nothing to check), and a spec file that
//      dies before reporting its cases would read as green. A listed title that matches nothing is
//      always RED — either restore the title, retitle the entry, or delete it.
//   4. SUITE DID NOT COMPLETE  — fewer SPEC FILES reported a result than the suite was supposed to run.
//      This is the SPECIFIC guard against the failure mode CPE-1594 exists to fix: a timeout/crash/hang
//      that kills the job partway through must never be indistinguishable from "everything after the
//      crash happened to pass" (see CPE-1594's evidence: 796 straight `cancelled` runs). Kept at spec
//      granularity deliberately — there is no committed expected-CASE count to compare against, and
//      clause 3 already catches a truncation that swallows a listed case.
//   5. DUPLICATE EXEMPTION     — the same `spec` + `test` is listed twice in known-failing.json. Not a
//      product signal; a list-hygiene one, caught here because a duplicate makes the "delete its entry"
//      instruction in clauses 2/3 silently insufficient.
//   6. UNRECOGNISED TEST STATE — a case reported a wdio `state` this ratchet has never seen (CPE-1680).
//      The reduction in `run-ratchet.ts#toCaseStatus` used to default anything unrecognised to
//      `"skipped"` — but a skipped case is exempt from EVERY clause above, so an unknown state (a new
//      wdio version, a new runner mode, a state produced by a crash path) silently reclassified a
//      possibly-FAILING case as one that never ran, and the gate stayed green. The same shape as the bug
//      CPE-1677 fixed: not a wrong answer, a confident green where the truth is unknown. This clause
//      reds the run unconditionally — see the comment on `"unknown"` below for why RED was chosen over
//      "warn but stay green".
//   7. UNEVIDENCED INTERMITTENT — an `intermittent: true` entry whose `reason`/`ticket` don't clear a
//      minimum evidence bar (CPE-1680). Proven by the UAT probing `evaluate()` directly: an entry with an
//      EMPTY `reason` and EMPTY `ticket` was accepted just as readily as the four real, well-evidenced
//      entries — silencing a case that fails on EVERY run, forever, with nothing distinguishing it from a
//      permanent break (which `intermittent` is explicitly not for). This cannot prove a case is actually
//      flaky — only real run history can, and it should not pretend to — but it can and does refuse the
//      one thing that's structurally checkable: no evidence attached at all.
//   8. INVALID EXPECTED SPEC COUNT — `expectedSpecCount < 1` (CPE-1728). Closes a vacuous-green hole a
//      reviewer found while auditing this ticket's other change: with zero results, an empty
//      known-failing.json, AND `expectedSpecCount === 0`, clause 4's own `reportedSpecCount <
//      expectedSpecCount` compares `0 < 0` (false) — nothing else fires, and `evaluate()` returns green
//      for a run that verified NOTHING. Not reachable through the real CI path today (the specs
//      directory is checked out from git and never empty), but this ticket's OWN change
//      (`run-ratchet.ts#loadCaseResults` tolerating a missing `.results/` dir instead of throwing) widens
//      who can reach `evaluate()` with a bad `expectedSpecCount`, so this closes the hole permanently
//      rather than leaving it merely unreachable today.
//   9. MISSING SHARD / SHARD PLAN PROBLEM — (CPE-1753) the suite now runs split across N parallel jobs,
//      so the verdict is assembled from N separate `.results/` directories. A shard that never reported
//      at all — cancelled, crashed, never scheduled — must be RED, and specifically must NOT be allowed
//      to lower the bar it is measured against. Two independent mechanisms enforce that, and they fail in
//      different ways so neither is a single point of trust: (a) `expectedSpecCount` is still globbed live
//      from the checked-out `specs/` directory, never derived from the downloaded artifacts, so a missing
//      shard's spec files simply stop reporting and clause 4 fires; and (b) the `shards` input below
//      carries an `expectedShardTotal` that comes from the WORKFLOW FILE, checked against the set of shard
//      manifests actually received. The naive aggregation — "count the manifests you found, expect that
//      many" — is exactly the silently-passing shape CPE-1728 exists to eliminate, which is why the
//      expectation is passed IN rather than inferred here. The same clause also reds a duplicated or
//      out-of-range shard index, a manifest whose own `shardTotal` disagrees with the expectation (the
//      shape shard-count drift takes: matrix bumped but the verdict job's literal not, or vice versa), and
//      a shard plan that does not cover every live spec file exactly once. Omitted (`shards` undefined)
//      for an unsharded run and for each shard's own local verdict — see `lib/shard.ts`'s header.
//
// One narrow escape hatch, `intermittent: true` on an entry (CPE-1677, forced by the evidence): case
// granularity turns a genuinely FLAKY case into a coin-flip gate — clause 1 reds the runs where it
// fails, clause 2 reds the runs where it passes, and the job is red either way through no fault of the
// change under test. The `samples/audio/*` cases are exactly that today: across six real runs
// (31593963928, 31598207125, 31602466829, 31617196015, 31621443412, 31622660088 attempts 1+2) each of
// `track.flac`, `track.mp3` and `track.ogg` has been seen both passing and failing on unchanged code —
// invisible until now precisely because the whole `samples.smoke.ts` file was exempt. An intermittent
// entry is exempt in BOTH directions but must still exist (clause 3), and `run-ratchet.ts` prints every
// one with its observed status on every run so it can't go quiet. It is not a way to silence a test that
// fails every time — that is a plain entry.

import { compareSpecNames, type ShardManifest } from "./shard.js";

/** A case's outcome as the JSON reporter records it. `skipped`/`pending` are neither a pass nor a
 *  failure: they can't red the job (clause 1) and can't retire an exemption (clause 2), but they DO
 *  count as "this title still exists" for clause 3.
 *
 *  `unknown` (CPE-1680) is the reduction's honest answer when wdio reports a `state` this ratchet has
 *  never seen — deliberately its OWN value, not a synonym for `skipped`. Folding an unrecognised state
 *  into `skipped` would exempt it from every clause above, so a state produced by (say) a new wdio
 *  version or a crash path could hide a genuine failure behind a green gate. `unknown` still counts as
 *  "this title exists" for clause 3, exactly like `skipped`/`pending` — but unlike them it is ALSO its
 *  own failure mode (clause 6) that unconditionally reds the run, listed or not: an "I don't know"
 *  about a test result must never be silently treated as "safe to ignore". */
export type CaseStatus = "passed" | "failed" | "skipped" | "pending" | "unknown";

/** One test case's outcome, already reduced from the raw WebdriverIO JSON report (see
 *  `scripts/run-ratchet.ts#loadCaseResults` for how that reduction happens). `spec` is the spec file's
 *  basename (e.g. `"samples.smoke.ts"`), `test` is the `it()` title exactly as the reporter recorded it
 *  (`suites[].tests[].name`) — together they're the key `known-failing.json` uses. */
export interface CaseResult {
  spec: string;
  test: string;
  status: CaseStatus;
}

/** One entry in `known-failing.json`: a single case that is allowed to fail today, with why and who
 *  owns getting it back. NOT a pattern, glob, or prefix — deliberately an exact title, because an
 *  exemption that can match "whatever else shows up" is the whole-file exemption CPE-1677 removed. */
export interface KnownFailingCase {
  spec: string;
  test: string;
  reason: string;
  ticket: string;
  /** `true` for a case PROVEN intermittent across real runs (cite the run ids in `reason`): it may pass
   *  or fail without redding the job, and passing does NOT retire it (clause 2 is skipped) — otherwise a
   *  coin-flip case would red every other run in one direction or the other. It must still EXIST
   *  (clause 3 applies), and `run-ratchet.ts` prints every intermittent entry with its observed status
   *  on every run, so these stay visible and drainable rather than becoming quiet permanent holes.
   *
   *  Deliberately narrow. This is NOT "the test is annoying" — it is "this exact case has been observed
   *  both passing and failing on unchanged code, it has an owning ticket, and until that ticket lands it
   *  cannot be a gate". A case that fails EVERY run is a plain entry, not an intermittent one. */
  intermittent?: boolean;
}

/** The shape of the committed `gui-smoke/known-failing.json`. `$comment` is optional/ignored — it
 *  exists in the file purely so a human opening the JSON sees the ratchet's rule inline. */
export interface KnownFailingFile {
  $comment?: string;
  cases: KnownFailingCase[];
}

/** CPE-1753 — what the JOIN job knows about the shard plan, independent of what it downloaded.
 *
 *  `expectedShardTotal` and `expectedSpecs` are BOTH supplied by the caller from sources that a failed
 *  shard cannot influence: the workflow file's `GUI_SMOKE_EXPECT_SHARDS` literal, and a live glob of the
 *  checked-out `specs/` directory. `manifests` is the only field that comes from the artifacts. Keeping
 *  that asymmetry explicit in the type is the point — the moment an aggregation derives its expectation
 *  from the results it received, a missing shard stops being detectable at all. */
export interface ShardCoverageInput {
  /** How many shards the workflow says exist. Never `manifests.length`. */
  expectedShardTotal: number;
  /** One per shard that actually reported, in any order. */
  manifests: ShardManifest[];
  /** Every spec file basename that exists on disk right now, for the "the plan covers every spec exactly
   *  once" check. Optional: when omitted, the index/total checks still run. */
  expectedSpecs?: string[];
}

export interface EvaluateInput {
  /** One entry per test case the suite actually reported a result for, across every spec file. */
  results: CaseResult[];
  knownFailing: KnownFailingFile;
  /** How many spec FILES should have run — derived by globbing `specs/*.smoke.ts`, never hard-coded
   *  (see `scripts/run-ratchet.ts`), so adding/removing a spec file never needs a matching edit here.
   *  In a SHARD's own local verdict this is that shard's assigned subset size; at the JOIN it is the whole
   *  directory (CPE-1753) — and at the join it is deliberately globbed from git rather than inferred from
   *  the downloaded shard results, so a missing shard cannot lower it. */
  expectedSpecCount: number;
  /** CPE-1753, join-only. Present in the `gui-smoke-linux-verdict` job; `undefined` for an unsharded run
   *  and for each shard's own local verdict (clause 9 then contributes nothing). */
  shards?: ShardCoverageInput;
}

export interface EvaluateResult {
  ok: boolean;
  /** Cases (as `caseKey` strings) that failed and are NOT in known-failing.json — a real regression. */
  newFailures: string[];
  /** Cases listed in known-failing.json that PASSED this run — the one-way ratchet firing. */
  fixedButStillListed: string[];
  /** Entries in known-failing.json whose `spec` + `test` matched no case in this run — a drifted title,
   *  a deleted test, or a spec that died before reporting. Always red (see clause 3). */
  unmatchedListings: string[];
  /** Keys listed more than once in known-failing.json. */
  duplicateListings: string[];
  /** Keys listed with `intermittent: true`, paired with what they actually did this run — printed every
   *  run by the CLI wrapper so a proven-flaky exemption stays visible instead of going quiet. */
  intermittentListings: { key: string; statuses: CaseStatus[] }[];
  /** Cases (as `caseKey` strings) that reported at least one `"unknown"` status this run — a wdio
   *  `state` this ratchet's reduction didn't recognise. Always reds the run (clause 6, CPE-1680),
   *  whether or not the case happens to be listed in known-failing.json, because an unknown result
   *  can't be verified to be a legitimate exemption either. */
  unknownStates: string[];
  /** `intermittent: true` entries in known-failing.json (as `caseKey` strings) whose `reason`/`ticket`
   *  don't clear the minimum evidence bar — see clause 7 / CPE-1680. */
  unevidencedIntermittent: string[];
  /** True when fewer spec FILES reported a result than `expectedSpecCount` (a timeout/crash/hang, not a
   *  clean run) OR `expectedSpecCount < 1` (clause 8, CPE-1728 — an invalid expectation can't certify a
   *  clean run either). This alone is enough to flip `ok` false, independent of the other clauses. */
  incomplete: boolean;
  /** How many distinct spec files reported at least one case (what `incomplete` compares). */
  reportedSpecCount: number;
  /** CPE-1753 — shard indices the join EXPECTED (`1..expectedShardTotal`) but received no manifest for.
   *  Non-empty always reds. Empty when `shards` was not supplied. */
  missingShards: number[];
  /** CPE-1753 — every other structural problem with the shard plan: a duplicated or out-of-range index, a
   *  manifest whose `shardTotal` disagrees with the join's expectation, or a spec file covered by no shard
   *  (or by more than one). Non-empty always reds. */
  shardPlanProblems: string[];
  /** Human-readable lines, one per problem found, each naming exactly what to do about it. Empty when
   *  `ok` is true. */
  messages: string[];
}

/** The identity of one exempted case: spec file basename + `it()` title. Used as the map key everywhere
 *  and as the human-readable name in every message, so a log line can be pasted straight into
 *  `known-failing.json` (or grepped for in `specs/`). */
export function caseKey(spec: string, test: string): string {
  return `${spec} :: ${test}`;
}

/** The reporter's `state` strings, narrowed to what `evaluate()` reasons about. Anything unrecognised
 *  reduces to `"unknown"` — CPE-1680 — NEVER to `"skipped"`. A skipped case is exempt from every clause
 *  `evaluate()` implements, so folding an unrecognised state into it would let a state this ratchet has
 *  never seen (a new wdio version, a new runner mode, a state produced by a crash path) silently pass as
 *  "safe to ignore" — the same "confident green where the truth is unknown" shape CPE-1677 was filed
 *  over. `"unknown"` reds the run unconditionally instead (see `evaluate()`'s clause 6).
 *
 *  CPE-1680 moved this out of `scripts/run-ratchet.ts` and in here: it's the ACTUAL reduction that
 *  performs finding #1's fix, and living inside the I/O wrapper meant `test:unit`'s `lib/**\/*.test.ts`
 *  glob never collected a test for it — every unit test constructed an already-`"unknown"` `CaseStatus`
 *  by hand, which exercises `evaluate()`'s reaction to the value but never the mapping that produces it.
 *  A reviewer mutation proved this: reverting the fallback here to `"skipped"` left all 59 tests green. */
export function toCaseStatus(state: string | undefined): CaseStatus {
  if (state === "passed" || state === "failed" || state === "skipped" || state === "pending") return state;
  return "unknown";
}

/** Minimal shape read out of one `@wdio/json-reporter` output file — a subset of the reporter's own
 *  `ResultSet` type (`specs: string[]`, `suites[].tests[]`, `suites[].hooks[]`). wdio spawns one worker
 *  per spec file in this repo's `wdio.conf.ts` (no spec grouping), so `specs` is normally length 1; a
 *  longer array is still handled defensively by attributing the chunk's suites to every spec path it
 *  lists (the finest granularity the reporter's schema offers without grouping). Exported so
 *  `scripts/run-ratchet.ts`'s file-reading wrapper and this module's pure reduction share one shape. */
export interface RawResultChunk {
  specs: string[];
  suites?: {
    name?: string;
    tests?: { name?: string; state?: string }[];
    hooks?: { title?: string; state?: string; error?: unknown }[];
  }[];
}

/** Reduces already-parsed `@wdio/json-reporter` chunks into one `CaseResult` per test case, keyed by the
 *  spec file's basename + the `it()` title — the same key `known-failing.json` uses. Pure — the caller
 *  (`scripts/run-ratchet.ts#loadCaseResults`) owns finding the files, reading them, and `JSON.parse`ing
 *  them; this function never touches the filesystem, which is what makes it (and `toCaseStatus` above)
 *  reachable from `ratchet.test.ts` instead of hiding inside the CLI wrapper (CPE-1680).
 *
 *  A FAILING HOOK (`before`/`beforeEach`/`after`/`afterEach`) becomes a synthetic case named
 *  `<hook> "<title>"`, because a hook that throws usually means its suite's cases never ran at all: the
 *  cases would simply be absent, and "absent" must not read as green. The synthetic case is unlisted by
 *  construction, so it reds the job as a NEW GUI REGRESSION — and the listed cases it prevented from
 *  running additionally trip the STALE EXEMPTION clause. Both are the honest verdict for a dead suite. */
export function reduceResultChunks(chunks: RawResultChunk[]): CaseResult[] {
  // Keyed so a defensive multi-spec chunk (see RawResultChunk's doc comment) can't double-count a case;
  // a failure anywhere for that key wins.
  const byKey = new Map<string, CaseResult>();
  const record = (spec: string, test: string, status: CaseStatus): void => {
    const key = caseKey(spec, test);
    const existing = byKey.get(key);
    if (!existing) byKey.set(key, { spec, test, status });
    else if (status === "failed") existing.status = "failed";
  };

  for (const chunk of chunks) {
    for (const specPath of chunk.specs ?? []) {
      // path.basename is a pure string operation (no filesystem access) — safe in an otherwise I/O-free
      // module, and needed because the reporter records each spec by its full path.
      const spec = specBasename(specPath);
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

/** `path.basename`, without importing Node's `path` module — the one file-shaped string this otherwise
 *  I/O-free module needs to slice, and a two-line reimplementation is cheaper than explaining why a
 *  "pure, no I/O" module imports `node:path`. Handles both `/`- and `\`-separated inputs, since the
 *  reporter's `specs[]` entries are absolute paths written by whatever OS the suite ran on.
 *
 *  CPE-1698: splitting on the separator regex leaves an empty trailing element when `specPath` ends in
 *  one or more separators (`"…/case-c.smoke.ts/".split(/[/\\]/)` ends in `""`), so simply taking the
 *  last element and falling back to the whole path on falsy would return the ENTIRE input instead of a
 *  basename — the opposite of what `path.basename`/`path.win32.basename` do (they strip the trailing
 *  separator and still return `"case-c.smoke.ts"`). Filtering out empty segments before taking the last
 *  one matches that real behaviour.
 *
 *  CPE-1701 (deliberate non-goal): this is not a full `path.basename` reimplementation, and three more
 *  shapes were found to diverge from real `path.basename`/`path.win32.basename` past the five CPE-1698
 *  covers — recorded here so nobody rediscovers them, not fixed because none is reachable from a real
 *  `@wdio/json-reporter` chunk (`specs[]` entries are always real absolute paths ending in an actual
 *  `.smoke.ts` file):
 *    - `"///"` (separators only) → this returns the whole string `"///"` (every segment filters out as
 *      empty, so the `parts.length > 0 ? … : specPath` fallback fires); real `basename` returns `""`.
 *    - `"C:x.ts"` (a Windows drive-relative path with no separator after the colon) → this returns
 *      `"C:x.ts"` unchanged (no separator to split on); real `win32.basename` returns `"x.ts"`.
 *    - `"C:\\"` (a bare drive root) → this returns `"C:"` (the one non-empty segment); real
 *      `win32.basename` returns `""`.
 *  Fixing these would mean hand-rolling real basename semantics (drive letters, the `///` edge case)
 *  purely for inputs this function is never actually given — not worth the complexity it would add to a
 *  "pure, no I/O" module that exists specifically to avoid importing `node:path`. */
function specBasename(specPath: string): string {
  const parts = specPath.split(/[/\\]/).filter((part) => part.length > 0);
  return parts.length > 0 ? parts[parts.length - 1]! : specPath;
}

/** Minimum length (after trimming) a `reason` must clear for an `intermittent: true` entry to pass
 *  clause 7's evidence bar. Deliberately low — this can't verify a case is actually flaky, only real run
 *  history can, and it must not force the four already-evidenced entries in known-failing.json to be
 *  rewritten to satisfy a stricter format (three of them evidence-by-reference — "see that entry
 *  above" — rather than repeating the same run IDs four times, which the bar has to accept as-is). It
 *  exists to catch exactly the UAT-proven failure mode: an entry with an EMPTY reason. */
const MIN_INTERMITTENT_REASON_LENGTH = 20;

/** A ticket id shape loose enough to accept this repo's `CPE-NNN` (and any other `PREFIX-NNN` project)
 *  without being so strict it becomes a second thing to keep in sync with the ticketing system. */
const TICKET_ID_PATTERN = /^[A-Za-z]+-\d+$/;

/** Builds the `known-failing.json` entry to suggest in a NEW GUI REGRESSION message, pre-filled with the
 *  REAL `spec`/`test` so it's paste-ready (only `reason`/`ticket` need writing in). Uses `JSON.stringify`
 *  — never string concatenation — so a title containing a double quote round-trips as valid JSON. wdio's
 *  own global-hook titles are exactly such a title (e.g. `"before all" hook`, which `run-ratchet.ts`
 *  further wraps as `<hook> "..."` when a hook throws): string-concatenating that into `"${test}"` used
 *  to print `<hook> ""before all" hook""` — not valid JSON, and not pasteable (CPE-1680, reviewer nit 3
 *  on PR #864). `JSON.stringify` escapes it correctly regardless of how many quotes the title contains. */
export function suggestedKnownFailingEntry(spec: string, test: string): string {
  return JSON.stringify({ spec, test, reason: "<why this is allowed to fail>", ticket: "<owning ticket>" });
}

/**
 * Pure verdict function. No filesystem, no process exit, no console — a caller (the CI wrapper script,
 * or a unit test) decides what to do with the result. See the module header above for the seven failure
 * modes this implements.
 */
export function evaluate({ results, knownFailing, expectedSpecCount, shards }: EvaluateInput): EvaluateResult {
  const listed = knownFailing.cases ?? [];
  const messages: string[] = [];
  const newFailures: string[] = [];
  const fixedButStillListed: string[] = [];
  const unmatchedListings: string[] = [];
  const duplicateListings: string[] = [];
  const intermittentListings: { key: string; statuses: CaseStatus[] }[] = [];
  const unknownStates: string[] = [];
  const unevidencedIntermittent: string[] = [];
  const missingShards: number[] = [];
  const shardPlanProblems: string[] = [];

  // Clause 9 — MISSING SHARD / SHARD PLAN PROBLEM (CPE-1753). Runs FIRST and reports first, because when
  // a shard is missing every downstream clause is reasoning about a partial run and its messages ("this
  // exemption matched no test", "only 31 of 41 spec files reported") are true but misleading about the
  // cause. Naming the missing shard at the top of the log is what stops a reader from chasing a phantom
  // stale exemption. Deliberately does NOT suppress the other clauses: an incomplete run's other findings
  // are still real, and hiding them would be its own kind of dishonesty.
  if (shards) {
    const { expectedShardTotal, manifests, expectedSpecs } = shards;
    if (!Number.isInteger(expectedShardTotal) || expectedShardTotal < 1) {
      shardPlanProblems.push(`expectedShardTotal=${expectedShardTotal}`);
      messages.push(
        `SHARD PLAN INVALID: the join was told to expect ${expectedShardTotal} shard(s) (must be an ` +
          `integer >= 1). A join that doesn't know how many shards exist cannot certify that they all ` +
          `reported — check GUI_SMOKE_EXPECT_SHARDS on the gui-smoke-linux-verdict job in ` +
          `.github/workflows/gui-smoke.yml. Never inferred from the artifacts that happened to download: ` +
          `that is precisely how a missing shard would lower its own bar and pass.`,
      );
    }

    const byIndex = new Map<number, ShardManifest>();
    for (const manifest of [...manifests].sort((a, b) => a.shardIndex - b.shardIndex)) {
      if (manifest.shardIndex < 1 || manifest.shardIndex > expectedShardTotal) {
        shardPlanProblems.push(`out-of-range shard ${manifest.shardIndex}`);
        messages.push(
          `SHARD PLAN MISMATCH: a manifest reported shard ${manifest.shardIndex}, which is outside the ` +
            `expected range 1..${expectedShardTotal}. The matrix in .github/workflows/gui-smoke.yml and the ` +
            `verdict job's GUI_SMOKE_EXPECT_SHARDS have drifted apart — fix whichever is wrong; do not ` +
            `"just re-run".`,
        );
        continue;
      }
      if (byIndex.has(manifest.shardIndex)) {
        shardPlanProblems.push(`duplicate shard ${manifest.shardIndex}`);
        messages.push(
          `SHARD PLAN MISMATCH: shard ${manifest.shardIndex} reported more than one manifest. Two jobs ` +
            `believe they are the same shard, so some spec files ran twice and others may not have run at ` +
            `all — this is never a benign duplicate.`,
        );
        continue;
      }
      if (manifest.shardTotal !== expectedShardTotal) {
        shardPlanProblems.push(`shard ${manifest.shardIndex} says total=${manifest.shardTotal}`);
        messages.push(
          `SHARD PLAN MISMATCH: shard ${manifest.shardIndex} ran as "${manifest.shardIndex} of ` +
            `${manifest.shardTotal}" but the join expects ${expectedShardTotal} shard(s). The shard jobs take ` +
            `their total from GitHub's own \`strategy.job-total\` (the matrix size) and the join takes it from ` +
            `GUI_SMOKE_EXPECT_SHARDS, so this means the matrix was resized without updating that literal (or ` +
            `the reverse). Every spec file's ownership shifts when the total changes, so the two MUST agree.`,
        );
        // Still recorded below: its specs did run, and pretending they didn't would double-report.
      }
      byIndex.set(manifest.shardIndex, manifest);
    }

    for (let index = 1; index <= expectedShardTotal; index += 1) {
      if (byIndex.has(index)) continue;
      missingShards.push(index);
      messages.push(
        `MISSING SHARD: shard ${index} of ${expectedShardTotal} never reported a manifest. Its spec files ` +
          `did not run — the shard job was cancelled (a newer push superseding this run), failed before it ` +
          `could write one, or never started. This is RED, and deliberately NOT "expect fewer spec files": ` +
          `letting an absent shard shrink the expectation is how a sharded gate silently certifies a ` +
          `fraction of its suite (see CPE-1728, and lib/shard.ts's header). Open the ` +
          `"GUI smoke (ubuntu-latest) shard ${index}" job to see what happened to it.`,
      );
    }

    if (expectedSpecs) {
      const coverage = new Map<string, number[]>();
      for (const [index, manifest] of byIndex) {
        for (const spec of manifest.specs) {
          const owners = coverage.get(spec);
          if (owners) owners.push(index);
          else coverage.set(spec, [index]);
        }
      }
      const uncovered = [...expectedSpecs].filter((spec) => !coverage.has(spec)).sort(compareSpecNames);
      if (uncovered.length > 0) {
        shardPlanProblems.push(`${uncovered.length} spec file(s) covered by no shard`);
        messages.push(
          `SHARD PLAN MISMATCH: ${uncovered.length} spec file(s) exist in specs/ but no shard claimed them: ` +
            `${uncovered.join(", ")}. Either a shard is missing (see above) or the partition in ` +
            `lib/shard.ts#assignShardSpecs dropped them — an unclaimed spec runs NOWHERE while every shard ` +
            `still reports a tidy green, so this is always red.`,
        );
      }
      const doubleClaimed = [...coverage.entries()]
        .filter(([, owners]) => owners.length > 1)
        .sort((a, b) => compareSpecNames(a[0], b[0]));
      for (const [spec, owners] of doubleClaimed) {
        shardPlanProblems.push(`${spec} claimed by shards ${owners.join("+")}`);
        messages.push(
          `SHARD PLAN MISMATCH: "${spec}" was claimed by more than one shard (${owners.join(", ")}). The ` +
            `partition must be disjoint — an overlap means some other spec is going unclaimed and the ` +
            `wall-clock saving is being spent running the same file twice.`,
        );
      }
      const unknownSpecs = [...coverage.keys()]
        .filter((spec) => !expectedSpecs.includes(spec))
        .sort(compareSpecNames);
      if (unknownSpecs.length > 0) {
        shardPlanProblems.push(`${unknownSpecs.length} claimed spec file(s) do not exist`);
        messages.push(
          `SHARD PLAN MISMATCH: shard(s) claimed spec file(s) that are not in specs/ right now: ` +
            `${unknownSpecs.join(", ")}. The shard jobs and this join job checked out different commits, or ` +
            `a spec was deleted mid-run — the verdict cannot be trusted either way.`,
        );
      }
    }
  }

  const reportedSpecs = new Set(results.map((r) => r.spec));
  const reportedSpecCount = reportedSpecs.size;
  // CPE-1728 (reviewer-found hole): `expectedSpecCount < 1` reds unconditionally, checked BEFORE the
  // ordinary `reportedSpecCount < expectedSpecCount` comparison below. Without this, `expectedSpecCount
  // === 0` (a misconfigured/empty `specs/` glob) with zero results and an empty `known-failing.json`
  // evaluates `0 < 0` as false — `incomplete` stays false, nothing else fires, and `evaluate()` returns a
  // vacuous green ("0/0 spec file(s) reported ... OK") for a run that proved NOTHING. Not reachable
  // through the real CI path today (`gui-smoke/specs/` is checked out from git and always non-empty), but
  // CPE-1728's OWN change widened who can reach `evaluate()` with a caller-supplied `expectedSpecCount` —
  // `run-ratchet.ts#loadCaseResults` now tolerates a missing `.results/` dir instead of throwing, so a
  // caller only one bug away from mis-globbing `specs/` (a bad `GUI_SMOKE_SPECS_DIR` override, or a
  // future refactor) would otherwise land on a green verdict instead of a red one. Same shape as clause 6
  // (CPE-1680): an "I don't know what was supposed to run" state must never default to green.
  const expectedSpecCountInvalid = expectedSpecCount < 1;
  if (expectedSpecCountInvalid) {
    messages.push(
      `INVALID EXPECTED SPEC COUNT: expectedSpecCount is ${expectedSpecCount} (must be >= 1). A run that ` +
        `doesn't know how many spec files it was supposed to execute cannot be certified green — check the ` +
        `specs glob (\`countExpectedSpecs\`/\`GUI_SMOKE_SPECS_DIR\` in scripts/run-ratchet.ts) rather than ` +
        `trusting a "0/0 reported ... OK" verdict.`,
    );
  }
  const incomplete = expectedSpecCountInvalid || reportedSpecCount < expectedSpecCount;
  if (!expectedSpecCountInvalid && reportedSpecCount < expectedSpecCount) {
    messages.push(
      `SUITE DID NOT COMPLETE: expected ${expectedSpecCount} spec file(s) (globbed from specs/*.smoke.ts) ` +
        `but only ${reportedSpecCount} reported any result. A timeout, crash, or hang killed the job before ` +
        `it finished — this is treated as RED, never as "everything else happened to pass". Check the job ` +
        `log for where it stopped.`,
    );
  }

  // The listed exemptions, keyed. A key listed twice is a hygiene bug (clause 5): the "delete its entry"
  // advice in clauses 2/3 would leave the duplicate behind and the case still silently exempt.
  const known = new Map<string, KnownFailingCase>();
  for (const entry of listed) {
    const key = caseKey(entry.spec, entry.test);
    if (known.has(key)) {
      duplicateListings.push(key);
      continue;
    }
    known.set(key, entry);
  }
  for (const key of [...duplicateListings].sort((a, b) => a.localeCompare(b))) {
    messages.push(
      `DUPLICATE EXEMPTION: "${key}" is listed more than once in gui-smoke/known-failing.json. Delete the ` +
        `extra entry — with a duplicate present, removing "the" entry when the case is fixed would leave ` +
        `the case exempt anyway, which is exactly the silent hole this ratchet exists to close.`,
    );
  }

  // Observed cases, grouped by key. A key normally maps to exactly one case; a spec that reuses an
  // `it()` title within the same file produces several, so the group's statuses are aggregated (a
  // failure anywhere in the group means "this key still fails"). `keyToCase` keeps one representative
  // `{ spec, test }` per key so messages can rebuild the real suggested-entry JSON (clause 1) without
  // re-parsing the `caseKey` string, which would be ambiguous for a title that itself contains " :: ".
  const observed = new Map<string, CaseStatus[]>();
  const keyToCase = new Map<string, { spec: string; test: string }>();
  for (const r of results) {
    const key = caseKey(r.spec, r.test);
    const statuses = observed.get(key);
    if (statuses) statuses.push(r.status);
    else {
      observed.set(key, [r.status]);
      keyToCase.set(key, { spec: r.spec, test: r.test });
    }
  }

  // Sort for deterministic output (Map/array order isn't guaranteed stable across a re-run, and a
  // stable message order makes a flapping-vs-fixed diff easy to read in CI logs).
  const observedKeys = [...observed.keys()].sort((a, b) => a.localeCompare(b));

  // Clause 1 — a case failed that nobody exempted. THE clause CPE-1677 added case granularity for.
  for (const key of observedKeys) {
    const statuses = observed.get(key)!;
    if (!statuses.includes("failed")) continue;
    if (known.has(key)) continue;
    newFailures.push(key);
    const { spec, test } = keyToCase.get(key)!;
    messages.push(
      `NEW GUI REGRESSION: "${key}" failed and is not listed in gui-smoke/known-failing.json. If this is a ` +
        `genuine new regression, fix it. If it's a case you're intentionally deferring, add this entry ` +
        `(fill in "reason"/"ticket") to known-failing.json — do not just re-run, and do NOT exempt its ` +
        `whole spec file (case granularity is the point, see CPE-1677): ${suggestedKnownFailingEntry(spec, test)}`,
    );
  }

  // Clause 6 — UNRECOGNISED TEST STATE (CPE-1680). A state `toCaseStatus()` (run-ratchet.ts) didn't
  // recognise reduces to `"unknown"`, never to `"skipped"` — see the `CaseStatus` doc comment for why.
  // Choice made here, spelled out because "report loudly but stay green" was the other option the ticket
  // allowed: RED unconditionally, regardless of whether the case happens to be listed. Reasoning: (1) this
  // ratchet's entire reason to exist (CPE-1594) is turning a soft/swallowed signal into a hard exit code —
  // a merely-logged warning that leaves `ok` true reintroduces that exact failure shape one layer up, and
  // CI logs are precisely what gets skimmed past; (2) an unknown state cannot be verified to be a
  // legitimate "still failing, exemption is doing its job" (clause 2/3's logic) OR a legitimate "this
  // passed" (clause 2's ratchet) — there is no safe default to fall back to, only a red one. `run-ratchet.ts`
  // additionally prints every unknown-state case on every run (loud AND red, not loud OR red).
  for (const key of observedKeys) {
    const statuses = observed.get(key)!;
    if (!statuses.includes("unknown")) continue;
    unknownStates.push(key);
    messages.push(
      `UNRECOGNISED TEST STATE: "${key}" reported a wdio "state" this ratchet does not recognise (reduced ` +
        `to "unknown", never to "skipped" — an unrecognised state must never be silently exempt from every ` +
        `clause). Treated as RED unconditionally, whether or not it is listed in known-failing.json, ` +
        `because an unknown result can't be verified as a legitimate exemption either. If wdio added a new ` +
        `state, teach toCaseStatus() in gui-smoke/scripts/run-ratchet.ts about it explicitly.`,
    );
  }

  // Clauses 2 + 3 — every exemption must still match a case, and that case must still be failing.
  for (const key of [...known.keys()].sort((a, b) => a.localeCompare(b))) {
    const statuses = observed.get(key);
    if (!statuses) {
      unmatchedListings.push(key);
      messages.push(
        `STALE EXEMPTION: "${key}" is listed in gui-smoke/known-failing.json but NO test with that title ran ` +
          `in this spec. Test titles are strings and they drift — a rename, a deleted case, or a spec that ` +
          `died before reporting all look like this. Never silently ignored: update the entry's "test" to ` +
          `the new title, or delete the entry if the case is gone. (If the case is genuinely gone, its ` +
          `coverage is gone too — make sure that was intended.)`,
      );
      continue;
    }
    // `=== true`, not truthiness (CPE-1680, UAT finding): `known-failing.json` is hand-edited JSON, so
    // TypeScript's `intermittent?: boolean` on `KnownFailingCase` is a type ASSERTION about well-formed
    // input, not a runtime guarantee — a typo like `"intermittent": "false"` parses to the STRING
    // `"false"`, which is truthy in JS. Under plain truthiness that typo would silently grant
    // BOTH-direction exemption to an entry the author meant to be an ordinary plain one, bypassing the
    // one-way ratchet (clause 2) the moment it starts passing. Tightened rather than refused outright: a
    // non-`true` value degrading to "ordinary plain entry" is the least surprising reading of a malformed
    // field, and matches how the rest of this function already treats a missing `intermittent`
    // (`undefined`).
    if (known.get(key)!.intermittent === true) {
      intermittentListings.push({ key, statuses });
      continue; // proven flaky: passing is expected some runs, so it can't retire the entry either
    }
    if (statuses.includes("failed")) continue; // still failing: the exemption is doing its job
    if (!statuses.includes("passed")) continue; // skipped/pending only — neither retires nor reds it
    fixedButStillListed.push(key);
    messages.push(
      `RATCHET: "${key}" now passes but is still listed in gui-smoke/known-failing.json — delete its entry ` +
        `from that file. The ratchet is one-way: a case may leave the failing column exactly once, and ` +
        `leaving it listed after it starts passing would hide a FUTURE regression on it (it would silently ` +
        `count as "still known-failing" instead of going red).`,
    );
  }

  // Clause 7 — UNEVIDENCED INTERMITTENT (CPE-1680). `intermittent: true` is a claim that THIS exact case
  // has been observed both passing and failing on unchanged code — not a config switch for "annoying".
  // The UAT proved the marker alone has no machine-checkable bar: an entry with an empty `reason` and
  // empty `ticket` sailed through exactly like the four real, well-evidenced ones, silencing a case that
  // fails on EVERY run — a permanent break — forever. This clause cannot prove flakiness (only real run
  // history can) so it does not try to; it refuses only the structurally checkable failure mode: no
  // evidence attached at all. Checked independent of `results` — a UAT probing `evaluate()` directly with
  // no matching case should still catch it, and it must not force the four existing entries (three of
  // which evidence by reference, "see that entry above", rather than repeating the same run IDs four
  // times) to be rewritten to satisfy a stricter format.
  for (const [key, entry] of [...known.entries()].sort((a, b) => a[0].localeCompare(b[0]))) {
    // `=== true`, matching the clause 2/3 check above — same reasoning: a malformed non-boolean value
    // (e.g. the string `"false"`) must not silently opt an entry INTO the evidence bar it never asked
    // for either. It falls back to "ordinary plain entry" like `undefined` does, consistently.
    if (entry.intermittent !== true) continue;
    const reasonOk = typeof entry.reason === "string" && entry.reason.trim().length >= MIN_INTERMITTENT_REASON_LENGTH;
    const ticketOk = typeof entry.ticket === "string" && TICKET_ID_PATTERN.test(entry.ticket.trim());
    if (reasonOk && ticketOk) continue;
    unevidencedIntermittent.push(key);
    messages.push(
      `UNEVIDENCED INTERMITTENT: "${key}" is marked "intermittent": true but its "reason"/"ticket" don't ` +
        `clear the evidence bar (a "reason" of at least ${MIN_INTERMITTENT_REASON_LENGTH} characters, and a ` +
        `"ticket" that looks like a real ticket id). "intermittent" means this exact case has been OBSERVED ` +
        `both passing and failing on unchanged code — a case that fails every run is a plain entry, not ` +
        `this. Cite the evidence in "reason" and the owning ticket in "ticket", or drop the marker.`,
    );
  }

  const ok =
    !incomplete &&
    newFailures.length === 0 &&
    fixedButStillListed.length === 0 &&
    unmatchedListings.length === 0 &&
    duplicateListings.length === 0 &&
    unknownStates.length === 0 &&
    unevidencedIntermittent.length === 0 &&
    // CPE-1753 — a missing shard or a broken shard plan reds unconditionally, on the same footing as
    // `incomplete`. It is NOT folded into `incomplete` on purpose: the two answer different questions
    // ("did every job report?" vs "did every spec file report?") and a reader who sees only
    // `incomplete=true` would look for a hung suite instead of a job that never ran.
    missingShards.length === 0 &&
    shardPlanProblems.length === 0;

  return {
    ok,
    missingShards,
    shardPlanProblems,
    newFailures,
    fixedButStillListed,
    unmatchedListings,
    duplicateListings,
    intermittentListings,
    unknownStates,
    unevidencedIntermittent,
    incomplete,
    reportedSpecCount,
    messages,
  };
}
