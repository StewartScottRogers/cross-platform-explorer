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
 *  "coverage".
 *
 *  KNOWN GAP N9 (CPE-1936, measured 2026-08-27, documented rather than fixed). A line whose quote is
 *  never closed comes back **unchanged**, comment and all: `echo "oops # not stripped` -> itself.
 *  Pinned as a case in `shellScriptLines.cases.json` so both implementations answer it the same way
 *  and a future fix has to update the case deliberately.
 *
 *  Not fixed because the obvious fix is worse than the gap. "Treat an unterminated quote as if the
 *  opener were a literal" would strip a `#` out of the FIRST line of a genuinely multi-line quoted
 *  string — legal bash, and this splitter is line-at-a-time, so every such string's opening line
 *  looks exactly like this. That is truncating live code, the direction the module header calls
 *  unsafe. The gap's own direction is under-stripping (a comment reading as live code), which is
 *  bounded here: an unterminated quote is a shell syntax error in its own right, the consumers match
 *  their anchors per logical line rather than over a joined step, and nothing in the tree has the
 *  shape. If it ever bites, the fix is to track quote state ACROSS lines, not to guess per line. */
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
 *
 *  CPE-1936 replaced the regex with the scanner below. A regex cannot be told about QUOTE STATE, and
 *  that was N8: `echo "use <<EOF to start a heredoc"` opened a phantom heredoc named `EOF` and every
 *  following line of the step vanished from the scan. Measured before the fix (this exact input is
 *  case "CPE-1936 N8" in the shared case file):
 *
 *      echo "use <<EOF to start a heredoc"
 *      cargo run --bin verify-release-artifacts -- --expect-channel sidecar
 *      echo tail
 *      -> ["echo \"use <<EOF to start a heredoc\""]          <- two real lines gone
 *
 *  That is the FALSE-NEGATIVE direction — `releaseHangHardening.test.ts`'s "no `apt`/`curl` left
 *  unhardened" scan simply stops seeing the unhardened command. Unlike the `<<<` shape above this one
 *  is NOT latent: `ffmpeg-pin-freshness.yml` writes GitHub multi-line outputs with
 *  `echo "failures<<PINFAIL_EOF" >> "$GITHUB_OUTPUT"`, which is exactly this shape, and the Rust
 *  whole-file consumers scan that workflow.
 */
interface HeredocOpener {
  /** The word a terminator line must equal to close the body. */
  delim: string;
  /** True for the `<<-` form, whose terminator may be indented (bash strips leading TABS from it). */
  dashed: boolean;
}

/** Finds the first heredoc redirection on a line that STARTS a body (`<<DELIM`, `<<'DELIM'`,
 *  `<<"DELIM"`, `<<-DELIM`) — never a here-string (`<<<`), and never a `<<` that is inside a quoted
 *  string. Character-by-character with the SAME quote/escape rules as `stripShellComment` above, so
 *  the two agree on where a quoted region starts and ends; a regex was tried first and could not
 *  express the quote-state part (see `HEREDOC_START`'s history in the comment above).
 *
 *  Deliberately NOT closed, measured 2026-08-27, each documented rather than fixed because none
 *  exists in the tree today and every fix here has a failure direction of its own:
 *
 *   - **Arithmetic left-shift**: `$(( a << b ))` reads as a heredoc named `b` (`x << 2` does not —
 *     `2` is not an identifier start). Suppressing `<<` inside `$(( … ))` needs depth tracking that
 *     a plain `( (cmd) )` subshell would false-trigger, trading a false negative for a false
 *     positive. No arithmetic `<<` in any workflow today.
 *   - **Two heredocs on one line**: `cat <<A <<B` opens both bodies in bash; only `A` is tracked
 *     here, so `B`'s body is scanned as live code after `A` closes. Same as the pre-CPE-1936
 *     behaviour, not a regression.
 *   - **A partially quoted delimiter**: bash reads `<<E"OF"` as `EOF`; this reads it as `E`.
 *
 *  Ported to `heredoc_delimiter()` in `crates/updater-verify/src/workflow_scan.rs`; both are run
 *  against `shellScriptLines.cases.json`, so a change on one side alone turns the other side red. */
