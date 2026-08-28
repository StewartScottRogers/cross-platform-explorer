// CPE-1975 — the `<temp>/cpe-ai-console/session-daemon.port` rendezvous path is spelled in exactly
// ONE place, and nothing rebuilds it by hand.
//
// ## What this file used to be, and why that was wrong
//
// Round 1 found the path written down **twice** — `sidecar/ai-console` and `sidecar/host` — under a
// bare `Keep them in sync` comment, which is the untested provenance claim CLAUDE.md warns about.
// It replaced that comment with a derived test that read both literals and asserted they matched,
// and justified keeping the duplicate like this:
//
//     The duplication is forced: ADR 0001's one-way rule means the host may not depend on a
//     sidecar crate, and CI fails the build if it tries.
//
// **That was false, and it was the same defect one level up** — a claim about a CI guard, stated as
// measured fact, sitting next to a green test that actually vouched only for two string literals
// being equal. ADR 0001's rule (`docs/adr/0001-sidecar-platform.md`) and its CI guard (`ci.yml`,
// "Enforce one-way dependency") both point the **other** way: a *sidecar* must never depend on the
// *explorer app*. The guard greps `sidecar/*/Cargo.toml` for `^(app_lib|cross-platform-explorer)\b`
// or `path = "../../src-tauri"`. A `path = "../ai-console"` in the host's manifest matches neither,
// so CI would have passed. The experiment was never run.
//
// So the duplication is gone rather than derived. Both constants live in `sidecar-contract`, which
// **both** crates already depend on — no new dependency edge, no effect on the one-way rule or the
// delete-test — and each crate re-exports them. CPE-1950's stated preference: where the duplication
// is removable, remove it, rather than building a guard to watch two copies agree.
//
// What is left to guard is the thing a shared constant cannot enforce by itself: that no site goes
// back to spelling the path inline. That is exactly how three sites came to exist with only two of
// them hardened.
//
// ## Red-proof, run rather than asserted
//
// Putting a bare `.join("cpe-ai-console")` back into `session_diag::log_path` and running
// `npx vitest run src/lib/consoleTempDirPath.test.ts` reds this file's sweep, reporting
// `sidecar/ai-console/src/session_diag.rs: "cpe-ai-console" appears 1x (allowed 0)`. Measured in
// round 1 against the same assertion (which then allowed two declaration sites rather than one) and
// **re-run in round 2 against this version**; reverted both times. A guard that never actually
// re-reads its source is the same defect with extra steps, so this was run, and the result is
// recorded here — at the site — rather than only in a PR body.
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { stripRustComments, rustStringLiteralAfter } from "./rustSource";
import { productionCode, productionRustFiles, REPO_ROOT } from "./rustProductionSources";

/** The single crate allowed to declare the two names. */
const DECLARATION_HOME = "sidecar/contract/src/lib.rs";

/**
 * The string literal assigned to `pub const <name>: &str = "…";` in comment-stripped Rust source.
 *
 * Anchored on the declaration's own text, and it THROWS when the anchor is missing rather than
 * returning a default — a renamed constant must red loudly, not derive an empty value that vacuously
 * matches nothing (the failure mode `rustStrSliceAfter` documents for its own anchor).
 */
function rustConstStr(source: string, name: string): string {
  const anchor = `pub const ${name}: &str`;
  const at = source.indexOf(anchor);
  if (at < 0) {
    throw new Error(
      `CPE-1975: could not find \`${anchor}\` in ${DECLARATION_HOME}. If the constant was renamed, ` +
        `rename it in this test too — a missing anchor must red rather than silently derive nothing.`,
    );
  }
  const eq = source.indexOf("=", at + anchor.length);
  const semi = source.indexOf(";", at);
  if (eq < 0 || (semi >= 0 && eq > semi)) {
    throw new Error(`CPE-1975: \`${anchor}\` has no initialiser before its \`;\``);
  }
  return rustStringLiteralAfter(source, eq);
}

describe("CPE-1975 — the AI Console rendezvous path is spelled once", () => {
  // The constants are the whole mechanism now that the second copy is gone, so pin what they say.
  // These two literals are hand-written here on purpose: if the rendezvous name legitimately changes,
  // the contract crate and this line must move together, which is the reviewable diff the old "keep
  // them in sync" comment never forced.
  it("the contract crate declares the shipped values", () => {
    const contract = stripRustComments(readFileSync(join(REPO_ROOT, DECLARATION_HOME), "utf8"));
    expect(rustConstStr(contract, "CONSOLE_DIR_NAME")).toBe("cpe-ai-console");
    expect(rustConstStr(contract, "PORT_FILE_NAME")).toBe("session-daemon.port");
  });

  // The point of a single declaration is that nothing rebuilds the path by hand — that is how three
  // sites ended up with three copies and only two of them got hardened.
  //
  // Stated blind spot, honestly (CLAUDE.md's "say at least these, never a count"): this catches at
  // LEAST a bare `"cpe-ai-console"` or `"session-daemon.port"` literal in tracked production Rust
  // outside the one declaration file. It does not see a path built from a `format!`, from a `const`
  // under another name, from a byte string, or by concatenation. It is a tripwire for the shape that
  // actually occurred, not a closure of the class.
  it("no site outside the contract crate spells either name", () => {
    const files = productionRustFiles();
    // CPE-1932: an enumeration that came back near-empty must fail loudly, never render a green
    // verdict over nothing. The repo has hundreds of tracked `.rs` files.
    expect(files.length).toBeGreaterThan(100);
    // And the one file that is *supposed* to contain them must be in the list, or the sweep is
    // asserting an absence over a set that never included the interesting case.
    expect(files).toContain(DECLARATION_HOME);

    const offenders: string[] = [];
    for (const rel of files) {
      const code = productionCode(rel);
      for (const literal of ['"cpe-ai-console"', '"session-daemon.port"']) {
        const count = code.split(literal).length - 1;
        const allowed = rel === DECLARATION_HOME ? 1 : 0;
        if (count > allowed) {
          offenders.push(`${rel}: ${literal} appears ${count}x (allowed ${allowed})`);
        }
      }
    }
    expect(
      offenders,
      "CPE-1975: use sidecar_contract::{CONSOLE_DIR_NAME, PORT_FILE_NAME} instead of rebuilding the " +
        "path by hand",
    ).toEqual([]);
  });
});
