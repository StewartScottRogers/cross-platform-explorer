/**
 * Font preview helpers (CPE-1586/CPE-1593, epic CPE-1568 slice 5): pure, unit-testable helpers behind the
 * font glyph grid + specimen view — capping the grid (STREAMING.md / PURPOSE.md fast/small/predictable),
 * parsing the font's own `cmap` table to drive that grid from REAL glyph coverage, and a lightweight sfnt
 * (`name`/`maxp` table) metadata sniff for when the browser's own `FontFace` API can't report a font's
 * internal family/style/version (it only ever echoes back what THE CALLER assigned it, never the font
 * file's own name-table strings — see {@link parseSfntMetadata}'s doc comment).
 *
 * No new dependency: TrueType/OpenType (uncompressed sfnt) tables are read directly off the raw bytes
 * (via targeted byte-range reads — see {@link locateSfntTable}'s doc comment for why, CPE-1593);
 * WOFF/WOFF2 (compressed containers) are sniffed by format only and otherwise degrade gracefully rather
 * than pulling in a decompression/font-parsing library.
 */

/** Fallback codepoints for the glyph grid when the font's own `cmap` coverage can't be read (parse
 *  failure, unsupported subtable format, or a WOFF/WOFF2 container this module doesn't decompress):
 *  printable Basic Latin (U+0021–U+007E, skipping the invisible space at U+0020) plus Latin-1 Supplement
 *  (U+00A1–U+00FF, skipping the invisible NBSP at U+00A0) — the common range most Latin-script fonts
 *  cover. This is a fixed SAMPLE, not the font's actual coverage; a glyph the font doesn't define simply
 *  falls back to the browser's normal font-substitution behaviour when rendered, same as any other
 *  missing-glyph case on the web platform. See {@link parseCmapCoverage} for the real-coverage path. */
export const FONT_GLYPH_CANDIDATES: number[] = (() => {
  const cps: number[] = [];
  for (let cp = 0x21; cp <= 0x7e; cp++) cps.push(cp);
  for (let cp = 0xa1; cp <= 0xff; cp++) cps.push(cp);
  return cps;
})();

/** Hard cap on glyph-grid cells actually rendered — an arbitrarily large candidate list (a CJK font's
 *  real `cmap` coverage can run into the tens of thousands) can never stall the pane with thousands of DOM
 *  nodes (STREAMING.md / PURPOSE.md fast/small/predictable). */
export const GLYPH_GRID_CAP = 200;

export interface GlyphGrid {
  /** The codepoints to actually render — at most `cap` entries. */
  shown: number[];
  /** The full candidate count before capping. */
  total: number;
  /** Whether `shown` is a strict prefix of the input (i.e. some candidates were dropped). */
  truncated: boolean;
  /** Whether `shown` reflects the font's real `cmap` coverage or the fixed Latin fallback sample — the
   *  UI labels the cap honestly using this (CPE-1593). */
  source: "coverage" | "fallback";
}

/** Cap `codepoints` to at most `cap` entries, reporting whether it was truncated. The pane renders only
 *  `shown` — never the full `codepoints` list — so a huge candidate set can't stall it. Used both for the
 *  fixed fallback list (a strict prefix is fine — it's small and arbitrary either way) and, via
 *  {@link glyphGridFromCoverage}, as the trivial case when real coverage already fits under the cap. */
export function capGlyphs(codepoints: number[], cap: number = GLYPH_GRID_CAP): GlyphGrid {
  const truncated = codepoints.length > cap;
  return {
    shown: truncated ? codepoints.slice(0, cap) : codepoints,
    total: codepoints.length,
    truncated,
    source: "fallback",
  };
}

/** Codepoints a glyph-grid cell should never show even when the font defines a glyph for them: they
 *  render as blank/invisible tiles, which just wastes a grid cell (CPE-1593 UAT: since `sampleCoverage`'s
 *  input is sorted ascending, cell 0 was ALWAYS one of these — `U+0020` space for arial, `U+0000` NUL for
 *  malgun, `U+000C` form feed for seguisym). Covers the Unicode categories that matter in practice — Cc
 *  (C0/C1 controls), Zs (space separators), and the common Cf ("format", i.e. invisible-by-design)
 *  characters — not an exhaustive Unicode category database (no new dependency for one). */
