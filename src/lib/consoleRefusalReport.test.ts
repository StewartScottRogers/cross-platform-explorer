// CPE-1975 — the rendezvous-path **refusal must be reported on a channel that is open by default**,
// and this is the guard that can actually see a regression of that.
//
// ## Why this file exists, which is a two-step failure worth writing down
//
// Round 2 fixed a swallowed error in `SessionDaemonHandle::discover_or_spawn` by routing it to
// `session_diag::trace` — a reporter that **returns early unless one of four env vars is set**, none
// of which the console process sets on the path this code exists for. The report went to nobody. The
// lesson recorded at that site: *a report is only as good as the channel it lands in — check the
// channel, not just the call.*
//
// Round 3 fixed that with an ungated `writeln!(std::io::stderr(), …)`, and then added a unit test in
// `session_diag`'s own module claiming it meant *"a future edit routing a must-see message back
// through `trace` alone has a red test standing next to it."* **That claim was false, and measured
// false:** deleting the `writeln!` from `discover_or_spawn` and leaving the `trace` call alone left
// the crate at **423 passed / 0 failed** (`Compiling ai-console` present, so not a stale binary).
// The test asserted a property of `trace`; nothing structurally connected it to `discover_or_spawn`
// or to any call site, so it could not fire for the regression it named.
//
// That is CLAUDE.md's rule twice in one PR: **do not name a backstop without checking it can fire —
// one that structurally cannot fire is worse than none, because it reads as one.** So the guard now
// lives where the regression lives: over the source of the call site itself.
//
// ## How it reads the source, and the strip claim — measured in both directions
//
// Comments are **stripped first** (`stripRustComments`), CPE-1933 rule 2. Same machinery and
// precedent as `MacroRunConfirm.test.ts`, which walks a `format!` literal out of `fsutil.rs`.
//
// Round 4 asserted the strip was "load-bearing rather than ceremonial ... reproduced here by
// construction". **That was false and was measured false**, in both directions: the scanned region
// was anchored at `if let Err(`, and the ~30-line block quoting the call sits *before* that anchor,
// so it was never scanned. Raw and stripped source gave identical verdicts. **A third sentence in
// this PR that reached further than its measurement** — after a report routed to a channel that was
// off, and a guard that could not see what it named.
//
// Round 5 fixed the mechanism rather than the sentence: [`reportRegion`] now starts at the
// `spawn_detached` line, so the quoting block falls **inside** the scanned region and the strip
// decides the outcome. Measured 2026-08-28, all four cells run (`npx vitest run
// src/lib/consoleRefusalReport.test.ts`, 3 tests in the file) — and **re-run after the call-site
// comment was edited**, because that comment lives inside the scanned region and editing it could
// have changed every cell:
//
// | | strip ON | strip OFF (raw source) |
// |---|---|---|
// | **real code** | 3 passed — green | **1 failed**: the `enabled()` leg reds on the comment |
// | **`writeln!` deleted** | **1 failed**: the stderr leg reds, naming the missing call | **the stderr leg PASSES** — it matched the comment's quotation; only the `enabled()` leg reds, for the wrong reason |
//
// The bottom-right cell is the point: with the strip off, **the leg that guards the deletion goes
// green on a comment.** That is CPE-1933's silent-pass shape, now actually reproduced rather than
// asserted. The top-right cell is the other direction — raw source reds on correct code, because the
// same block explains the `enabled()` gate it is checking for the absence of.
//
// ## Red-proof, run rather than asserted
//
// Written while the sabotage was still applied (the `writeln!` deleted from the error arm) and run:
// **RED**, naming the missing call. Restoring the line turned it green. So this guard is measured to
// fire on the one regression round 3's test could not see.
//
// ## Stated blind spot — this is a tripwire, not a proof
//
// It asserts a **code shape is present** in one function's error arm: an ungated `writeln!` to
// `std::io::stderr()`, with no `enabled()` in that arm. It does **not** prove the line is reached at
// run time, that stderr is attached, or that some future wrapper does not gate it in a way this
// scan cannot see. Proving the runtime property would need the console spawned with a planted link
// and its stderr captured — worth doing on the day `discover_or_spawn` acquires a caller, and noted
// here so that is a decision rather than an oversight. What this closes is the regression that
// actually happened and was otherwise caught by nothing.
//
// **Not caught today, at least these** (an open list — never a count):
//
// * The scan is anchored on **literal substrings**. A semantically-equivalent respelling
//   (`let mut e = std::io::stderr(); writeln!(e, …)`) reds — the safe direction, a false alarm
//   rather than a miss. But **a rename of the `Ok(handle)` tail silently moves the slice boundary**,
//   and a rename of the `let handle = Self::spawn_detached` start anchor throws (loud) while a
//   changed tail merely shrinks or grows the region. That asymmetry is the one to watch.
// * `not.toContain("enabled()")` is a substring test over the region, so it catches the gate being
//   named there; it cannot see a gate expressed some other way (a helper, a `cfg!`, a bool computed
//   earlier and passed in).
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { stripRustComments } from "./rustSource";
import { REPO_ROOT } from "./rustProductionSources";

