// CPE-1849: shell-comment-and-continuation-aware splitting of a GitHub Actions `run:` block into
// logical lines, extracted from `releaseHangHardening.test.ts` (CPE-1908 round 2) so a second guard
// — `channelPurityCoverage.test.ts` — can reuse the exact same, already-reviewed logic instead of a
// second hand-rolled comment stripper that could disagree with the first one on an edge case. See
// `releaseHangHardening.test.ts`'s own describe block ("logicalLines() handles shell comments and
// continuations") for the property tests proving quote-awareness and continuation-joining; this
// module is deliberately framework-free (no vitest import) so any `.ts` consumer can use it, test or
// otherwise.

/** Strips a shell `#` comment from one line, respecting quotes. A `#` only opens a comment when it
 *  is unquoted AND starts a word (line start, or preceded by whitespace). The quote-awareness is
 *  load-bearing in the SAFE direction: a naive "cut at the first #" would truncate a real command
 *  whose argument carries a `#` (a URL fragment, a quoted value), hiding it from the scan entirely —
 *  a SILENT false negative, the dangerous direction. */
export function stripShellComment(line: string): string {
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

/** Splits a `run` script into LOGICAL shell lines: backslash continuations joined, `#` comments
 *  stripped, before anything looks for a flag or a value. Without the join, ordinary multi-line shell
 *  formatting (a flag and its value split across a `\` continuation) evades any scan that requires
 *  both on the same PHYSICAL line — see `releaseHangHardening.test.ts`'s header comment for the real
 *  in-repo example this was built to catch. */
export function logicalLines(run: string | undefined): string[] {
  const out: string[] = [];
  let pending = "";
  for (const raw of (run ?? "").split("\n")) {
    const line = stripShellComment(raw).trim();
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
