// CPE-1728 — distinguishes "the app is broken" (a real AssertionError fired) from "the runner could
// not paint" (WebDriver-level noise: DRI3/EGL failures, elements never becoming interactable, a
// renderer falling behind under CI) in the gui-smoke-linux leg's own output, WITHOUT changing what
// passes or fails. Pure text classification over the suite's captured stdout/stderr — no filesystem,
// no process exit — reachable from a headless unit test the same way `ratchet.ts` is (see
// `logSignature.test.ts`), mirroring the "pure lib module + thin CLI wrapper" split
// `scripts/run-ratchet.ts` already established.
//
// Why this exists: PR #900 (CPE-1723) redded three unrelated specs (`network.smoke.ts`,
// `samples.smoke.ts`, `saved-search.smoke.ts`) after a run saturated with `libEGL`/DRI3 warnings,
// `move target out of bounds`, and a flood of `no such element` — and zero `AssertionError`s anywhere
// in the log. That is the signature of a WebKitGTK/Xvfb renderer that isn't painting fast enough for
// WebDriver's own element queries, not of a broken assertion — but a reader had to count PASSED/FAILED
// lines and grep the raw log by hand to work that out (the Foreman's own first read of the log's last
// 25 lines concluded "infrastructure", and only the pass/fail COUNT contradicted it). This module makes
// the distinction a first-class, printed fact instead of a manual log-reading exercise.
//
// Deliberately advisory only (CPE-1728 AC "do not simply add retries or raise timeouts" — the same
// reasoning extends to this): this NEVER changes the ratchet's verdict or the job's exit code. Doing
// that — e.g. "an environment-signature failure doesn't count" — would be exactly the "hide the race"
// move CPE-1679 refused; a case that fails under this signature might still be a real regression that
// merely LOOKS environmental. This module only labels what kind of red a red already is, so a human (or
// the Foreman) triaging the log doesn't have to re-derive that by hand every time.

export interface LogSignatureResult {
  assertionErrorCount: number;
  environmentMarkerCount: number;
  environmentMarkersHit: string[];
  verdict: "assertion-failures" | "environment-signature-only" | "mixed" | "no-signal";
}

/** Substrings observed in real gui-smoke-linux runs (PR #900 / CPE-1728's own evidence) that come from
 *  WebDriver or the WebKitGTK-under-Xvfb stack itself, never from a mocha/chai assertion. Each is a
 *  literal, case-sensitive substring match — deliberately narrow (the same "exact, not a pattern" rule
 *  `known-failing.json` uses) so this can't be fooled by a coincidental partial match, and easy to
 *  extend if a future signature run turns up a marker not covered yet. */
export const ENVIRONMENT_MARKERS: readonly string[] = [
  "move target out of bounds",
  "no such element",
  "Could not get DRI3 device",
  "could not inhibit screen lock",
  "AT-SPI: Error retrieving accessibility bus address",
  "stale element reference",
  "unknown error: session deleted",
];

/** The literal string a thrown `AssertionError` (chai's `expect(...).to...`, or a bare node `assert`)
 *  carries in its own name — the one marker that means a real check actually ran and actually failed,
 *  as opposed to a WebDriver command itself throwing before any assertion got the chance to run. */
export const ASSERTION_MARKER = "AssertionError";

function countOccurrences(haystack: string, needle: string): number {
  if (needle.length === 0) return 0;
  let count = 0;
  let index = haystack.indexOf(needle);
  while (index !== -1) {
    count++;
    index = haystack.indexOf(needle, index + needle.length);
  }
  return count;
}

/** Classifies a captured log's text. Pure — takes the whole log as one string, returns counts and a
 *  verdict; never touches disk or a process (the CLI wrapper, `scripts/classify-log.ts`, owns reading
 *  the file). */
export function classifyLog(logText: string): LogSignatureResult {
  const assertionErrorCount = countOccurrences(logText, ASSERTION_MARKER);
  const environmentMarkersHit = ENVIRONMENT_MARKERS.filter((marker) => logText.includes(marker));
  const environmentMarkerCount = environmentMarkersHit.reduce(
    (sum, marker) => sum + countOccurrences(logText, marker),
    0,
  );

  let verdict: LogSignatureResult["verdict"];
  if (assertionErrorCount > 0 && environmentMarkerCount > 0) verdict = "mixed";
  else if (assertionErrorCount > 0) verdict = "assertion-failures";
  else if (environmentMarkerCount > 0) verdict = "environment-signature-only";
  else verdict = "no-signal";

  return { assertionErrorCount, environmentMarkerCount, environmentMarkersHit, verdict };
}

/** Human-readable lines for the CI log — mirrors `run-ratchet.ts`'s "pure formatter, thin CLI wrapper
 *  prints it" split. Purely advisory: never asserts, never returns an exit code, never changes what
 *  gates the job — see the module header above. */
export function formatSignatureReport(result: LogSignatureResult): string[] {
  const lines: string[] = [
    `[gui-smoke log-signature] ${result.assertionErrorCount} AssertionError occurrence(s), ` +
      `${result.environmentMarkerCount} environment-signature occurrence(s) ` +
      `(${result.environmentMarkersHit.length} distinct marker(s)) in this run's captured suite output.`,
  ];

  if (result.verdict === "assertion-failures") {
    lines.push(
      "[gui-smoke log-signature] REAL ASSERTION FAILURE(S) observed — at least one check actually ran " +
        "and failed. This is evidence about the app/spec under test; investigate normally.",
    );
  } else if (result.verdict === "environment-signature-only") {
    lines.push(
      "[gui-smoke log-signature] ENVIRONMENT SIGNATURE ONLY — no AssertionError anywhere in this run's " +
        `output, but ${result.environmentMarkersHit.length} known WebDriver/runner marker(s) matched ` +
        `(${result.environmentMarkersHit.join(", ")}). This is the signature of a renderer that did not ` +
        "paint/settle in time under CI (CPE-1728), not of a broken assertion. A case failing under this " +
        "signature is NOT, by itself, strong evidence the code under test is broken — but it still gates " +
        "the job normally (see the Ratchet step above/below): this line only tells you what KIND of red " +
        "you're looking at, it does not excuse it.",
    );
  } else if (result.verdict === "mixed") {
    lines.push(
      "[gui-smoke log-signature] MIXED — both a real AssertionError and environment-signature marker(s) " +
        `(${result.environmentMarkersHit.join(", ")}) appear in this run's output. Do not assume every ` +
        "failing case is environmental just because some markers are present; the AssertionError(s) need " +
        "real investigation.",
    );
  } else {
    lines.push(
      "[gui-smoke log-signature] No AssertionError and no known environment marker found in the " +
        "captured output (a clean run, or the log capture itself is empty/missing).",
    );
  }

  return lines;
}
