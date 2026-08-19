/**
 * CPE-1771: detection primitives for the repo-wide mojibake guard (`mojibakeGuard.test.ts`).
 *
 * This codebase has hit the same corruption three times: CPE-1752 (`dispatch.rs`), CPE-1733 (caught
 * live via `git diff --numstat` before it landed), and CPE-1771 itself (`src-tauri/Cargo.toml`,
 * `src-tauri/tauri.conf.json`, `CLAUDE.md`). Every occurrence is the same root cause: a UTF-8 file read
 * and re-written through something that treats it as CP1252/Latin-1 (a PowerShell `Get-Content`/
 * `Set-Content` round-trip -- `Set-Content -Encoding utf8` included, since that still writes a BOM and
 * mis-decodes on the read side; see commit `86888aed`, which blocked release 0.57.66 this way). Every
 * byte of a multi-byte UTF-8 character gets reinterpreted as one CP1252 character, and the result is
 * re-encoded as UTF-8, so an em dash (U+2014, UTF-8 bytes `E2 80 94`) becomes three separate CP1252
 * characters. A UTF-8 BOM (bytes `EF BB BF`) goes through the same misreading and can itself reappear as
 * three ordinary-looking characters in already-corrupted text.
 *
 * Round 1 of this guard (attempt 1 of this ticket) matched a hand-picked table of "lead character
 * followed by a specific artifact character" pairs -- the shapes actually seen at the time (em dash,
 * ellipsis, arrow). An independent review (attempt 2) proved that table catches only 13 of 34 real
 * mojibake shapes: it missed ordinary double-encoded accented Latin (cafe/e-acute, n-tilde, u-diaeresis,
 * ...), the whole CP1252-symbol block (copyright, degree, plus-minus, guillemets, ...), and every
 * non-Latin script (Greek, Cyrillic, Hebrew, Arabic, CJK, kana, Hangul) plus emoji -- exactly the shapes
 * this repo's own bidi/CJK test fixtures and docs are most likely to contain next.
 *
 * Round 2 (this file) replaces the pair table with what it always should have been: an actual UTF-8
 * round-trip validator. A CP1252-decoded byte sequence is "mojibake" precisely when re-encoding the
 * decoded characters back to bytes yields a sequence that is ALSO valid UTF-8 for some single Unicode
 * code point -- i.e. two different, unrelated interpretations of the same bytes both parse cleanly, which
 * essentially never happens by chance in real prose. This needs no hand-picked shape list, so it can't
 * miss a shape by omission the way the pair table did.
 */

/** CP1252 byte value (0x00-0xFF) -> the Unicode code point it decodes to. Bytes 0x00-0x7F and 0xA0-0xFF
 *  are identical to their code point (CP1252 matches ASCII and Latin-1 there). Bytes 0x80-0x9F are
 *  CP1252's own symbol block (curly quotes, dashes, dagger, trademark, ...); the five slots CP1252
 *  leaves undefined there (0x81, 0x8D, 0x8F, 0x90, 0x9D) decode, per the real-world CP1252 decoders that
 *  actually produce this corruption (.NET's among them), to a C1 control code point equal to the byte's
 *  own value -- which the identity default below already gives them. */
function buildCp1252DecodeTable(): number[] {
  const table = new Array<number>(256);
  for (let b = 0; b <= 0xff; b++) table[b] = b;
  const symbols: Record<number, number> = {
    0x80: 0x20ac,
    0x82: 0x201a,
    0x83: 0x0192,
    0x84: 0x201e,
    0x85: 0x2026,
    0x86: 0x2020,
    0x87: 0x2021,
    0x88: 0x02c6,
    0x89: 0x2030,
    0x8a: 0x0160,
    0x8b: 0x2039,
    0x8c: 0x0152,
    0x8e: 0x017d,
    0x91: 0x2018,
    0x92: 0x2019,
    0x93: 0x201c,
    0x94: 0x201d,
    0x95: 0x2022,
    0x96: 0x2013,
    0x97: 0x2014,
    0x98: 0x02dc,
    0x99: 0x2122,
    0x9a: 0x0161,
    0x9b: 0x203a,
    0x9c: 0x0153,
    0x9e: 0x017e,
    0x9f: 0x0178,
  };
  for (const [byte, codepoint] of Object.entries(symbols)) table[Number(byte)] = codepoint;
  return table;
}

/** byte (0-255) -> code point it decodes to as CP1252. */
const CP1252_DECODE = buildCp1252DecodeTable();

/** code point -> the single CP1252 byte that decodes to it (the exact inverse of {@link CP1252_DECODE};
 *  every entry in that table is a distinct code point, so this inverse is unambiguous). Any code point
 *  CP1252 cannot produce (Greek, Cyrillic, Hebrew, CJK, emoji, an arrow, ...) is simply absent -- which is
 *  exactly right: a genuine CP1252 misread can never have produced that character in the first place. */
const CP1252_ENCODE = new Map<number, number>(CP1252_DECODE.map((codepoint, byte) => [codepoint, byte]));

/** Simulates the exact corruption this guard exists to catch: encodes `text` as UTF-8 bytes, then
 *  decodes each byte as if it were CP1252 (one byte, one character) -- precisely what a PowerShell
 *  `Get-Content`/`Set-Content` round-trip does to a real UTF-8 file. Used to generate genuine mojibake
 *  from genuine text for this module's own tests, rather than hand-typing corrupted literals (which is
 *  exactly the mistake that briefly made an earlier draft of this very file trip its own guard). */
