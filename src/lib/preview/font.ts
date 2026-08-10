/**
 * Font preview helpers (CPE-1586, epic CPE-1568 slice 5): pure, unit-testable helpers behind the font
 * glyph grid + specimen view — capping the grid (STREAMING.md / PURPOSE.md fast/small/predictable) and a
 * lightweight sfnt (`name`/`maxp` table) metadata sniff for when the browser's own `FontFace` API can't
 * report a font's internal family/style/version (it only ever echoes back what THE CALLER assigned it,
 * never the font file's own name-table strings — see {@link parseSfntMetadata}'s doc comment).
 *
 * No new dependency: TrueType/OpenType (uncompressed sfnt) tables are read directly off the raw bytes
 * `FontPreview.svelte` already fetches for the specimen; WOFF/WOFF2 (compressed containers) are sniffed by
 * format only and otherwise degrade gracefully rather than pulling in a decompression/font-parsing library.
 */

/** Codepoints sampled for the glyph grid: printable Basic Latin (U+0021–U+007E, skipping the invisible
 *  space at U+0020) plus Latin-1 Supplement (U+00A1–U+00FF, skipping the invisible NBSP at U+00A0) — the
 *  common range most Latin-script fonts cover. This is a fixed SAMPLE, not the font's actual cmap:
 *  enumerating a font's real glyph coverage would need a cmap-table parse this module deliberately
 *  doesn't do (out of scope for a lean preview — see {@link parseSfntMetadata}). A glyph the font doesn't
 *  define simply falls back to the browser's normal font-substitution behaviour when rendered, same as
 *  any other missing-glyph case on the web platform. */
export const FONT_GLYPH_CANDIDATES: number[] = (() => {
  const cps: number[] = [];
  for (let cp = 0x21; cp <= 0x7e; cp++) cps.push(cp);
  for (let cp = 0xa1; cp <= 0xff; cp++) cps.push(cp);
  return cps;
})();

/** Hard cap on glyph-grid cells actually rendered — an arbitrarily large candidate list can never stall
 *  the pane with thousands of DOM nodes (STREAMING.md / PURPOSE.md fast/small/predictable). */
export const GLYPH_GRID_CAP = 200;

export interface GlyphGrid {
  /** The codepoints to actually render — at most `cap` entries. */
  shown: number[];
  /** The full candidate count before capping. */
  total: number;
  /** Whether `shown` is a strict prefix of the input (i.e. some candidates were dropped). */
  truncated: boolean;
}

/** Cap `codepoints` to at most `cap` entries, reporting whether it was truncated. The pane renders only
 *  `shown` — never the full `codepoints` list — so a huge candidate set can't stall it. */
export function capGlyphs(codepoints: number[], cap: number = GLYPH_GRID_CAP): GlyphGrid {
  const truncated = codepoints.length > cap;
  return { shown: truncated ? codepoints.slice(0, cap) : codepoints, total: codepoints.length, truncated };
}

/** The actual character a codepoint renders as. */
export function glyphChar(cp: number): string {
  return String.fromCodePoint(cp);
}

/** `U+0041`-style label for a codepoint (uppercase hex, zero-padded to at least 4 digits). */
export function codepointLabel(cp: number): string {
  return `U+${cp.toString(16).toUpperCase().padStart(4, "0")}`;
}

export type FontFormat = "TrueType" | "OpenType" | "WOFF" | "WOFF2" | "Unknown";

/** Sniff the font container format from its magic bytes — cheap and works even when
 *  {@link parseSfntMetadata} can't go further for a compressed WOFF/WOFF2 container. */
export function sniffFontFormat(bytes: Uint8Array): FontFormat {
  if (bytes.length < 4) return "Unknown";
  const tag = ((bytes[0] << 24) | (bytes[1] << 16) | (bytes[2] << 8) | bytes[3]) >>> 0;
  if (tag === 0x774f4646) return "WOFF"; // 'wOFF'
  if (tag === 0x774f4632) return "WOFF2"; // 'wOF2'
  if (tag === 0x4f54544f) return "OpenType"; // 'OTTO' (CFF-flavored OpenType)
  if (tag === 0x00010000 || tag === 0x74727565 /* 'true' */) return "TrueType";
  if (tag === 0x74746366 /* 'ttcf' */) return "TrueType"; // TrueType Collection — use the first font
  return "Unknown";
}

/** Human label for a font's format before the byte sniff has resolved (or if it fails) — falls back to
 *  the file extension, which is always known immediately from the selected entry. */
export function formatLabelForExt(ext: string): FontFormat {
  switch (ext.toLowerCase()) {
    case "ttf":
      return "TrueType";
    case "otf":
      return "OpenType";
    case "woff":
      return "WOFF";
    case "woff2":
      return "WOFF2";
    default:
      return "Unknown";
  }
}

