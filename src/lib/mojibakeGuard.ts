/**
 * CPE-1771: detection primitives for the repo-wide mojibake guard (`mojibakeGuard.test.ts`).
 *
 * This codebase has hit the same corruption three times: CPE-1752 (`dispatch.rs`), CPE-1733 (caught
 * live via `git diff --numstat` before it landed), and CPE-1771 itself (`src-tauri/Cargo.toml`,
 * `src-tauri/tauri.conf.json`, `CLAUDE.md`). Every occurrence is the same root cause: a UTF-8 file read
 * and re-written through something that treats it as CP1252/Latin-1 (a PowerShell `Get-Content`/
 * `Set-Content` round-trip -- `Set-Content -Encoding utf8` included, since that still writes a BOM and
 * mis-decodes on the read side; see commit `86888aed`, which blocked release 0.57.66 this way). A
 * multi-byte UTF-8 character gets split into its individual bytes, each byte reinterpreted as one
 * CP1252 character, and the result re-encoded as UTF-8. For example an em dash (U+2014, UTF-8 bytes
 * `E2 80 94`) becomes three separate CP1252 characters -- U+00E2, U+20AC, U+201D -- which is why the
 * corrupted text always LOOKS like it starts with the letter "a with circumflex" immediately followed
 * by a currency/typographic symbol. A UTF-8 BOM (bytes `EF BB BF`) goes through the same misreading.
 *
 * Detection strategy: that lead character (U+00E2 "a-circumflex", or U+00C3 "A-tilde", or U+00C2
 * "A-circumflex") is a perfectly ordinary LETTER in plenty of real text -- Romanian, Portuguese, French
 * all use it constantly. CPE-1771's own review caught `src/lib/i18n.ts:5320`'s Portuguese "NAO" (with an
 * A-tilde) as a false positive of a naive "contains U+00C3" scan. The actual mojibake signature is one
 * of those three lead characters immediately followed by one of the CP1252 0x80-0x9F "artifact"
 * characters (curly quotes, em/en dash, ellipsis, dagger, trademark sign, ...) or a non-breaking space --
 * a shape essentially unreachable by real prose, because those artifact characters are not themselves
 * ordinary letters that can follow an accented Latin letter. `mojibakeRegex()` matches exactly that
 * adjacency (lead character + artifact character), not the lead character alone.
 */

/** The characters UTF-8 bytes 0x80-0x9F decode to when misread as CP1252 (skipping the handful of
 *  undefined slots: 0x81, 0x8D, 0x8F, 0x90, 0x9D), plus U+00A0 (non-breaking space, CP1252 0xA0) -- the
 *  "artifact" half of every mojibake pair this repo has produced so far (em dash, ellipsis, apostrophe,
 *  curly quotes, arrow, non-breaking space). Listed by escape, not by literal glyph, so this file never
 *  contains a literal mojibake sequence itself (which would otherwise trip this very guard). */
const CP1252_ARTIFACTS =
  "€‚ƒ„…†‡ˆ‰Š‹ŒŽ" +
  "‘’“”•–—˜™š›œžŸ ";

/** The three Latin-1-supplement "lead" characters a UTF-8-as-CP1252 double-decode always starts with:
 *  U+00C3 (A-tilde), U+00C2 (A-circumflex), U+00E2 (a-circumflex). Listed by escape for the same reason
 *  as {@link CP1252_ARTIFACTS}. */
const MOJIBAKE_LEAD = "ÃÂâ";

/** A fresh mojibake-signature regex (global, so callers can `exec` in a loop without cross-call state
 *  leaking through `lastIndex`). Matches a {@link MOJIBAKE_LEAD} character directly followed by a
 *  {@link CP1252_ARTIFACTS} character. */
export function mojibakeRegex(): RegExp {
  return new RegExp(`[${MOJIBAKE_LEAD}][${CP1252_ARTIFACTS}]`, "gu");
}

export interface MojibakeOffender {
  /** 1-based line number of the match. */
  line: number;
  /** The two-character offending sequence itself. */
  match: string;
}

/** Every mojibake-signature match in `text`, in order, with 1-based line numbers. */
export function findMojibake(text: string): MojibakeOffender[] {
  const re = mojibakeRegex();
  const out: MojibakeOffender[] = [];
  let m: RegExpExecArray | null;
  while ((m = re.exec(text))) {
    const line = text.slice(0, m.index).split("\n").length;
    out.push({ line, match: m[0] });
  }
  return out;
}

/** True if `bytes` opens with a UTF-8 BOM (`EF BB BF`) -- the other half of the same PowerShell
 *  round-trip failure mode (commit `86888aed` added exactly this to `src-tauri/Cargo.toml` and
 *  `src-tauri/tauri.conf.json`, which is what blocked release 0.57.66). */
export function hasLeadingBom(bytes: Uint8Array): boolean {
  return bytes.length >= 3 && bytes[0] === 0xef && bytes[1] === 0xbb && bytes[2] === 0xbf;
}