function heredocOpener(line: string): HeredocOpener | null {
  let quote: string | null = null;
  let i = 0;
  while (i < line.length) {
    const ch = line[i];
    if (quote !== null) {
      if (ch === "\\" && quote === '"' && i + 1 < line.length) {
        i += 2; // an escaped char inside a double-quoted string does not end the quote
        continue;
      }
      if (ch === quote) quote = null;
      i += 1;
      continue;
    }
    if (ch === "\\" && i + 1 < line.length) {
      i += 2; // a backslash-escaped quote outside any quote is a literal char, not an opener
      continue;
    }
    if (ch === '"' || ch === "'") {
      const prev = i > 0 ? line[i - 1] : undefined;
      if (prev === undefined || !/[A-Za-z0-9_]/.test(prev)) quote = ch;
      i += 1;
      continue;
    }
    if (ch !== "<" || line[i + 1] !== "<") {
      i += 1;
      continue;
    }
    let j = i + 2;
    if (line[j] === "<") {
      // A here-STRING feeds one word to stdin and opens no body. Skipping past ALL THREE `<` is the
      // scanner's equivalent of the `(?<!<)` half of the old regex's exclusion pair: without it the
      // walk would resume at the second `<` and read `<< "names"` as a heredoc opener (CPE-1933).
      i = j + 1;
      continue;
    }
    let dashed = false;
    if (line[j] === "-") {
      dashed = true;
      j += 1;
    }
    while (j < line.length && /\s/.test(line[j])) j += 1;
    let opener: string | null = null;
    if (line[j] === "'" || line[j] === '"') {
      opener = line[j];
      j += 1;
    }
    const start = j;
    if (j < line.length && /[A-Za-z_]/.test(line[j])) {
      j += 1;
      while (j < line.length && /[A-Za-z0-9_]/.test(line[j])) j += 1;
      // A quoted delimiter must be closed by the same quote, exactly as the old regex's `\1`
      // backreference required.
      if (opener === null || line[j] === opener) return { delim: line.slice(start, j), dashed };
    }
    i += 2;
  }
  return null;
}

/** True when `raw` is the terminator line for the open heredoc `h`.
 *
 *  CPE-1936 N7: the old test was `raw.trim() === delim`, which let an INDENTED line close a plain
 *  `<<EOF`. Real bash only accepts the delimiter alone on the line; only `<<-` tolerates indentation
 *  (and only leading tabs). Measured before the fix (case "CPE-1936 N7"):
 *
 *      cat <<EOF
 *        EOF                                                   <- body in bash, terminator here
 *      cargo run --bin verify-release-artifacts -- --expect-channel sidecar
 *      EOF
 *      echo after
 *      -> ["cat <<EOF", "cargo run … --expect-channel sidecar", "EOF", "echo after"]
 *
 *  — a heredoc BODY line read as a live invocation, the false-POSITIVE direction that lets a ratchet
 *  believe a channel is covered when it structurally is not.
 *
 *  The rule is bash's, RELATIVE to the opener's own indentation rather than to column 0, because this
 *  splitter is also handed text that is uniformly indented: `release_workflow_wiring.rs` runs it over
 *  a whole `.yml` FILE, where `release-sidecar.yml`'s `cat > "$notes_file" <<'EOF'` and its `EOF` both
 *  sit ten spaces in. Requiring column 0 there would leave that heredoc open for the rest of the file
 *  and empty the scan — the worst possible direction. So: the terminator must be the delimiter alone,
 *  indented no more than the line that opened it. For a genuine shell script (opener at column 0) that
 *  IS bash's rule exactly. The one shape it still accepts that bash would not is a terminator indented
 *  less than an already-indented opener (`if …; then` + an indented `cat <<EOF`); closing early there
 *  is the pre-existing behaviour and no such shape exists in the tree. */
function closesHeredoc(raw: string, h: HeredocOpener & { indent: number }): boolean {
  const body = raw.replace(/\r$/, "");
  const indent = /^[ \t]*/.exec(body)![0];
  if (body.slice(indent.length).replace(/\s+$/, "") !== h.delim) return false;
  return h.dashed || indent.length <= h.indent;
}

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
  let heredoc: (HeredocOpener & { indent: number }) | null = null;
  for (const raw of (run ?? "").split("\n")) {
    if (heredoc !== null) {
      if (closesHeredoc(raw, heredoc)) heredoc = null;
      continue; // heredoc body (and its terminator line) -- data, not a shell statement
    }
    const line = stripShellComment(raw).trim();
    const opener = heredocOpener(line);
    // The opener's OWN indentation is measured on the raw physical line, not on the trimmed one --
    // `closesHeredoc` compares the terminator's indent against it (see its comment for why relative).
    if (opener) heredoc = { ...opener, indent: /^[ \t]*/.exec(raw)![0].length };
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
