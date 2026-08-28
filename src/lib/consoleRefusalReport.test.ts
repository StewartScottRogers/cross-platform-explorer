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
// ## How it reads the source
//
// Comments are **stripped first** (`stripRustComments`), which is load-bearing here rather than
// ceremonial: the function this scans is wrapped in ~30 lines of commentary that quote
// `writeln!(std::io::stderr(), …)` verbatim while explaining why it is there. A scanner reading raw
// text would match the *explanation* and pass with the code deleted — the exact silent-pass shape
// CPE-1933 documents, reproduced here by construction. Same machinery and same precedent as
// `MacroRunConfirm.test.ts`, which walks a `format!` literal out of `fsutil.rs`.
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

describe("CPE-1975 — the refusal is reported on an ungated channel", () => {
  const source = () => stripRustComments(readFileSync(join(REPO_ROOT, SUPERVISOR), "utf8"));

  it("discover_or_spawn's write_port_file failure arm writes to the real stderr handle", () => {
    const body = fnBody(source(), "discover_or_spawn");

    // The premise: this function still writes the port file at all. Without it the assertions below
    // would be about a code path that no longer exists.
    expect(body, "discover_or_spawn no longer calls write_port_file").toContain("write_port_file(");

    // The error arm, sliced from the `if let Err(` to the function's `Ok(handle)` tail.
    const from = body.indexOf("if let Err(");
    const to = body.indexOf("Ok(handle)");
    expect(from, "no `if let Err(` arm around write_port_file — is the error swallowed again?").toBeGreaterThan(-1);
    expect(to, "no `Ok(handle)` tail to delimit the error arm").toBeGreaterThan(from);
    const arm = body.slice(from, to);

    // The whole point: an UNGATED write to the process's own stderr.
    expect(
      arm.replace(/\s+/g, ""),
      "CPE-1975: the write_port_file refusal must be reported with `writeln!(std::io::stderr(), …)`. " +
        "Reporting it only through `session_diag::trace` sends it nowhere — `trace` returns early " +
        "unless one of four env vars is set, and the console process sets none of them on this path. " +
        "See this file's header and the comment at the call site.",
    ).toContain("writeln!(std::io::stderr()");

    // And nothing in that arm may gate the report on diagnostics being on.
    expect(
      arm,
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
