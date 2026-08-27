// CPE-1908 round 3, R2-2 (Reviewer): `stripShellComment()`/`logicalLines()` had two gaps that let a
// REAL trailing `#` comment survive un-stripped, reading as live code to a "presence implies
// coverage" consumer like `channelPurityCoverage.test.ts` (see that file's own header comment for why
// this direction is the dangerous one for that kind of ratchet, inverted from this module's original
// framing). This file is the property-test suite for the fix; see `releaseHangHardening.test.ts`'s
// own "logicalLines() handles shell comments and continuations" describe block for the ORIGINAL
// (still-valid) quote-vs-`#`-truncation property tests this module was extracted from.
import { describe, it, expect } from "vitest";
import { stripShellComment, logicalLines } from "./shellScriptLines";
import sharedCases from "./shellScriptLines.cases.json";

/** A byte-for-byte reproduction of `stripShellComment()`'s PRE-R2-2 body (no backslash-escape
 *  handling, no word-boundary rule for opening a quote) -- kept here, not in production code, purely
 *  so the tests below can prove the three demonstrated inputs really did read as "unchanged" under the
 *  old algorithm before asserting the fixed export corrects them. A literal red-then-green, not just
 *  an assertion about current behaviour. */
function preR2StripShellComment(line: string): string {
  let quote: string | null = null;
  for (let i = 0; i < line.length; i += 1) {
    const ch = line[i];
    if (quote !== null) {
      if (ch === quote) quote = null;
      continue;
    }
    if (ch === '"' || ch === "'") {
      quote = ch;
      continue;
    }
    if (ch === "#" && (i === 0 || /\s/.test(line[i - 1]))) return line.slice(0, i);
  }
  return line;
}

describe("stripShellComment() escape handling + quote word-boundary (CPE-1908 round 3, R2-2)", () => {
  // The three inputs the Reviewer demonstrated against this module, verbatim.
  const escapedQuoteComment = 'echo "a \\" b"   # --expect-channel sidecar';
  const contractionComment = "echo don't # --expect-channel sidecar";
  const plainQuotedComment = 'echo "ok" # --expect-channel sidecar';

  it("RED (pre-R2-2 algorithm): a comment after a backslash-escaped quote survives unstripped", () => {
    // The escaped `\"` closes the quote early under the old algorithm; the following bare `"` then
    // opens a NEW unterminated quote that swallows the real trailing comment for the rest of the line.
    expect(preR2StripShellComment(escapedQuoteComment)).toBe(escapedQuoteComment);
  });

  it("GREEN (fixed): the same input has its comment correctly stripped", () => {
    expect(stripShellComment(escapedQuoteComment)).toBe('echo "a \\" b"   ');
  });

  it("RED (pre-R2-2 algorithm): a comment after a mid-word contraction apostrophe survives unstripped", () => {
    // The bare `'` in "don't" opens an unterminated single-quoted string under the old algorithm,
    // swallowing the rest of the line, comment included.
    expect(preR2StripShellComment(contractionComment)).toBe(contractionComment);
  });

  it("GREEN (fixed): the same input has its comment correctly stripped", () => {
    expect(stripShellComment(contractionComment)).toBe("echo don't ");
  });

  it("a genuinely quoted comment (no escape, no mid-word apostrophe) was already stripped correctly, and still is", () => {
    expect(preR2StripShellComment(plainQuotedComment)).toBe('echo "ok" ');
    expect(stripShellComment(plainQuotedComment)).toBe('echo "ok" ');
  });

  it("still does not truncate a `#` inside a genuinely quoted, unescaped value -- the original safe-direction property", () => {
    const line = 'curl --fail --retry 3 -sS -o /tmp/x "https://example.com/a#frag"';
    expect(stripShellComment(line)).toBe(line);
  });
});

describe("logicalLines() skips heredoc bodies (CPE-1908 round 3, R2-1/R2-2)", () => {
  it("a heredoc body crafted to look like a real invocation is never scanned as a logical line", () => {
    const run = [
      "cargo run --locked --manifest-path crates/updater-verify/Cargo.toml --release --bin verify-release-artifacts -- \\",
      "  --conf src-tauri/tauri.conf.json",
      "cat <<'EOF'",
      "cargo run --bin verify-release-artifacts -- --expect-channel sidecar",
      "EOF",
    ].join("\n");
    const lines = logicalLines(run);
    // Exactly the real invocation line, joined across its one continuation -- the heredoc start line,
    // its decoy body line, and its terminator must all be absent.
    expect(lines).toHaveLength(2);
    expect(lines[0]).toContain("--bin verify-release-artifacts --");
    expect(lines[0]).not.toContain("--expect-channel");
    expect(lines[1]).toBe("cat <<'EOF'");
    expect(lines.some((l) => l.includes("--expect-channel sidecar"))).toBe(false);
  });

  it("a real heredoc used for release notes (release-sidecar.yml's own site) does not lose the commands around it", () => {
    const run = [
      'notes_file="$(mktemp)"',
      "cat > \"$notes_file\" <<'EOF'",
      "Build with the sidecar platform + AI Console bundled in.",
      "EOF",
      'gh release create "$TAG" --notes-file "$notes_file"',
    ].join("\n");
    const lines = logicalLines(run);
    expect(lines).toEqual(['notes_file="$(mktemp)"', "cat > \"$notes_file\" <<'EOF'", 'gh release create "$TAG" --notes-file "$notes_file"']);
  });

  it("a here-string (`<<<`) is not mistaken for a heredoc start", () => {
    const run = ['names="a"', 'while IFS= read -r name; do echo "$name"; done <<< "$names"', 'echo done'].join("\n");
    expect(logicalLines(run)).toEqual(['names="a"', 'while IFS= read -r name; do echo "$name"; done <<< "$names"', "echo done"]);
  });
});

// ---------------------------------------------------------------------------------------------
// CPE-1933: the shared cross-language case file.
//
// `crates/updater-verify/src/workflow_scan.rs` is a Rust PORT of this module — the Rust guards that
// scan `.github/workflows/*.yml` cannot import a `.ts` module, so one cross-language copy is
// unavoidable. What is avoidable is the copy quietly diverging, which is the very defect CPE-1933
// exists to kill. So neither side merely *claims* fidelity: both run against
// `shellScriptLines.cases.json`, and that Rust test reads this exact file at run time.
//
// Add a case here and both languages are held to it. If you change behaviour on one side only, the
// other side's suite goes red.
// ---------------------------------------------------------------------------------------------
describe("the shared case file both implementations are held to (CPE-1933)", () => {
  interface SharedCase {
    name: string;
    input: string;
    expected: string[];
  }

  it("is non-empty, so agreement across languages can never be vacuous", () => {
    expect(sharedCases.length).toBeGreaterThanOrEqual(8);
  });

  it.each(sharedCases as SharedCase[])("$name", ({ input, expected }) => {
    expect(logicalLines(input)).toEqual(expected);
  });
});
