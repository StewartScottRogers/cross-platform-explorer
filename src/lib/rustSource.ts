/**
 * Reading facts out of Rust source, for the guards that derive a provenance claim instead of asserting
 * one (CLAUDE.md → "Derive provenance, don't claim it").
 *
 * These two functions were written for `src/lib/components/MacroRunConfirm.test.ts` (CPE-1933, PR
 * #1056 Finding 2) and lived inside it. CPE-1950 needed the same two for
 * `sidecarBundleResources.test.ts` and `RepoBrowser.test.ts`, and this repo has already written four
 * separate hand-rolled strippers before the fifth was caught — so they live here now, with one set of
 * tests (`rustSource.test.ts`), and every Rust-source scanner imports them rather than re-deriving
 * the escape and comment rules. There is no Rust port of this module, so nothing here is pinned by a
 * shared case file the way `shellScriptLines.ts` is; it is a reader, not a reimplementation.
 */

/**
 * Blanks Rust line comments and block comments, preserving every offset (comment bytes become spaces)
 * so indices into the result still address the original file.
 *
 * CPE-1933. Anchoring a scan on "the first `format!(` after the fn" (or "the first `&[` after the
 * const") is beaten **silently** by a comment sitting between the signature and the real code that
 * contains the same token and quotes the OLD value: the extractor reads the comment, the fixture still
 * matches it, and the derivation certifies something the code no longer says — the whole purpose,
 * inverted. Every other adversarial shape a Reviewer tried failed loudly; that one did not.
 *
 * Stripping comments before scanning kills the class rather than that one shape, and is the same rule
 * `crates/updater-verify/src/workflow_scan.rs` applies to workflows: **anchor on code, never on text a
 * comment can also contain.** Quote-aware, so a `//` inside a string literal (a URL in a message) is
 * left alone.
 *
 * Known limitation, deliberate: this tracks `"` string literals but not Rust CHAR literals, so a `'"'`
 * sitting before the target would open a phantom string and swallow what follows. The failure
 * direction is **loud** — the extractor then finds the wrong item or none at all, and the caller's
 * equality assertion fails — never the silent wrong-value pass this stripping exists to prevent. Add
 * char-literal handling if such a literal ever appears in a scanned file, rather than trusting this
 * note. Raw strings (`r#"…"#`) are not handled either, and fail the same loud way.
 */
export function stripRustComments(src: string): string {
  const out = src.split("");
  let i = 0;
  let quote: '"' | null = null;
  while (i < src.length) {
    const ch = src[i];
    if (quote) {
      if (ch === "\\") {
        i += 2;
        continue;
      }
      if (ch === quote) quote = null;
      i += 1;
      continue;
    }
    if (ch === '"') {
      quote = '"';
      i += 1;
      continue;
    }
    if (ch === "/" && src[i + 1] === "/") {
      while (i < src.length && src[i] !== "\n") {
        out[i] = " ";
        i += 1;
      }
      continue;
    }
    if (ch === "/" && src[i + 1] === "*") {
      const end = src.indexOf("*/", i + 2);
      const stop = end < 0 ? src.length : end + 2;
      for (let j = i; j < stop; j += 1) if (out[j] !== "\n") out[j] = " ";
      i = stop;
      continue;
    }
    i += 1;
  }
  return out.join("");
}

/**
 * Reads the Rust string literal starting at the first `"` at or after `fromIndex`, resolving the
 * escapes that actually appear in this repo's literals: `\"`, `\\`, `\n`, `\t`, and — the one that
 * matters — Rust's `\`-at-end-of-line continuation, which swallows the newline AND the next line's
 * indentation. A naive join gets that last one wrong and produces a string with the source's leading
 * spaces embedded in it.
 */
export function rustStringLiteralAfter(src: string, fromIndex: number): string {
  const start = src.indexOf('"', fromIndex);
  if (start < 0) throw new Error("no string literal found");
  let out = "";
  for (let i = start + 1; i < src.length; ) {
    const ch = src[i];
    if (ch === "\\") {
      const next = src[i + 1];
      if (next === "\n" || next === "\r") {
        i += 1;
        while (i < src.length && /\s/.test(src[i])) i += 1;
        continue;
      }
      out += next === "n" ? "\n" : next === "t" ? "\t" : next;
      i += 2;
      continue;
    }
    if (ch === '"') return out;
    out += ch;
    i += 1;
  }
  throw new Error("unterminated Rust string literal");
}

/**
 * Every string literal inside the `&[ … ]` slice literal that follows `anchor` in `src` — e.g. the
 * elements of a `pub const FOO: &[&str] = &["a", "b"];`.
 *
 * `src` must already be comment-stripped ([`stripRustComments`]); passing raw source is exactly the
 * hole that stripping exists to close. Throws if the anchor or the slice is missing, so a renamed
 * const reds loudly rather than deriving an empty list that vacuously matches nothing.
 */
export function rustStrSliceAfter(src: string, anchor: string): string[] {
  const at = src.indexOf(anchor);
  if (at < 0) throw new Error(`anchor not found in Rust source: ${anchor}`);
  // Start after the `=`, never at the first `&[` — the TYPE is written `&[&str]`, so scanning from
  // the anchor lands on the type's own brackets and derives an empty list. (Measured: it did.)
  const eq = src.indexOf("=", at);
  const semi = src.indexOf(";", at);
  const open = eq < 0 ? -1 : src.indexOf("&[", eq);
  if (open < 0 || (semi >= 0 && open > semi)) {
    throw new Error(`no &[…] slice literal follows ${anchor}`);
  }
  const close = src.indexOf("]", open);
  if (close < 0) throw new Error(`unterminated slice literal after ${anchor}`);
  const items: string[] = [];
  let i = open + 2;
  while (i < close) {
    const q = src.indexOf('"', i);
    if (q < 0 || q > close) break;
    const value = rustStringLiteralAfter(src, q);
    items.push(value);
    // Skip past the closing quote of the literal we just read. The literal is escape-free in every
    // slice this is used on, but walk with the same escape rule anyway so a `\"` cannot desync us.
    let j = q + 1;
    for (;;) {
      if (src[j] === "\\") {
        j += 2;
        continue;
      }
      if (src[j] === '"' || j >= close) break;
      j += 1;
    }
    i = j + 1;
  }
  if (items.length === 0) throw new Error(`slice literal after ${anchor} held no string literals`);
  return items;
}
