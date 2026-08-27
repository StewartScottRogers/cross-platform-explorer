// CPE-1849: shell-comment-and-continuation-aware splitting of a GitHub Actions `run:` block into
// logical lines, extracted from `releaseHangHardening.test.ts` (CPE-1908 round 2) so a second guard
// — `channelPurityCoverage.test.ts` — can reuse the exact same, already-reviewed logic instead of a
// second hand-rolled comment stripper that could disagree with the first one on an edge case. See
// `releaseHangHardening.test.ts`'s own describe block ("logicalLines() handles shell comments and
// continuations") for the property tests proving quote-awareness and continuation-joining; this
// module is deliberately framework-free (no vitest import) so any `.ts` consumer can use it, test or
// otherwise. Moved out of `src/lib/preview/` (CPE-1908 round 3): that directory is exclusively the
// file-preview subsystem, and a CI-workflow shell-line splitter isn't one — nothing else in
// `preview/` enumerates its directory, and this module is only ever imported by test files, so the
// move is a pure rename.
//
// CPE-1908 round 2 (Reviewer, R2-2) corrected the polarity claim below. The ORIGINAL comment on
// `stripShellComment` argued quote-awareness was "load-bearing in the SAFE direction" because a false
// NEGATIVE (truncating real code, hiding a real invocation from the scan) is the dangerous failure —
// true for `releaseHangHardening.test.ts`'s original use ("no apt/apt-get invocation is left
// unhardened": a real, unguarded `curl`/`apt-get` call silently vanishing from the scan is the unsafe
// outcome; a comment or heredoc line wrongly counted as a "found" invocation just fails LOUDLY,
// reported as an extra/mismatched entry).
//
// That polarity is INVERTED for a "presence of a real, live invocation implies coverage" ratchet —
// which is what `channelPurityCoverage.test.ts` is, and what `releaseHangHardening.test.ts`'s own
// "no invocation left unhardened" checks structurally are too (see above): the dangerous failure mode
// there is a false POSITIVE — mistaking something that is NOT a real, live invocation (a shell
// comment, a heredoc body line, decorative text) for one, because that lets the ratchet believe a
// channel/command is "covered" when it structurally is not. A false negative there just makes the
// relevant assertion fail LOUDLY (a channel reported as missing, an invocation count that doesn't
// match) — safe. Concretely, this is CPE-1908 round 2's H1 finding: a `--expect-channel sidecar`
// commented out (or hidden in a heredoc body, or sitting in a quoted `echo` string) must NOT read as
// "coverage" — under-stripping in that direction is what let a 100%-plain manifest under a `-sidecar`
// tag pass the ratchet clean.
//
// So `stripShellComment` and `logicalLines` are held to BOTH directions at once, not one: they must
// never truncate/strip something that is genuinely LIVE code (the original, still-correct concern —
// a real `--expect-channel $CH` interpolation can't be made to disappear this way; it just fails the
// union check loudly if it's genuinely the only guard and gets miscounted, never silently "covered"),
// and they must never leave a genuine comment, heredoc body, or backslash-escaped-quote artifact
// looking like live code. The escape-handling and heredoc-awareness added below close the SECOND
// direction; the original quote-tracking (a `#`/quote char only opens/closes at a real shell boundary,
// never mid-token) already covered the first and is left undisturbed.

/** Strips a shell `#` comment from one line, respecting quotes and backslash escapes. A `#` only
 *  opens a comment when it is unquoted AND starts a word (line start, or preceded by whitespace) —
 *  this is what keeps a real command whose argument carries a literal `#` (a URL fragment, a quoted
 *  value) from being truncated and silently vanishing from a scan (see this module's header comment
 *  for why that direction still matters).
 *
 *  A quote character (`'`/`"`) only OPENS a quoted string at the same kind of boundary — line start,
 *  or preceded by something other than a letter/digit/underscore — so an apostrophe mid-word (a
 *  contraction like "don't" in an `echo` message) is not misread as opening an unterminated quote and
 *  swallowing the rest of the line, comment included (CPE-1908 round 2, R2-2). Inside a quote, a
 *  backslash escapes the next character (real bash: only meaningful inside double quotes, where it
 *  can escape the closing `"`; harmless to also honour inside single quotes, which never contain an
 *  unescaped backslash-quote pair in practice) — without this, a genuinely closed quoted string whose
 *  content contains an escaped quote (`"a \" b"`) was misread as closing early, leaving the rest of
 *  the physical line — including a REAL trailing `# --expect-channel sidecar` comment — stuck "inside"
 *  a phantom unterminated quote and never stripped. That under-stripping is exactly the direction
 *  R2-2 needed closed: a comment that reads as live code is what let a disabled flag still count as
 *  "coverage". */