function isDisplayableCodepoint(cp: number): boolean {
  // Cc: C0 and C1 controls.
  if (cp <= 0x1f || (cp >= 0x7f && cp <= 0x9f)) return false;
  // Zs: space separators.
  if (cp === 0x20 || cp === 0xa0 || cp === 0x1680 || (cp >= 0x2000 && cp <= 0x200a) || cp === 0x202f || cp === 0x205f || cp === 0x3000) {
    return false;
  }
  // Cf: the common invisible "format character" cases (soft hyphen, zero-width space/joiners, bidi
  // marks, BOM, interlinear annotation anchors).
  if (
    cp === 0xad ||
    (cp >= 0x200b && cp <= 0x200f) ||
    (cp >= 0x202a && cp <= 0x202e) ||
    (cp >= 0x2060 && cp <= 0x2064) ||
    (cp >= 0x2066 && cp <= 0x206f) ||
    cp === 0xfeff ||
    (cp >= 0xfff9 && cp <= 0xfffb)
  ) {
    return false;
  }
  return true;
}

/** Unicode ranges reserved a slice of the sample budget, when the font's coverage includes them, ahead of
 *  evenly sampling the rest — the densely-useful "common Latin" range a user opens a font expecting to
 *  see first. Checked in codepoint order, so the reserved slice comes out front-loaded with punctuation,
 *  then digits, then the alphabet, in natural reading order. */
const RESERVED_LOW_BLOCKS: readonly { start: number; end: number }[] = [
  { start: 0x21, end: 0x7e }, // Basic Latin, printable (94 codepoints: digits, A–Z, a–z, punctuation)
  { start: 0xa1, end: 0xff }, // Latin-1 Supplement
];

/** Within {@link RESERVED_LOW_BLOCKS}, the codepoints an "alphabet" reservation actually needs: digits,
 *  then uppercase, then lowercase — exactly the `0–9`, `A–Z`, `a–z` run the CPE-1593 acceptance criterion
 *  requires. Selected out of the low-block pool AHEAD OF plain punctuation and Latin-1 Supplement (see
 *  {@link sampleCoverage}), so the reservation's mandatory floor is the minimum that still guarantees the
 *  full alnum run — not however much of the low blocks happens to sort first by raw codepoint value. Final
 *  render order is unaffected: {@link sampleCoverage} re-sorts `shown` ascending regardless of selection
 *  order, so this only changes what gets INCLUDED under a tight budget, never the on-screen ordering. */
function isReservedAlnum(cp: number): boolean {
  return (cp >= 0x30 && cp <= 0x39) || (cp >= 0x41 && cp <= 0x5a) || (cp >= 0x61 && cp <= 0x7a);
}

/** Upper bound on the Latin reservation's share of the cap — the ceiling {@link sampleCoverage} scales
 *  down FROM as a font's non-Latin coverage grows (CPE-1598), and, unscaled, the same 50% flat share
 *  CPE-1593 originally shipped: never worse than that scheme for a genuinely Latin-dominant font (little
 *  or no other coverage), and enough headroom that the reservation can still consume most of a small
 *  extended-Latin font's budget when its own coverage is almost entirely inside {@link RESERVED_LOW_BLOCKS}. */
const RESERVED_LOW_BLOCK_MAX_SHARE = 0.5;

