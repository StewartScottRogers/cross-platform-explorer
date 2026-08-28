/**
 * "Every production Rust file in this repo, with its comments and its unit-test module removed" —
 * the enumerator CPE-1964 corrected, lifted out of `tempDirSites.test.ts` so the second guard that
 * needed it (CPE-1975's `consoleTempDirPath.test.ts`) reuses it instead of growing a second copy.
 *
 * CPE-1950's rule: where the duplication is removable, remove it. Pinning two copies to a shared case
 * file proves they agree, not that either is right; one implementation cannot disagree with itself.
 *
 * The recipe is CPE-1964's, and its history is the reason it is written down as code rather than as a
 * comment: CPE-1952's hand-written version ("`git ls-files '*.rs'`, minus `tests/`, minus everything
 * after each file's first `#[cfg(test)]`") also matched the **indented** attribute on an in-function
 * test module and the token inside a doc comment, so run literally it amputated production code and
 * found 10 sites where the corrected recipe finds 14 — under-counting the very defect it was
 * enumerating, while reading as a measurement.
 *
 *   1. `git ls-files '*.rs'` — the tree decides what exists, not a list someone typed;
 *   2. drop integration-test files (a `tests/` path segment);
 *   3. strip comments FIRST, so a doc comment quoting the token cannot move or fake a hit;
 *   4. cut each file at its first **column-0** `#[cfg(test)]`, keeping the indented in-function form
 *      out of it.
 */
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { stripRustComments } from "./rustSource";

/** The repository root, from this module's own location. */
export const REPO_ROOT = resolve(__dirname, "..", "..");

/**
 * A tracked Rust file's production half: comments blanked, then everything from the first column-0
 * `#[cfg(test)]` onward dropped.
 *
 * ## The fallback, and why it is safe *for this shape of guard*
 *
 * `stripRustComments` ends with a tripwire — "no line may still begin with `//`" — that catches a
 * desync of any cause (CPE-1950). Three files in this repo trip it without being desynced at all,
 * because a `//` line legitimately *survives* when it lives inside a string literal:
 * `crates/server/src/net_share.rs` (a `/proc/mounts` fixture whose CIFS row starts `//fileserver/…`,
 * in a backslash-continued string), `sidecar/host/src/scaffold.rs` (an `r#"…"#` template of a
 * generated `main.rs`, doc comments and all) and `sidecar/agent-board/src/ui.rs` (an `r#"…"#` page
 * with inline JS). The strip is correct in all three; the invariant is stricter than the property.
 *
 * Fixing that belongs to `rustSource.ts`, not to a caller (its doc says so in as many words), so this
 * enumerator falls back to the **raw** source for such a file. That is conservative in the only
 * direction that matters for a "does this token appear anywhere?" guard: raw text is a superset of
 * stripped text, so such an assertion can only gain matches, never lose them — a comment could make
 * one of these three files red, never green. It would be the **wrong** fallback for a guard that
 * reads a *value* out of Rust; those must let the throw propagate.
 */
export function productionCode(rel: string): string {
  const raw = readFileSync(join(REPO_ROOT, rel), "utf8");
  let stripped: string;
  try {
    stripped = stripRustComments(raw);
  } catch {
    stripped = raw;
  }
  const cut = stripped.search(/^#\[cfg\(test\)\]/m);
  return cut === -1 ? stripped : stripped.slice(0, cut);
}

/** Every tracked `.rs` file that is not an integration test. */
export function productionRustFiles(): string[] {
  return execFileSync("git", ["ls-files", "*.rs"], {
    cwd: REPO_ROOT,
    encoding: "utf8",
    maxBuffer: 64 << 20,
  })
    .split("\n")
    .map((l) => l.trim().split("\\").join("/"))
    .filter(Boolean)
    .filter((p) => !p.split("/").includes("tests"));
}
