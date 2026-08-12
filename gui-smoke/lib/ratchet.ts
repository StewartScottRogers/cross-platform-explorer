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
// FIVE distinct failure modes, each with its own message so a future engineer knows exactly what to do
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

/** A case's outcome as the JSON reporter records it. `skipped`/`pending` are neither a pass nor a
 *  failure: they can't red the job (clause 1) and can't retire an exemption (clause 2), but they DO
 *  count as "this title still exists" for clause 3. */
export type CaseStatus = "passed" | "failed" | "skipped" | "pending";

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

export interface EvaluateInput {
  /** One entry per test case the suite actually reported a result for, across every spec file. */
  results: CaseResult[];
  knownFailing: KnownFailingFile;
  /** How many spec FILES should have run — derived by globbing `specs/*.smoke.ts`, never hard-coded
   *  (see `scripts/run-ratchet.ts`), so adding/removing a spec file never needs a matching edit here. */
  expectedSpecCount: number;
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
  /** True when fewer spec FILES reported a result than `expectedSpecCount` — a timeout/crash/hang, not
   *  a clean run. This alone is enough to flip `ok` false, independent of the other clauses. */
  incomplete: boolean;
  /** How many distinct spec files reported at least one case (what `incomplete` compares). */
  reportedSpecCount: number;
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

/**
 * Pure verdict function. No filesystem, no process exit, no console — a caller (the CI wrapper script,
 * or a unit test) decides what to do with the result. See the module header above for the five failure
 * modes this implements.
 */
export function evaluate({ results, knownFailing, expectedSpecCount }: EvaluateInput): EvaluateResult {
  const listed = knownFailing.cases ?? [];
  const messages: string[] = [];
  const newFailures: string[] = [];
  const fixedButStillListed: string[] = [];
  const unmatchedListings: string[] = [];
  const duplicateListings: string[] = [];
  const intermittentListings: { key: string; statuses: CaseStatus[] }[] = [];

  const reportedSpecs = new Set(results.map((r) => r.spec));
  const reportedSpecCount = reportedSpecs.size;
  const incomplete = reportedSpecCount < expectedSpecCount;
  if (incomplete) {
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
  // failure anywhere in the group means "this key still fails").
  const observed = new Map<string, CaseStatus[]>();
  for (const r of results) {
    const key = caseKey(r.spec, r.test);
    const statuses = observed.get(key);
    if (statuses) statuses.push(r.status);
    else observed.set(key, [r.status]);
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
    messages.push(
      `NEW GUI REGRESSION: "${key}" failed and is not listed in gui-smoke/known-failing.json. If this is a ` +
        `genuine new regression, fix it. If it's a case you're intentionally deferring, add an entry ` +
        `{ "spec": ..., "test": ..., "reason": ..., "ticket": ... } for it to known-failing.json — do not ` +
        `just re-run, and do NOT exempt its whole spec file (case granularity is the point, see CPE-1677).`,
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
    if (known.get(key)!.intermittent) {
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

  const ok =
    !incomplete &&
    newFailures.length === 0 &&
    fixedButStillListed.length === 0 &&
    unmatchedListings.length === 0 &&
    duplicateListings.length === 0;

  return {
    ok,
    newFailures,
    fixedButStillListed,
    unmatchedListings,
    duplicateListings,
    intermittentListings,
    incomplete,
    reportedSpecCount,
    messages,
  };
}