/**
 * Scale the Latin reservation by how much of the font's own coverage is actually Latin, instead of a flat
 * share (CPE-1598 — follow-up to CPE-1593's flat `RESERVED_LOW_BLOCK_SHARE = 0.5`). That flat share taxed
 * every font covering Basic Latin + Latin-1 the same 100 cells regardless of context; nearly every
 * real-world CJK UI font fully covers ASCII (needed for mixed-script text) and so paid the full tax even
 * though its own script is what a user opens the font to see. Measured on Malgun Gothic (27,133 codepoints,
 * cmap dominated by Hangul/Han): the flat share cost it 34 of its 134 available distinct 256-blocks (134 →
 * 100) for a reservation double what showing the alphabet actually requires.
 *
 * Three-part shape (CPE-1598's stated fix, each part measured against `arial.ttf`/`malgun.ttf`/
 * `seguisym.ttf`/`msyh.ttc` — see `font.test.ts` for the calibration test and PR description for the
 * before/after table):
 *
 * - **Proportional**: `latinShare` — the reserved low blocks' share of the font's own TOTAL displayable
 *   coverage — scaled onto the cap. A font whose Latin coverage is a small fraction of a much larger
 *   repertoire (any real CJK font: 189 Latin codepoints out of tens of thousands) gets a tiny proportional
 *   figure; a font whose coverage is mostly/entirely those same low blocks gets a large one.
 * - **Floored**: never below however many of the {@link isReservedAlnum} codepoints the font actually
 *   covers — the CPE-1593 acceptance criterion (full `A–Z`/`a–z`/`0–9`) must hold regardless of how the
 *   proportional term comes out. This floor is usually what real fonts land on: measured against all four
 *   test fonts above, `latinShare` never exceeds ~5.4% (Arial, the most Latin-heavy of the four) — the
 *   reserved low blocks are capped at 189 codepoints by definition, so `latinShare` only climbs above the
 *   floor for a font with a genuinely small total repertoire (see the "small extended-Latin font" case in
 *   `font.test.ts`). For Malgun this floor is 62 cells versus the old flat 100 — recovering roughly half
 *   the block-diversity CPE-1593's flat share cost it (134-ceiling measurement above), while still
 *   guaranteeing the exact same alphabet/digit run.
 * - **Capped**: never above {@link RESERVED_LOW_BLOCK_MAX_SHARE} of the cap — the same ceiling the old flat
 *   share used, so a Latin-dominant font (little/no other coverage) still gets a reservation generous
 *   enough to fill the grid, exactly as before.
 */
function reservedLowBlockBudget(reservedPool: number[], visibleTotal: number, cap: number): number {
  const alnumCount = reservedPool.filter(isReservedAlnum).length;
  const latinShare = reservedPool.length / visibleTotal;
  const proportional = Math.ceil(cap * latinShare);
  const maxBudget = Math.ceil(cap * RESERVED_LOW_BLOCK_MAX_SHARE);
  return Math.min(reservedPool.length, Math.max(alnumCount, Math.min(proportional, maxBudget)));
}

/** Evenly sample at most `cap` codepoints across a SORTED coverage list, so a capped grid shows a
 *  representative spread of what the font covers (touching multiple Unicode blocks) rather than an
 *  arbitrary run of consecutive low codepoints from the front of the list — a CJK font's cmap, sorted
 *  ascending, would otherwise show only the first Unicode block it happens to define (CPE-1593: "prefer a
 *  meaningful spread over an arbitrary first-N slice"). Two refinements on top of plain even-stride
 *  sampling (both CPE-1593 UAT findings against real fonts):
 *
 *  - Blank/invisible codepoints (control characters, whitespace — see {@link isDisplayableCodepoint}) are
 *    filtered out before sampling, so a grid cell is never wasted on an invisible tile.
 *  - A share of the budget is reserved for {@link RESERVED_LOW_BLOCKS} (ordinary printable Latin) when the
 *    font's coverage includes them, so an ordinary Latin-script font (the common case — Arial is the font
 *    people preview most) still shows its alphabet up front instead of getting diluted by even-stride
 *    sampling across its full (often much larger) Unicode coverage. The SIZE of that share is scaled by
 *    {@link reservedLowBlockBudget} rather than a flat fraction (CPE-1598): a font with no Latin coverage
 *    at all pays nothing (as before), but a font that fully covers ASCII+Latin-1 only as a side effect of
 *    a much bigger non-Latin repertoire — nearly every real-world CJK UI font, which needs ASCII for
 *    mixed-script text — now pays only the minimum needed to guarantee the alphabet/digit run, not a flat
 *    tax sized as if Latin were its main content. See that function's doc comment for the measured numbers.
 */
