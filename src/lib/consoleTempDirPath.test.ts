// CPE-1975 — the `<temp>/cpe-ai-console/session-daemon.port` path is written down TWICE, in two Rust
// crates, and it has to be the same path in both or the host's startup reaper sweeps a file nobody
// wrote and the sidecar leaves a stale one behind.
//
// The duplication is forced, not sloppy: ADR 0001's one-way dependency rule means `sidecar/host` may
// not depend on `sidecar/ai-console`, and CI fails the build if it tries. So there is no "delete the
// second copy" available here (which CPE-1950 rightly prefers where it is), only "derive it".
//
// Before this file, the second copy carried a comment saying
//
//     Mirrors `ai-console`'s `session_supervisor::default_port_file()` … Keep them in sync.
//
// which is exactly the untested provenance claim CLAUDE.md warns about — worse than no comment,
// because the green suite next to it reads as vouching for it. It was true when written; nothing
// would have noticed if either side moved.
//
// So: read both Rust sources, pull the literals out of the two declarations, and assert they agree.
// Comments are stripped first (`stripRustComments`) — CPE-1933's rule 2, anchor on code and never on
// prose, because these very files now contain doc comments that quote both literals, and a scanner
// reading raw source would happily certify a comment against a comment.
//
// ## Red-proof, run rather than asserted (2026-08-28)
//
// Two sabotages were applied and run, and both were reverted:
//
//   1. host `CONSOLE_DIR_NAME` changed "cpe-ai-console" → "cpe-ai-console-x" →
//      **1 of 4 RED**, "the two crates agree on the rendezvous directory name", with the message
//      naming both file paths, the constant, and both values.
//   2. `session_diag::log_path` changed back to rebuilding the path with a bare
//      `.join("cpe-ai-console")` → **1 of 4 RED**, "no site rebuilds the path from a bare literal",
//      reporting `session_diag.rs: "cpe-ai-console" appears 1x (allowed 0)`.
//
// A "derivation" that never actually re-reads its source is the same defect with extra steps, so
// these were run rather than reasoned about, and the results are recorded here — at the site — rather
// than only in a PR body.
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { stripRustComments, rustStringLiteralAfter } from "./rustSource";
import { productionCode, productionRustFiles, REPO_ROOT } from "./rustProductionSources";

const SIDECAR_CONSOLE_TEMP_DIR = join(REPO_ROOT, "sidecar", "ai-console", "src", "console_temp_dir.rs");
const HOST_REAPER = join(REPO_ROOT, "sidecar", "host", "src", "reaper.rs");

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
      `CPE-1975: could not find \`${anchor}\` in the Rust source. If the constant was renamed, ` +
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

/** Both files, comment-stripped once. */
function sources() {
  return {
    sidecar: stripRustComments(readFileSync(SIDECAR_CONSOLE_TEMP_DIR, "utf8")),
    host: stripRustComments(readFileSync(HOST_REAPER, "utf8")),
  };
}

describe("CPE-1975 — the AI Console rendezvous path is the same in both crates", () => {
  it("the two crates agree on the rendezvous directory name", () => {
    const { sidecar, host } = sources();
    const fromSidecar = rustConstStr(sidecar, "CONSOLE_DIR_NAME");
    const fromHost = rustConstStr(host, "CONSOLE_DIR_NAME");
    expect(fromHost, `${HOST_REAPER}'s CONSOLE_DIR_NAME must match ${SIDECAR_CONSOLE_TEMP_DIR}'s`).toBe(
      fromSidecar,
    );
  });

  it("the two crates agree on the port file's name", () => {
    const { sidecar, host } = sources();
    const fromSidecar = rustConstStr(sidecar, "PORT_FILE_NAME");
    const fromHost = rustConstStr(host, "PORT_FILE_NAME");
    expect(fromHost, `${HOST_REAPER}'s PORT_FILE_NAME must match ${SIDECAR_CONSOLE_TEMP_DIR}'s`).toBe(
      fromSidecar,
    );
  });

  // A derivation that compares two values it read from the same wrong place would pass while proving
  // nothing, so pin what the value actually is. This is the one hand-written literal in the file, and
  // it is deliberate: if the rendezvous name legitimately changes, BOTH crates and this line must move
  // together, which is the reviewable diff the old "keep them in sync" comment never forced.
  it("and the agreed value is the shipped one", () => {
    const { sidecar } = sources();
    expect(rustConstStr(sidecar, "CONSOLE_DIR_NAME")).toBe("cpe-ai-console");
    expect(rustConstStr(sidecar, "PORT_FILE_NAME")).toBe("session-daemon.port");
  });

  // The whole point of the constants is that nothing rebuilds the path by hand any more — that is how
  // three sites ended up with three copies and only two of them got hardened. A `.join("cpe-…")` with
  // a bare literal anywhere in either crate's source is that defect coming back.
  //
  // Stated blind spot, honestly (CLAUDE.md's "say at least these, never a count"): this catches at
  // LEAST a `.join("cpe-ai-console")` or `.join("session-daemon.port")` written literally in the two
  // crates' `src/`. It does not see a path built from a `format!`, from a `const` under another name,
  // from a byte string, or from concatenation — nor anything outside those two `src/` trees. It is a
  // tripwire for the shape that actually occurred, not a closure of the class.
  it("no site rebuilds the path from a bare literal", () => {
    const files = productionRustFiles();
    // CPE-1932: an enumeration that came back near-empty must fail loudly, never render a green
    // verdict over nothing. The repo has hundreds of tracked `.rs` files.
    expect(files.length).toBeGreaterThan(100);
    const offenders: string[] = [];
    for (const rel of files) {
      const code = productionCode(rel);
      for (const literal of ['"cpe-ai-console"', '"session-daemon.port"']) {
        // The two `pub const` declarations are the one legitimate place each literal appears — one in
        // each crate, because ADR 0001 forbids sharing the constant itself.
        const isDeclaration =
          literal === '"cpe-ai-console"'
            ? /pub const CONSOLE_DIR_NAME/.test(code)
            : /pub const PORT_FILE_NAME/.test(code);
        const count = code.split(literal).length - 1;
        const allowed = isDeclaration ? 1 : 0;
        if (count > allowed) {
          offenders.push(`${rel}: ${literal} appears ${count}x (allowed ${allowed})`);
        }
      }
    }
    expect(
      offenders,
      "CPE-1975: use CONSOLE_DIR_NAME / PORT_FILE_NAME instead of rebuilding the path by hand",
    ).toEqual([]);
  });
});