export interface SfntMetadata {
  family: string | null;
  style: string | null;
  version: string | null;
  numGlyphs: number | null;
}

/**
 * Read the `name` (family/style/version strings) and `maxp` (glyph count) tables directly out of an
 * UNCOMPRESSED sfnt container (TrueType/OpenType/TTC) — a fixed, well-documented binary layout, so this
 * needs no font-parsing library. Returns `null` for anything this can't cheaply read: a WOFF/WOFF2
 * container (their tables are zlib/Brotli-compressed — decoding them would need real decompression, which
 * this module deliberately doesn't take on per the lean-core guardrail) or a corrupt/truncated file. The
 * preview shows whatever comes back and simply omits a metadata row it doesn't have — never an error.
 */
export function parseSfntMetadata(bytes: Uint8Array): SfntMetadata | null {
  const format = sniffFontFormat(bytes);
  if (format !== "TrueType" && format !== "OpenType") return null; // WOFF/WOFF2/Unknown: bail gracefully
  try {
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    // A TTC ('ttcf') collection's header holds an offset table pointing at each contained font's own
    // sfnt header, rather than the table directory living right after the 4-byte tag.
    const sfntOffset = view.getUint32(0) === 0x74746366 ? view.getUint32(12) : 0;
    const numTables = view.getUint16(sfntOffset + 4);
    let nameOffset = -1;
    let nameLength = 0;
    let maxpOffset = -1;
    let p = sfntOffset + 12;
    for (let i = 0; i < numTables; i++, p += 16) {
      const tag = String.fromCharCode(bytes[p], bytes[p + 1], bytes[p + 2], bytes[p + 3]);
      const offset = view.getUint32(p + 8);
      const length = view.getUint32(p + 12);
      if (tag === "name") {
        nameOffset = offset;
        nameLength = length;
      } else if (tag === "maxp") {
        maxpOffset = offset;
      }
    }
    const numGlyphs = maxpOffset >= 0 ? view.getUint16(maxpOffset + 4) : null;
    const names = nameOffset >= 0 ? parseNameTable(view, bytes, nameOffset, nameLength) : {};
    return { family: names[1] ?? null, style: names[2] ?? null, version: names[5] ?? null, numGlyphs };
  } catch {
    return null; // an offset/length that runs past the buffer — treat as unparseable, not a crash
  }
}

/** Decode the `name` table's records for the nameIDs this preview cares about (1 = family, 2 = subfamily,
 *  5 = version), preferring a Windows/Unicode-BMP English record (platform 3, encoding 1, language
 *  0x0409) and falling back to Macintosh Roman (platform 1) when Windows English isn't present. */
function parseNameTable(view: DataView, bytes: Uint8Array, offset: number, length: number): Record<number, string> {
  const count = view.getUint16(offset + 2);
  const stringOffset = offset + view.getUint16(offset + 4);
  const found: Record<number, { value: string; priority: number }> = {};
  let p = offset + 6;
  for (let i = 0; i < count; i++, p += 12) {
    const platformId = view.getUint16(p);
    const encodingId = view.getUint16(p + 2);
    const languageId = view.getUint16(p + 4);
    const nameId = view.getUint16(p + 6);
    const len = view.getUint16(p + 8);
    const strOff = view.getUint16(p + 10);
    if (nameId !== 1 && nameId !== 2 && nameId !== 5) continue;
    const start = stringOffset + strOff;
    if (start + len > bytes.length) continue; // malformed record — skip rather than throw

    let value: string;
    let priority: number;
    if (platformId === 3 && encodingId === 1) {
      value = decodeUtf16Be(bytes, start, len);
      priority = languageId === 0x0409 ? 2 : 1;
    } else if (platformId === 1) {
      value = decodeMacRoman(bytes, start, len);
      priority = 0;
    } else {
      continue;
    }
    const existing = found[nameId];
    if (!existing || priority > existing.priority) found[nameId] = { value, priority };
  }
  const out: Record<number, string> = {};
  for (const [id, rec] of Object.entries(found)) out[Number(id)] = rec.value;
  return out;
}

function decodeUtf16Be(bytes: Uint8Array, start: number, len: number): string {
  let s = "";
  for (let i = 0; i + 1 < len; i += 2) s += String.fromCharCode((bytes[start + i] << 8) | bytes[start + i + 1]);
  return s;
}

/** Good enough for the ASCII-range font names this covers in practice (family/style/version strings are
 *  overwhelmingly ASCII); non-ASCII Mac Roman bytes pass through as their raw code unit rather than being
 *  properly re-mapped — an acceptable simplification for a metadata display, not a text editor. */
function decodeMacRoman(bytes: Uint8Array, start: number, len: number): string {
  let s = "";
  for (let i = 0; i < len; i++) s += String.fromCharCode(bytes[start + i]);
  return s;
}