export function sampleCoverage(codepoints: number[], cap: number = GLYPH_GRID_CAP): GlyphGrid {
  const visible = codepoints.filter(isDisplayableCodepoint);
  if (visible.length <= cap) {
    return { shown: visible, total: visible.length, truncated: false, source: "coverage" };
  }

  const isReservedLowBlock = (cp: number) => RESERVED_LOW_BLOCKS.some((r) => cp >= r.start && cp <= r.end);
  const reservedPool = visible.filter(isReservedLowBlock); // already ascending — `visible` is sorted
  const reservedBudget = reservedLowBlockBudget(reservedPool, visible.length, cap);
  // Alnum codepoints (the CPE-1593 acceptance criterion) get first claim on the budget, ahead of plain
  // punctuation/Latin-1 — see `isReservedAlnum`'s doc comment for why this doesn't affect render order.
  const alnumFirst = [...reservedPool.filter(isReservedAlnum), ...reservedPool.filter((cp) => !isReservedAlnum(cp))];
  const reservedShown = alnumFirst.slice(0, reservedBudget);
  const reservedShownSet = new Set(reservedShown);

  // The rest of the budget is spent evenly across everything NOT already shown — including any leftover
  // reserved-block codepoints that didn't fit in reservedBudget, blended in with the font's other coverage
  // rather than dropped outright.
  const rest = visible.filter((cp) => !reservedShownSet.has(cp));
  const remainingCap = cap - reservedShown.length;
  let restShown: number[];
  if (rest.length <= remainingCap) {
    restShown = rest;
  } else {
    restShown = [];
    const stride = rest.length / remainingCap;
    for (let i = 0; i < remainingCap; i++) restShown.push(rest[Math.floor(i * stride)]);
  }

  const shown = [...reservedShown, ...restShown].sort((a, b) => a - b);
  return { shown, total: visible.length, truncated: true, source: "coverage" };
}

/** Build the glyph grid to render: real `cmap` coverage when available, otherwise the fixed Latin fallback
 *  sample — the single seam `FontPreview.svelte` calls after attempting to read+parse the font's `cmap`
 *  table (CPE-1593). Also falls back when coverage is non-empty but entirely non-displayable after
 *  {@link sampleCoverage}'s filtering (a pathological font whose cmap covers only control/whitespace
 *  codepoints) — an empty "real coverage" grid would be worse than the honest fallback sample. */
