// CPE-1594 — the gui-smoke Linux leg's blocking gate. Pure, no I/O (mirrors the `compare.ts`/
// `compare.test.ts` split — see `scripts/run-ratchet.ts` for the filesystem-aware wrapper a CI step
// actually runs).
//
// The problem this fixes: `gui-smoke.yml`'s Linux leg was `continue-on-error: true` — it produced a
// real, readable "33 passed, 7 failed" result every run, but nothing ever turned that into a pass/fail
// verdict, so it never gated anything and the failing tail grew silently (3 -> 7 specs, CPE-1507 ->
// today). This module is the missing verdict function: given the suite's own JSON results, the
// committed list of specs that are ALLOWED to fail today, and how many specs SHOULD have run, decide
// whether the run is green.
//
// Three distinct failure modes, each with its own message so a future engineer knows exactly what to
// do without re-deriving the ratchet's rules from scratch:
//   1. NEW GUI REGRESSION      — a spec failed that isn't in known-failing.json.
//   2. RATCHET (one-way)       — a spec in known-failing.json now PASSES. Per the QA charter a surface
//      may leave the failing column exactly once; leaving a passing spec listed hides a future
//      regression on it (it would silently rejoin known-failing instead of going red).
//   3. SUITE DID NOT COMPLETE  — fewer specs reported a result than the suite was supposed to run. This
//      is the SPECIFIC guard against the failure mode CPE-1594 exists to fix: a timeout/crash/hang that
//      kills the job partway through must never be indistinguishable from "everything after the crash
//      happened to pass" (see CPE-1594's evidence: 796 straight `cancelled` runs).

/** One spec file's outcome, already reduced from the raw WebdriverIO JSON report (see
 *  `scripts/run-ratchet.ts#loadSpecResults` for how that reduction happens). `spec` is the spec file's
 *  basename (e.g. `"archive-browse.smoke.ts"`) — the same key `known-failing.json` uses. */
export interface SpecResult {
  spec: string;
  status: "passed" | "failed";
}

export interface KnownFailingEntry {
  reason: string;
  ticket: string;
}

/** The shape of the committed `gui-smoke/known-failing.json`. `$comment` is optional/ignored — it
 *  exists in the file purely so a human opening the JSON sees the ratchet's rule inline. */
export interface KnownFailingFile {
  $comment?: string;
  specs: Record<string, KnownFailingEntry>;
}

export interface EvaluateInput {
  /** One entry per spec file the suite actually reported a result for. */
  results: SpecResult[];
  knownFailing: KnownFailingFile;
  /** How many spec files SHOULD have run — derived by globbing `specs/*.smoke.ts`, never hard-coded
   *  (see `scripts/run-ratchet.ts`), so adding/removing a spec file never needs a matching edit here. */
  expectedSpecCount: number;
}

export interface EvaluateResult {
  ok: boolean;
  /** Specs that failed and are NOT in known-failing.json — a real regression. */
  newFailures: string[];
  /** Specs listed in known-failing.json that PASSED this run — the one-way ratchet firing. */
  fixedButStillListed: string[];
  /** True when fewer specs reported a result than `expectedSpecCount` — a timeout/crash/hang, not a
   *  clean run. This alone is enough to flip `ok` false, independent of `newFailures`/`fixedButStillListed`. */
  incomplete: boolean;
  /** Human-readable lines, one per problem found, each naming exactly what to do about it. Empty when
   *  `ok` is true. */
  messages: string[];
}

/**
 * Pure verdict function. No filesystem, no process exit, no console — a caller (the CI wrapper script,
 * or a unit test) decides what to do with the result. See the module header above for the three failure
 * modes this implements.
 */
export function evaluate({ results, knownFailing, expectedSpecCount }: EvaluateInput): EvaluateResult {
  const known = knownFailing.specs ?? {};
  const messages: string[] = [];
  const newFailures: string[] = [];
  const fixedButStillListed: string[] = [];

  const incomplete = results.length < expectedSpecCount;
  if (incomplete) {
    messages.push(
      `SUITE DID NOT COMPLETE: expected ${expectedSpecCount} spec file(s) (globbed from specs/*.smoke.ts) ` +
        `but only ${results.length} reported a result. A timeout, crash, or hang killed the job before it ` +
        `finished — this is treated as RED, never as "everything else happened to pass". Check the job log ` +
        `for where it stopped.`,
    );
  }

  // Sort for deterministic output (Object key order / array order isn't guaranteed to be stable
  // across a re-run, and a stable message order makes a flapping-vs-fixed diff easy to read in CI logs).
  const sortedResults = [...results].sort((a, b) => a.spec.localeCompare(b.spec));

  for (const r of sortedResults) {
    const isKnown = Object.prototype.hasOwnProperty.call(known, r.spec);
    if (r.status === "failed" && !isKnown) {
      newFailures.push(r.spec);
      messages.push(
        `NEW GUI REGRESSION: "${r.spec}" failed and is not listed in gui-smoke/known-failing.json. ` +
          `If this is a genuine new regression, fix it. If it's a spec you're intentionally deferring, add ` +
          `an entry for it to known-failing.json with a reason and an owning ticket — do not just re-run.`,
      );
    }
    if (r.status === "passed" && isKnown) {
      fixedButStillListed.push(r.spec);
      messages.push(
        `RATCHET: "${r.spec}" now passes but is still listed in gui-smoke/known-failing.json — delete its ` +
          `entry from that file. The ratchet is one-way: a spec may leave the failing column exactly once, ` +
          `and leaving it listed after it starts passing would hide a FUTURE regression on it (it would ` +
          `silently count as "still known-failing" instead of going red).`,
      );
    }
  }

  const ok = !incomplete && newFailures.length === 0 && fixedButStillListed.length === 0;
  return { ok, newFailures, fixedButStillListed, incomplete, messages };
}