export function simulateCp1252Corruption(text: string): string {
  const utf8 = new TextEncoder().encode(text);
  let out = "";
  for (const byte of utf8) out += String.fromCodePoint(CP1252_DECODE[byte]);
  return out;
}

/** A single `TextDecoder` reused across calls -- `decode()` without `{ stream: true }` is stateless per
 *  call, so sharing one instance is safe. `fatal: true` rejects anything that isn't a strictly valid,
 *  minimal-length UTF-8 encoding (overlong sequences, lone surrogates, truncated sequences, ...) instead
 *  of silently substituting U+FFFD -- exactly the "valid UTF-8 sequence" half of the round-trip check.
 *  `ignoreBOM: true` stops the decoder from silently discarding a leading `EF BB BF`/`FF FE`/`FE FF` and
 *  returning "" for it -- without this, a mojibake'd BOM (three characters: U+00EF, U+00BB, U+00BF,
 *  themselves valid UTF-8 for the three bytes that spell the real BOM) would decode to nothing and
 *  vanish instead of being reported. */
const STRICT_UTF8_DECODER = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true });

/** Decodes `bytes` as UTF-8, returning the resulting string only if it is valid UTF-8 that decodes to
 *  EXACTLY one Unicode code point (counted by code point, not UTF-16 unit, so a valid astral character
 *  decoded as a surrogate pair still counts as one). Returns null for anything invalid or multi-codepoint. */
function decodeSingleCodepointUtf8(bytes: number[]): string | null {
  let decoded: string;
  try {
    decoded = STRICT_UTF8_DECODER.decode(Uint8Array.from(bytes));
  } catch {
    return null;
  }
  return [...decoded].length === 1 ? decoded : null;
}

/** How many UTF-8 continuation bytes a valid lead byte declares, by range. Only 0xC2-0xF4 are valid
 *  UTF-8 lead bytes for a real (non-overlong, non-surrogate, <= U+10FFFF) code point; 0xF5-0xFF and the
 *  ASCII/continuation ranges are never lead bytes at all. `decodeSingleCodepointUtf8`'s strict decode
 *  still re-validates every other UTF-8 constraint (overlong encodings, surrogate code points, and the
 *  E0/ED/F0/F4 second-byte range restrictions) -- this table only bounds how many characters to look at. */
function continuationCount(leadByte: number): number {
  if (leadByte >= 0xc2 && leadByte <= 0xdf) return 1;
  if (leadByte >= 0xe0 && leadByte <= 0xef) return 2;
  if (leadByte >= 0xf0 && leadByte <= 0xf4) return 3;
  return 0;
}

export interface MojibakeOffender {
  /** 1-based line number of the match. */
  line: number;
  /** The offending substring, as it actually appears in the (corrupted) text. */
  match: string;
}

/** Scans `text` for the mojibake signature: a run of 2-4 characters that (a) each individually decode
 *  from a single CP1252 byte, AND (b) whose CP1252 byte values, read back together, form a strictly
 *  valid UTF-8 encoding of exactly one Unicode code point. Matches are non-overlapping (a consumed match
 *  is not re-scanned), and line numbers are tracked in one forward pass -- no lead/artifact table, no
 *  hand-picked shape list, so no shape can be missed by omission the way round 1's table missed almost
 *  everything outside the accented-Latin-adjacent-to-a-CP1252-symbol shape it was built from. */
export function findMojibake(text: string): MojibakeOffender[] {
  const out: MojibakeOffender[] = [];
  let i = 0;
  let line = 1;
  while (i < text.length) {
    const ch = text.charCodeAt(i);
    if (ch === 0x0a) {
      line++;
      i++;
      continue;
    }
    const leadByte = CP1252_ENCODE.get(ch);
    const need = leadByte === undefined ? 0 : continuationCount(leadByte);
    if (leadByte !== undefined && need > 0) {
      const bytes = [leadByte];
      let ok = true;
      for (let k = 1; k <= need; k++) {
        const contByte = CP1252_ENCODE.get(text.charCodeAt(i + k));
        if (contByte === undefined || contByte < 0x80 || contByte > 0xbf) {
          ok = false;
          break;
        }
        bytes.push(contByte);
      }
      if (ok && decodeSingleCodepointUtf8(bytes) !== null) {
        out.push({ line, match: text.slice(i, i + 1 + need) });
        // None of a matched span's characters can be U+000A (CP1252 byte 0x0A is ASCII "\n", which is
        // never a valid lead or continuation byte per continuationCount/the 0x80-0xBF check above), so
        // jumping past the whole match without re-checking for embedded newlines is safe.
        i += 1 + need;
        continue;
      }
    }
    i++;
  }
  return out;
}

/** True if `bytes` opens with a UTF-8 BOM (`EF BB BF`) -- the other half of the same PowerShell
 *  round-trip failure mode (commit `86888aed` added exactly this to `src-tauri/Cargo.toml` and
 *  `src-tauri/tauri.conf.json`, which is what blocked release 0.57.66). A BOM that has ITSELF been
 *  through the round-trip no longer has these bytes at offset 0 -- `findMojibake` catches that shape as
 *  ordinary mojibake text instead (see the `ignoreBOM` note above and this file's "mojibake'd BOM" test). */
export function hasLeadingBom(bytes: Uint8Array): boolean {
  return bytes.length >= 3 && bytes[0] === 0xef && bytes[1] === 0xbb && bytes[2] === 0xbf;
}