export function glyphGridFromCoverage(coverage: number[] | null): GlyphGrid {
  if (coverage && coverage.length > 0) {
    const sampled = sampleCoverage(coverage);
    if (sampled.shown.length > 0) return sampled;
  }
  return capGlyphs(FONT_GLYPH_CANDIDATES);
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

/** One sfnt table directory entry: where a table lives in the file and how long it is. */
export interface SfntTableEntry {
  offset: number;
  length: number;
}

/** Sanity cap on `numTables` — real fonts have a few dozen at most; a corrupt/hostile count this large
 *  can only come from a bogus field, so treat it as unparseable rather than looping needlessly. */
const MAX_SFNT_TABLES = 512;

/**
 * Walk an sfnt (TrueType/OpenType/TTC) table directory and return every table's offset + length, keyed by
 * 4-byte tag (`"name"`, `"maxp"`, `"cmap"`, …). Shared by {@link parseSfntMetadata} and
 * {@link locateSfntTable} so the directory is only walked once per concern. Throws (RangeError) on a
 * directory that runs past `bytes` — callers wrap this in `try/catch` and degrade gracefully, exactly as
 * they already do for a truncated/corrupt file; a `ttcf` collection with a bogus sub-font offset hits the
 * same path (the `numTables` read at that bogus offset throws immediately).
 */
function readSfntTableDirectory(view: DataView, bytes: Uint8Array): Map<string, SfntTableEntry> {
  // A TTC ('ttcf') collection's header holds an offset table pointing at each contained font's own sfnt
  // header, rather than the table directory living right after the 4-byte tag.
  const sfntOffset = view.getUint32(0) === 0x74746366 ? view.getUint32(12) : 0;
  const numTables = view.getUint16(sfntOffset + 4);
  if (numTables > MAX_SFNT_TABLES) return new Map(); // implausible count — treat as unparseable, not a crash
  const tables = new Map<string, SfntTableEntry>();
  let p = sfntOffset + 12;
  for (let i = 0; i < numTables; i++, p += 16) {
    const tag = String.fromCharCode(bytes[p], bytes[p + 1], bytes[p + 2], bytes[p + 3]);
    const offset = view.getUint32(p + 8);
    const length = view.getUint32(p + 12);
    tables.set(tag, { offset, length });
  }
  return tables;
}

/**
 * Locate a table's offset/length in the sfnt table directory WITHOUT reading its contents — used to find
 * where the `cmap` table lives so the caller (`FontPreview.svelte`) can decide whether it's already within
 * the metadata head-bytes it read, or needs a second targeted `read_file_range` call (CPE-1593: avoids
 * ever reading a whole multi-MB font file just to sniff coverage). Same graceful-degrade contract as
 * {@link parseSfntMetadata}: `null` for WOFF/WOFF2/Unknown, a missing table, or anything unparseable.
 */
export function locateSfntTable(bytes: Uint8Array, tag: string): SfntTableEntry | null {
  const format = sniffFontFormat(bytes);
  if (format !== "TrueType" && format !== "OpenType") return null;
  try {
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    return readSfntTableDirectory(view, bytes).get(tag) ?? null;
  } catch {
    return null;
  }
}

/**
 * Read the `name` (family/style/version strings) and `maxp` (glyph count) tables directly out of an
 * UNCOMPRESSED sfnt container (TrueType/OpenType/TTC) — a fixed, well-documented binary layout, so this
 * needs no font-parsing library. Returns `null` for anything this can't cheaply read: a WOFF/WOFF2
 * container (their tables are zlib/Brotli-compressed — decoding them would need real decompression, which
 * this module deliberately doesn't take on per the lean-core guardrail) or a corrupt/truncated file. The
 * preview shows whatever comes back and simply omits a metadata row it doesn't have — never an error.
 *
 * This is the "everything in one buffer" convenience form — pass the whole file, or a big-enough leading
 * chunk that happens to also contain the `name`/`maxp` tables. **`FontPreview.svelte` does NOT call this
 * for a real file**: real-world fonts (verified against `arial.ttf`/`malgun.ttf`/`seguisym.ttf` on Windows,
 * CPE-1593) commonly place `name` right near the very END of the file — often hundreds of KB to multi-MB
 * in — while `maxp` sits near the start, so no single "leading byte range" reliably contains both. The
 * preview instead locates each table via {@link locateSfntTable} and parses each one from its OWN
 * table-local bytes (a small targeted `read_file_range` for whichever table isn't already in the head
 * chunk it read) using {@link parseMaxpNumGlyphs} and {@link parseNameTable} directly. This function stays
 * around as a simpler, still fully tested pure entry point for the case where the caller already has one
 * contiguous buffer covering everything.
 */
export function parseSfntMetadata(bytes: Uint8Array): SfntMetadata | null {
  const format = sniffFontFormat(bytes);
  if (format !== "TrueType" && format !== "OpenType") return null; // WOFF/WOFF2/Unknown: bail gracefully
  try {
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    const tables = readSfntTableDirectory(view, bytes);
    const nameEntry = tables.get("name");
    const maxpEntry = tables.get("maxp");
    const numGlyphs = maxpEntry
      ? parseMaxpNumGlyphs(bytes.subarray(maxpEntry.offset, maxpEntry.offset + maxpEntry.length))
      : null;
    const names = nameEntry ? parseNameTable(bytes.subarray(nameEntry.offset, nameEntry.offset + nameEntry.length)) : {};
    return { family: names[1] ?? null, style: names[2] ?? null, version: names[5] ?? null, numGlyphs };
  } catch {
    return null; // an offset/length that runs past the buffer — treat as unparseable, not a crash
  }
}

/** Parse the `maxp` table's glyph count from TABLE-LOCAL bytes (`bytes[0]` is the table's own first byte —
 *  e.g. a slice at the offset/length {@link locateSfntTable} reports, whether that came from the metadata
 *  head chunk or a separate targeted range read). Never throws; returns `null` for anything too short to
 *  hold the field it needs. */
export function parseMaxpNumGlyphs(bytes: Uint8Array): number | null {
  if (bytes.length < 6) return null;
  try {
    return new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getUint16(4);
  } catch {
    return null;
  }
}

/** Parse the `name` table's records for the nameIDs this preview cares about (1 = family, 2 = subfamily,
 *  5 = version) from TABLE-LOCAL bytes (`bytes[0]` is the table's own first byte, same convention as
 *  {@link parseMaxpNumGlyphs}) — preferring a Windows/Unicode-BMP English record (platform 3, encoding 1,
 *  language 0x0409) and falling back to Macintosh Roman (platform 1) when Windows English isn't present.
 *  Reads are inherently bounded to `bytes.length` — this table's own declared extent — so a corrupt record
 *  count or string offset can never read a neighbouring table's bytes (CPE-1593 correctness nit: solved by
 *  construction here, since every caller hands this function ONLY that one table's own slice, rather than
 *  needing a separate `length` parameter to police reads against a larger shared buffer). Never throws. */
export function parseNameTable(bytes: Uint8Array): Record<number, string> {
  try {
    if (bytes.length < 6) return {};
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    const count = view.getUint16(2);
    const stringOffset = view.getUint16(4);
    const found: Record<number, { value: string; priority: number }> = {};
    let p = 6;
    for (let i = 0; i < count; i++, p += 12) {
      if (p + 12 > bytes.length) break; // record header runs past this table's own extent — stop
      const platformId = view.getUint16(p);
      const encodingId = view.getUint16(p + 2);
      const languageId = view.getUint16(p + 4);
      const nameId = view.getUint16(p + 6);
      const len = view.getUint16(p + 8);
      const strOff = view.getUint16(p + 10);
      if (nameId !== 1 && nameId !== 2 && nameId !== 5) continue;
      const start = stringOffset + strOff;
      if (start < 0 || start + len > bytes.length) continue; // malformed/out-of-table record — skip

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
  } catch {
    return {}; // an offset/length that runs past the buffer — treat as unparseable, not a crash
  }
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

// ---- Byte-range read sizing (CPE-1593) -----------------------------------------------------------
//
// `FontPreview.svelte` no longer reads the whole font file to sniff metadata: it does one `read_file_range`
// call for a small LEADING chunk (covers the sfnt header + table directory, which are always right at the
// front — and often `maxp` too, a small table real font compilers tend to place early), then, for whichever
// of `name`/`maxp`/`cmap` the table directory says lies OUTSIDE that chunk, one further targeted range read
// per table for exactly its own declared extent (capped, since a hostile/corrupt length field must never
// justify an unbounded read). Verified against real Windows fonts (CPE-1593): `name` in particular is
// routinely placed right near EOF — often hundreds of KB to multi-MB into the file — so a leading chunk
// alone is NOT enough to reliably reach it; the extra targeted read is the norm for `name`, not a rare
// exception. Even so, this is at most a handful of small, bounded reads — nowhere close to reading the
// whole file (the original problem this ticket fixes).

/** Size of the leading byte range `FontPreview.svelte` reads first. Comfortably covers the sfnt header and
 *  table directory for any real font (a few hundred tables at 16 bytes each would still fit many times
 *  over), and often `maxp` too — while staying minuscule next to a multi-MB font. Whatever isn't covered
 *  (very commonly `name`, sometimes `cmap`) gets its own small targeted read instead (CPE-1593). */
export const FONT_METADATA_HEAD_BYTES = 8 * 1024; // 8 KiB

/** Hard cap on a single targeted range read for a table (`name`, `maxp`, or `cmap`) that lives outside the
 *  head bytes above. Bounds the read even if a corrupt/hostile table-directory `length` field lies about
 *  the table's size — `read_file_range` itself also clamps to EOF, so this is defense in depth, not the
 *  only guard. Real `name` tables are a few KB at most; real `cmap` tables (even format 12 covering
 *  full CJK/astral-plane repertoires) are normally well under 1 MB — this cap has generous headroom over
 *  both. */
export const FONT_TABLE_RANGE_MAX_BYTES = 4 * 1024 * 1024; // 4 MiB

/** Sanity cap on a format-4 subtable's segment count and a format-12 subtable's group count — bounds the
 *  per-entry loop below even against a corrupt/hostile count field (each entry is only inspected once the
 *  byte-range bound check below also passes, so this is a second, independent guard). */
const MAX_CMAP_ENTRIES = 20_000;

/** Overall cap on codepoints collected from a single `cmap` subtable — independent of {@link GLYPH_GRID_CAP}
 *  (that caps what's actually RENDERED; this caps how much work/memory the parse itself can do before
 *  bailing out early with whatever it already found). Comfortably above any real font's BMP coverage
 *  (65,536 codepoints) with headroom for astral-plane format-12 coverage too. */
const MAX_COVERAGE_CODEPOINTS = 100_000;

/**
 * Parse a font's `cmap` table to discover the actual Unicode codepoints it defines a (non-`.notdef`) glyph
 * for, so the glyph grid can be driven by the font's REAL coverage (a CJK font, a symbol font, any
 * non-Latin face) instead of the fixed Latin fallback sample (CPE-1593). `bytes` must start AT the `cmap`
 * table itself (i.e. already sliced/read to that table's offset — see {@link locateSfntTable}), not at the
 * start of the file.
 *
 * Only the two most common subtable formats are handled: format 4 (segment mapping, BMP-only — present on
 * almost every TrueType/OpenType font with any Unicode coverage) and format 12 (segmented coverage by
 * groups — used for full-Unicode coverage including astral planes: CJK Extension B+, emoji, historic
 * scripts). Anything else (formats 0/2/6/13/14, or a subtable that doesn't parse cleanly) returns `null` so
 * the caller falls back to the fixed candidate list — deliberately not a complete cmap implementation.
 *
 * Never throws: a hostile/corrupt table degrades to `null` (nothing usable found) or a partial codepoint
 * list (whatever was validly readable before a bound was hit), exactly like {@link parseSfntMetadata}.
 */
export function parseCmapCoverage(bytes: Uint8Array): number[] | null {
  try {
    if (bytes.length < 4) return null;
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    if (view.getUint16(0) !== 0) return null; // cmap table version must be 0
    const numTables = view.getUint16(2);
    if (numTables > MAX_CMAP_ENTRIES) return null; // implausible — treat as unparseable

    // Pick the subtable most likely to give real Unicode coverage: prefer a full-Unicode encoding (format
    // 12 territory) over a BMP-only one (format 4 territory), preferring Windows platform records (the
    // overwhelmingly common case) over the rarer platform-0 "Unicode" platform ones at the same tier.
    let best: { offset: number; rank: number } | null = null;
    let p = 4;
    for (let i = 0; i < numTables; i++, p += 8) {
      if (p + 8 > bytes.length) break;
      const platformId = view.getUint16(p);
      const encodingId = view.getUint16(p + 2);
      const offset = view.getUint32(p + 4);
      const rank =
        platformId === 3 && encodingId === 10 ? 4 // Windows, full Unicode (UCS-4) — typically format 12
        : platformId === 0 && encodingId >= 4 ? 4 // Unicode platform, full-repertoire encoding
        : platformId === 3 && encodingId === 1 ? 3 // Windows, BMP Unicode — typically format 4
        : platformId === 0 ? 2 // Unicode platform, any other encoding
        : platformId === 3 && encodingId === 0 ? 1 // Windows, Symbol — format 4, PUA-mapped
        : 0;
      if (rank > 0 && (!best || rank > best.rank)) best = { offset, rank };
    }
    if (!best || best.offset + 2 > bytes.length) return null;

    const format = view.getUint16(best.offset);
    if (format === 12) return parseCmapFormat12(view, bytes, best.offset);
    if (format === 4) return parseCmapFormat4(view, bytes, best.offset);
    return null; // an unsupported subtable format — caller falls back to the fixed candidate list
  } catch {
    return null; // any offset/length that runs past the buffer — treat as unparseable, not a crash
  }
}

/** Format 4 ("segment mapping to delta values"): BMP-only coverage described as parallel arrays of
 *  end/start codes, deltas, and range offsets, one entry per segment. See the OpenType spec's `cmap`
 *  chapter for the full layout this mirrors. */
function parseCmapFormat4(view: DataView, bytes: Uint8Array, offset: number): number[] | null {
  const segCountX2 = view.getUint16(offset + 6);
  const segCount = segCountX2 / 2;
  if (!Number.isInteger(segCount) || segCount <= 0 || segCount > MAX_CMAP_ENTRIES) return null;
  const endCodeOff = offset + 14;
  const startCodeOff = endCodeOff + segCountX2 + 2; // +2 skips the reservedPad field
  const idDeltaOff = startCodeOff + segCountX2;
  const idRangeOff = idDeltaOff + segCountX2;
  const out: number[] = [];
  // Bounds the WHOLE traversal by codepoints examined, not codepoints pushed (CPE-1593 code review): the
  // `idRangeOffset !== 0` indirection branch below can `continue` — skip a codepoint without pushing it —
  // for every single codepoint in a segment (a crafted glyphId pointer that's always out of bounds), so a
  // push-only counter never fires and the loop degenerates to the full BMP span per segment. A crafted
  // ~160 KB cmap with segCount at the MAX_CMAP_ENTRIES ceiling, each segment spanning the BMP via that
  // indirection with an always-out-of-bounds offset, drove this to ~1.31 BILLION iterations (~8.8s of
  // synchronous UI-thread freeze) before this counter was added — the preview opens automatically on file
  // selection, so a hostile downloaded font could freeze the app. `examined` counts every codepoint the
  // inner loop LOOKS AT, pushed or not, so the cap always fires — a no-op for any real font (100,000 is
  // comfortably above the BMP's 65,536 codepoints).
  let examined = 0;
  for (let i = 0; i < segCount; i++) {
    const endCodeAddr = endCodeOff + i * 2;
    const startCodeAddr = startCodeOff + i * 2;
    const idDeltaAddr = idDeltaOff + i * 2;
    const idRangeAddr = idRangeOff + i * 2;
    if (idRangeAddr + 2 > bytes.length) break; // segment arrays run past the buffer — stop, keep what we found
    const endCode = view.getUint16(endCodeAddr);
    const startCode = view.getUint16(startCodeAddr);
    const idDelta = view.getInt16(idDeltaAddr);
    const idRangeOffset = view.getUint16(idRangeAddr);
    if (startCode > endCode) continue; // malformed segment — skip
    if (startCode === 0xffff && endCode === 0xffff) continue; // required terminator segment — no real glyphs
    for (let c = startCode; c <= endCode; c++) {
      if (++examined > MAX_COVERAGE_CODEPOINTS) return out;
      let glyphId: number;
      if (idRangeOffset === 0) {
        glyphId = (c + idDelta) & 0xffff;
      } else {
        const glyphIdAddr = idRangeAddr + idRangeOffset + (c - startCode) * 2;
        if (glyphIdAddr + 2 > bytes.length) continue; // out-of-range glyphId pointer — skip this codepoint
        const raw = view.getUint16(glyphIdAddr);
        glyphId = raw === 0 ? 0 : (raw + idDelta) & 0xffff;
      }
      if (glyphId !== 0) out.push(c); // glyph 0 is always .notdef — not real coverage
    }
  }
  return out;
}

/** Format 12 ("segmented coverage"): full-Unicode (including astral-plane) coverage described as groups
 *  of contiguous character-code ranges, each mapped to a contiguous run of glyph ids starting at
 *  `startGlyphId` — so every character in `[startCharCode, endCharCode]` has a real (non-`.notdef`) glyph
 *  by construction; unlike format 4 there's no per-codepoint zero-glyph case to filter out. */
function parseCmapFormat12(view: DataView, bytes: Uint8Array, offset: number): number[] | null {
  if (offset + 16 > bytes.length) return null;
  const numGroups = view.getUint32(offset + 12);
  if (numGroups > 500_000) return null; // implausible — treat as unparseable rather than looping needlessly
  const out: number[] = [];
  let p = offset + 16;
  for (let i = 0; i < numGroups; i++, p += 12) {
    if (p + 12 > bytes.length) break; // group table runs past the buffer — stop, keep what we found
    const startCharCode = view.getUint32(p);
    const endCharCode = view.getUint32(p + 4);
    if (startCharCode > endCharCode) continue; // malformed group — skip
    if (endCharCode - startCharCode > MAX_COVERAGE_CODEPOINTS) continue; // absurd single-group range — skip
    for (let c = startCharCode; c <= endCharCode; c++) {
      out.push(c);
      if (out.length >= MAX_COVERAGE_CODEPOINTS) return out;
    }
  }
  return out;
}