export function stripShellComment(line: string): string {
  let quote: string | null = null;
  for (let i = 0; i < line.length; i += 1) {
    const ch = line[i];
    if (quote !== null) {
      if (ch === "\\" && quote === '"' && i + 1 < line.length) {
        i += 1; // an escaped char inside a double-quoted string does not end the quote
        continue;
      }
      if (ch === quote) quote = null;
      continue;
    }
    if (ch === "\\" && i + 1 < line.length) {
      i += 1; // a backslash-escaped quote outside any quote is a literal char, not an opener
      continue;
    }
    if (ch === '"' || ch === "'") {
      const prev = i > 0 ? line[i - 1] : undefined;
      if (prev === undefined || !/[A-Za-z0-9_]/.test(prev)) {
        quote = ch;
      }
      continue;
    }
    if (ch === "#" && (i === 0 || /\s/.test(line[i - 1]))) return line.slice(0, i);
  }
  return line;
}

/** Matches a shell heredoc redirection that STARTS a body (`<<DELIM`, `<<'DELIM'`, `<<"DELIM"`,
 *  `<<-DELIM`), never a here-string (`<<<...`, excluded by the `(?<!<)`/`(?!<)` PAIR -- see below).
 *  Group 2 is the delimiter a terminator line must match exactly (trimmed) to close the body.
 *
 *  CPE-1933: the here-string exclusion needs BOTH guards. `(?!<)` alone only stops a match that
 *  begins at the FIRST `<` of `<<<`; the engine then retries from the SECOND one, where `<<`
 *  consumes chars 2-3, the lookahead sees the following space and passes, and the `\1`
 *  backreference closes happily on a quoted word. So `done <<< "names"` -- a here-string whose word
 *  is a quoted LITERAL rather than a `$var` -- opened a phantom heredoc named `names` and swallowed
 *  every subsequent line of the script. `(?<!<)` refuses a match starting one char into a `<<<`.
 *
 *  This was a FALSE NEGATIVE, the direction this module's header calls unsafe for a
 *  presence-implies-coverage ratchet: a real unhardened `apt-get`, or a real `--expect-channel`,
 *  sitting after such a line would drop out of the scan entirely and the guard would report clean.
 *  Latent rather than live -- no such shape exists in the repo today, and the `$names` form at
 *  `release-sidecar.yml:760` never matched (`$` is not `[A-Za-z_]`). Found by the cross-language
 *  oracle added with `shellScriptLines.cases.json`: the Rust port scans `<<` by hand and skipped
 *  the whole `<<<` correctly, so the two halves disagreed and the case file said so. Belongs to
 *  CPE-1936's family (heredoc gaps in this module) -- that ticket's owner can treat this shape as
 *  already closed.
 */
const HEREDOC_START = /(?<!<)<<(?!<)-?\s*(['"]?)([A-Za-z_][A-Za-z0-9_]*)\1/;

/** Splits a `run` script into LOGICAL shell lines: backslash continuations joined, `#` comments
 *  stripped, HEREDOC BODIES skipped entirely, before anything looks for a flag or a value. Without
 *  the continuation join, ordinary multi-line shell formatting (a flag and its value split across a
 *  `\` continuation) evades any scan that requires both on the same PHYSICAL line — see
 *  `releaseHangHardening.test.ts`'s header comment for the real in-repo example this was built to
 *  catch. Without the heredoc skip (CPE-1908 round 2, R2-1/R2-2 — the Reviewer's "heredoc body"
 *  exploit shape): a heredoc body is INERT DATA being fed to a command (`cat <<'EOF' ... EOF`), never
 *  a separately-executed shell statement, so a body line crafted to look exactly like a real
 *  `cargo run ... --bin verify-release-artifacts -- --expect-channel sidecar` invocation must never be
 *  scanned as if it were one — the same false-POSITIVE direction this module's header comment
 *  describes. */
export function logicalLines(run: string | undefined): string[] {
  const out: string[] = [];
  let pending = "";
  let heredocDelim: string | null = null;
  for (const raw of (run ?? "").split("\n")) {
    if (heredocDelim !== null) {
      if (raw.trim() === heredocDelim) heredocDelim = null;
      continue; // heredoc body (and its terminator line) -- data, not a shell statement
    }
    const line = stripShellComment(raw).trim();
    const heredocMatch = line.match(HEREDOC_START);
    if (heredocMatch) heredocDelim = heredocMatch[2];
    if (line.endsWith("\\")) {
      pending += `${line.slice(0, -1).trim()} `;
      continue;
    }
    const joined = (pending + line).trim();
    if (joined) out.push(joined);
    pending = "";
  }
  if (pending.trim()) out.push(pending.trim());
  return out;
}