const SUPERVISOR = "sidecar/ai-console/src/session_supervisor.rs";

/**
 * The body of `pub fn <name>` in comment-stripped source, ending at the next `pub fn` in the same
 * `impl` block.
 *
 * Deliberately delimited by the next `pub fn` rather than by brace matching: after comment
 * stripping, string literals survive, and this very function contains a `format!` whose literal
 * holds `{}` placeholders — a naive brace counter walks straight out of the function on them.
 * Throws when either anchor is missing, so a rename reds loudly instead of silently scanning an
 * empty string that vacuously satisfies every `not.toContain` below.
 */
function fnBody(source: string, name: string): string {
  const anchor = `pub fn ${name}`;
  const at = source.indexOf(anchor);
  if (at < 0) {
    throw new Error(
      `CPE-1975: \`${anchor}\` not found in ${SUPERVISOR}. If it was renamed or removed, update this ` +
        `guard — a missing anchor must red, never silently scan nothing.`,
    );
  }
  const next = source.indexOf("pub fn ", at + anchor.length);
  if (next < 0) {
    throw new Error(`CPE-1975: no function follows \`${anchor}\`, so its body cannot be delimited`);
  }
  return source.slice(at, next);
}

/**
 * The region of `discover_or_spawn` that reports a `write_port_file` failure: from the
 * `spawn_detached` call that produces the handle, to the function's `Ok(handle)` tail.
 *
 * **The start anchor is deliberately the `spawn_detached` line and not the `if let Err(`**, and that
 * choice is the whole reason comment-stripping is load-bearing here. The ~30-line block explaining
 * *why* the report is ungated sits **between** those two anchors and quotes both
 * `writeln!(std::io::stderr(), …)` and `enabled()` verbatim. Anchoring on `if let Err(` — which is
 * what round 4 did — put that block outside the scanned region, so raw and stripped source gave
 * identical verdicts and the strip did nothing. Round 5 widened it, and the four-cell table in this
 * file's header is the measurement showing the strip now decides the outcome.
 */
function reportRegion(source: string): string {
  const body = fnBody(source, "discover_or_spawn");
  // The premise: this function still writes the port file at all. Without it the assertions below
  // would be about a code path that no longer exists.
  if (!body.includes("write_port_file(")) {
    throw new Error("CPE-1975: discover_or_spawn no longer calls write_port_file");
  }
  const from = body.indexOf("let handle = Self::spawn_detached");
  const to = body.indexOf("Ok(handle)");
  if (from < 0) {
    throw new Error(
      "CPE-1975: no `let handle = Self::spawn_detached` start anchor in discover_or_spawn — the " +
        "scanned region cannot be delimited, so this guard would be asserting over nothing",
    );
  }
  if (to <= from) {
    throw new Error("CPE-1975: no `Ok(handle)` tail after the start anchor to delimit the region");
  }
  return body.slice(from, to);
}

describe("CPE-1975 — the refusal is reported on an ungated channel", () => {
  const source = () => stripRustComments(readFileSync(join(REPO_ROOT, SUPERVISOR), "utf8"));

  it("discover_or_spawn's write_port_file failure writes to the real stderr handle", () => {
    expect(
      reportRegion(source()).replace(/\s+/g, ""),
      "CPE-1975: the write_port_file refusal must be reported with `writeln!(std::io::stderr(), …)`. " +
        "Reporting it only through `session_diag::trace` sends it nowhere — `trace` returns early " +
        "unless one of four env vars is set, and the console process sets none of them on this path. " +
        "See this file's header and the comment at the call site.",
    ).toContain("writeln!(std::io::stderr()");
  });

  it("and that report is not gated on diagnostics being enabled", () => {
    expect(
      reportRegion(source()),
      "CPE-1975: the stderr report must not be conditional on `enabled()` — that is the gate this " +
        "whole change exists to get out from behind",
    ).not.toContain("enabled()");
  });

  it("session_diag::trace really is gated, which is why the assertion above matters", () => {
    // Derived from `trace`'s own source rather than restated from memory: if the early return ever
    // goes away, the reasoning in the test above (and in three doc comments) needs revisiting, and
    // this is what will say so.
    const diag = stripRustComments(
      readFileSync(join(REPO_ROOT, "sidecar/ai-console/src/session_diag.rs"), "utf8"),
    );
    const body = fnBody(diag, "trace");
    expect(
      body.replace(/\s+/g, ""),
      "`trace` no longer returns early on `!enabled()`. That would make it a usable channel for " +
        "must-see messages — re-check CPE-1975's reasoning before relying on it.",
    ).toContain("if!enabled(){return;}");
  });
});
