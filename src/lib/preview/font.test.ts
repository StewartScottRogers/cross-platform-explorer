// CPE-1586/CPE-1593 (epic CPE-1568 slice 5): pure helpers behind the font preview's glyph grid + metadata
// sniff + cmap coverage parse.
import { describe, it, expect } from "vitest";
import {
  FONT_GLYPH_CANDIDATES,
  GLYPH_GRID_CAP,
  capGlyphs,
  sampleCoverage,
  glyphGridFromCoverage,
  glyphChar,
  codepointLabel,
  sniffFontFormat,
  formatLabelForExt,
  parseSfntMetadata,
  parseMaxpNumGlyphs,
  parseNameTable,
  locateSfntTable,
  parseCmapCoverage,
} from "./font";

describe("FONT_GLYPH_CANDIDATES", () => {
  it("is the printable Basic Latin + Latin-1 Supplement range, skipping the invisible space/NBSP", () => {
    expect(FONT_GLYPH_CANDIDATES[0]).toBe(0x21); // '!' — not the invisible space at 0x20
    expect(FONT_GLYPH_CANDIDATES).not.toContain(0x20);
    expect(FONT_GLYPH_CANDIDATES).not.toContain(0xa0);
    expect(FONT_GLYPH_CANDIDATES).toContain(0x7e); // '~'
    expect(FONT_GLYPH_CANDIDATES).toContain(0xff); // 'ÿ'
    // Every candidate falls in one of the two documented ranges.
    for (const cp of FONT_GLYPH_CANDIDATES) {
      expect((cp >= 0x21 && cp <= 0x7e) || (cp >= 0xa1 && cp <= 0xff)).toBe(true);
    }
    // Comfortably under the cap, so the grid never actually truncates today (see capGlyphs test below).
    expect(FONT_GLYPH_CANDIDATES.length).toBeLessThan(GLYPH_GRID_CAP);
  });
});

describe("capGlyphs", () => {
  it("passes a list at or under the cap through unchanged, tagged as the fallback source", () => {
    const result = capGlyphs([1, 2, 3], 5);
    expect(result).toEqual({ shown: [1, 2, 3], total: 3, truncated: false, source: "fallback" });
  });

  it("caps a list over the limit and reports the true total (STREAMING.md/PURPOSE.md)", () => {
    const big = Array.from({ length: 500 }, (_, i) => i);
    const result = capGlyphs(big, 200);
    expect(result.shown.length).toBe(200);
    expect(result.shown).toEqual(big.slice(0, 200));
    expect(result.total).toBe(500);
    expect(result.truncated).toBe(true);
  });

  it("defaults to GLYPH_GRID_CAP when no explicit cap is given", () => {
    const big = Array.from({ length: GLYPH_GRID_CAP + 50 }, (_, i) => i);
    const result = capGlyphs(big);
    expect(result.shown.length).toBe(GLYPH_GRID_CAP);
    expect(result.truncated).toBe(true);
  });
});

describe("sampleCoverage", () => {
  it("passes a list at or under the cap through unchanged, tagged as the coverage source", () => {
    const result = sampleCoverage([10, 20, 30], 5);
    expect(result).toEqual({ shown: [10, 20, 30], total: 3, truncated: false, source: "coverage" });
  });

  it("evenly samples across the full list rather than taking a prefix, when over the cap", () => {
    // A big sorted range spanning several "blocks" — a prefix slice would only ever show the front.
    const big = Array.from({ length: 10_000 }, (_, i) => i);
    const result = sampleCoverage(big, 100);
    expect(result.shown.length).toBe(100);
    expect(result.truncated).toBe(true);
    expect(result.total).toBe(10_000);
    // Not a prefix: the sample reaches well past what the first 100 entries of `big` would cover.
    expect(Math.max(...result.shown)).toBeGreaterThan(9000);
    // Ascending stride means it should still be sorted and touch the low end too.
    expect(result.shown[0]).toBe(0);
    expect([...result.shown]).toEqual([...result.shown].sort((a, b) => a - b));
  });
});

describe("glyphGridFromCoverage", () => {
  it("uses real coverage when given a non-empty codepoint list", () => {
    const result = glyphGridFromCoverage([65, 66, 67]);
    expect(result.source).toBe("coverage");
    expect(result.shown).toEqual([65, 66, 67]);
  });

  it("falls back to the fixed Latin sample when coverage is null", () => {
    const result = glyphGridFromCoverage(null);
    expect(result.source).toBe("fallback");
    expect(result.shown).toEqual(capGlyphs(FONT_GLYPH_CANDIDATES).shown);
  });

  it("falls back to the fixed Latin sample when coverage is an empty array", () => {
    const result = glyphGridFromCoverage([]);
    expect(result.source).toBe("fallback");
  });
});

describe("glyphChar / codepointLabel", () => {
  it("renders the character for a codepoint", () => {
    expect(glyphChar(0x41)).toBe("A");
    expect(glyphChar(0x20ac)).toBe("€");
  });

  it("formats a U+XXXX label, zero-padded and uppercase", () => {
    expect(codepointLabel(0x41)).toBe("U+0041");
    expect(codepointLabel(0x1f600)).toBe("U+1F600");
    expect(codepointLabel(0x9)).toBe("U+0009");
  });
});

describe("formatLabelForExt", () => {
  it("maps the four supported font extensions to their format label", () => {
    expect(formatLabelForExt("ttf")).toBe("TrueType");
    expect(formatLabelForExt("otf")).toBe("OpenType");
    expect(formatLabelForExt("woff")).toBe("WOFF");
    expect(formatLabelForExt("woff2")).toBe("WOFF2");
    expect(formatLabelForExt("TTF")).toBe("TrueType"); // case-insensitive
  });

  it("falls back to Unknown for anything else", () => {
    expect(formatLabelForExt("txt")).toBe("Unknown");
    expect(formatLabelForExt("")).toBe("Unknown");
  });
});

describe("sniffFontFormat", () => {
  const magic = (...bytes: number[]) => new Uint8Array([...bytes, 0, 0, 0, 0]);

  it("recognises TrueType (0x00010000 and 'true')", () => {
    expect(sniffFontFormat(magic(0x00, 0x01, 0x00, 0x00))).toBe("TrueType");
    expect(sniffFontFormat(magic(0x74, 0x72, 0x75, 0x65))).toBe("TrueType");
  });

  it("recognises a TrueType Collection ('ttcf') as TrueType", () => {
    expect(sniffFontFormat(magic(0x74, 0x74, 0x63, 0x66))).toBe("TrueType");
  });

  it("recognises OpenType ('OTTO')", () => {
    expect(sniffFontFormat(magic(0x4f, 0x54, 0x54, 0x4f))).toBe("OpenType");
  });

  it("recognises WOFF and WOFF2", () => {
    expect(sniffFontFormat(magic(0x77, 0x4f, 0x46, 0x46))).toBe("WOFF");
    expect(sniffFontFormat(magic(0x77, 0x4f, 0x46, 0x32))).toBe("WOFF2");
  });

  it("returns Unknown for unrecognised or too-short input", () => {
    expect(sniffFontFormat(magic(0xde, 0xad, 0xbe, 0xef))).toBe("Unknown");
    expect(sniffFontFormat(new Uint8Array([1, 2]))).toBe("Unknown");
    expect(sniffFontFormat(new Uint8Array(0))).toBe("Unknown");
  });
});

/**
 * Build a minimal, valid, UNCOMPRESSED sfnt (TrueType) buffer containing just a `name` table (family,
 * style, version as Windows/Unicode-BMP English records) and a `maxp` table (numGlyphs) — the two tables
 * `parseSfntMetadata` reads. Mirrors the real sfnt binary layout exactly (see that function's doc comment
 * for a description of the format) so the parser is exercised against real-shaped bytes, not a mock.
 */
function buildSfnt(opts: { family?: string; style?: string; version?: string; numGlyphs?: number }): Uint8Array {
  const records = [
    { nameId: 1, text: opts.family ?? "Test Sans" },
    { nameId: 2, text: opts.style ?? "Regular" },
    { nameId: 5, text: opts.version ?? "Version 1.000" },
  ];
  const encoded = records.map((r) => {
    const bytes: number[] = [];
    for (const ch of r.text) {
      const code = ch.charCodeAt(0);
      bytes.push((code >> 8) & 0xff, code & 0xff);
    }
    return { ...r, bytes };
  });

  const nameHeaderLen = 6 + encoded.length * 12;
  let cursor = 0;
  const stringOffsets = encoded.map((e) => {
    const off = cursor;
    cursor += e.bytes.length;
    return off;
  });
  const nameTableLen = nameHeaderLen + cursor;
  const maxpTableLen = 6;

  const sfntHeaderLen = 12;
  const numTables = 2;
  const tableDirLen = numTables * 16;
  const nameTableOffset = sfntHeaderLen + tableDirLen;
  const maxpTableOffset = nameTableOffset + nameTableLen;
  const totalLen = maxpTableOffset + maxpTableLen;

  const buf = new Uint8Array(totalLen);
  const view = new DataView(buf.buffer);
  const writeTag = (offset: number, tag: string) => {
    for (let i = 0; i < 4; i++) buf[offset + i] = tag.charCodeAt(i);
  };

  view.setUint32(0, 0x00010000);
  view.setUint16(4, numTables);

  writeTag(12, "name");
  view.setUint32(12 + 8, nameTableOffset);
  view.setUint32(12 + 12, nameTableLen);
  writeTag(28, "maxp");
  view.setUint32(28 + 8, maxpTableOffset);
  view.setUint32(28 + 12, maxpTableLen);

  let p = nameTableOffset;
  view.setUint16(p, 0); // format 0
  view.setUint16(p + 2, encoded.length);
  view.setUint16(p + 4, nameHeaderLen); // stringOffset, relative to table start
  let rp = p + 6;
  encoded.forEach((e, i) => {
    view.setUint16(rp, 3); // platformID: Windows
    view.setUint16(rp + 2, 1); // encodingID: Unicode BMP
    view.setUint16(rp + 4, 0x0409); // languageID: en-US
    view.setUint16(rp + 6, e.nameId);
    view.setUint16(rp + 8, e.bytes.length);
    view.setUint16(rp + 10, stringOffsets[i]);
    rp += 12;
  });
  let sp = p + nameHeaderLen;
  for (const e of encoded) for (const b of e.bytes) buf[sp++] = b;

  view.setUint32(maxpTableOffset, 0x00010000);
  view.setUint16(maxpTableOffset + 4, opts.numGlyphs ?? 42);

  return buf;
}

describe("parseSfntMetadata", () => {
  it("reads family/style/version/numGlyphs off a well-formed TrueType file", () => {
    const buf = buildSfnt({ family: "Acme Sans", style: "Bold", version: "Version 2.10", numGlyphs: 1337 });
    const meta = parseSfntMetadata(buf);
    expect(meta).toEqual({ family: "Acme Sans", style: "Bold", version: "Version 2.10", numGlyphs: 1337 });
  });

  it("returns null for a WOFF/WOFF2 container (compressed tables — out of scope, degrades gracefully)", () => {
    const woff = new Uint8Array(64);
    woff.set([0x77, 0x4f, 0x46, 0x46], 0); // 'wOFF'
    expect(parseSfntMetadata(woff)).toBeNull();

    const woff2 = new Uint8Array(64);
    woff2.set([0x77, 0x4f, 0x46, 0x32], 0); // 'wOF2'
    expect(parseSfntMetadata(woff2)).toBeNull();
  });

  it("returns null (never throws) for a truncated/corrupt TrueType-tagged buffer", () => {
    const truncated = new Uint8Array([0x00, 0x01, 0x00, 0x00]); // magic only, no table directory
    expect(parseSfntMetadata(truncated)).toBeNull();
  });

  it("returns null for an unrecognised format", () => {
    expect(parseSfntMetadata(new Uint8Array([1, 2, 3, 4, 5, 6]))).toBeNull();
  });

  // --- Fuzz-derived regression tests (CPE-1593) --------------------------------------------------
  // The Reviewer fuzzed parseSfntMetadata with 9 crafted malformed inputs and 20 rounds of random bytes
  // for CPE-1586 and it degraded gracefully every time, but that coverage never made it into the shipped
  // suite. These lock in the three specific cases called out for regression coverage, plus a random-bytes
  // sweep mirroring the reviewer's own fuzz pass.

  it("degrades gracefully (never throws) when a table's offset points past EOF — that table's own fields go null, others still parse", () => {
    const buf = buildSfnt({ family: "Real Family", numGlyphs: 42 });
    const view = new DataView(buf.buffer);
    // Corrupt the `name` table directory entry's offset field to point well past the buffer's end. `maxp`
    // (the second directory entry, untouched) is unaffected — each table is resolved/parsed independently
    // (CPE-1593), so one corrupt table doesn't have to take the whole result down with it.
    view.setUint32(12 + 8, buf.length + 100_000);
    expect(() => parseSfntMetadata(buf)).not.toThrow();
    const meta = parseSfntMetadata(buf);
    expect(meta).not.toBeNull();
    expect(meta).toEqual({ family: null, style: null, version: null, numGlyphs: 42 });
  });

  it("degrades gracefully (never throws, returns null) for a ttcf collection with a bogus sub-font offset", () => {
    const buf = new Uint8Array(32);
    const view = new DataView(buf.buffer);
    view.setUint32(0, 0x74746366); // 'ttcf'
    view.setUint32(12, 0xfffffff0); // absurd sub-font offset, far past EOF
    expect(() => parseSfntMetadata(buf)).not.toThrow();
    expect(parseSfntMetadata(buf)).toBeNull();
  });

  it("degrades gracefully (never throws, never hangs) for a corrupt/huge name-record count", () => {
    const buf = buildSfnt({ family: "Real Family" });
    const view = new DataView(buf.buffer);
    const nameTableOffset = view.getUint32(12 + 8);
    view.setUint16(nameTableOffset + 2, 0xffff); // implausible record count vs. the table's real (tiny) length
    const start = performance.now();
    expect(() => parseSfntMetadata(buf)).not.toThrow();
    // Bounded by the table's own declared length (see the next test), not by the bogus count — so this
    // must return almost immediately, nowhere near what iterating 0xFFFF fake records would cost.
    expect(performance.now() - start).toBeLessThan(50);
  });

  it("bounds name-record string reads to [offset, offset+length) — never reads a neighbouring table's bytes", () => {
    // parseNameTable used to take a whole-file-relative offset/length and never actually used `length` to
    // bound reads (CPE-1593 correctness nit): a corrupt stringOffset could read past this table into
    // whatever bytes happen to follow it (here, the `maxp` table built right after `name` — see buildSfnt's
    // layout). It's now table-local (bytes[0] IS the table's own first byte, see the describe block below),
    // so parseSfntMetadata hands it a `subarray` that physically excludes maxp's bytes — solving this by
    // construction rather than by a bounds check someone could forget.
    const buf = buildSfnt({ family: "Real Family", style: "Regular" });
    const view = new DataView(buf.buffer);
    const nameTableOffset = view.getUint32(12 + 8);
    const nameTableLength = view.getUint32(12 + 12);
    // The family record (nameId 1) is the first one; its stringOffset field sits at +6 (record header) +10.
    view.setUint16(nameTableOffset + 6 + 10, nameTableLength + 50); // now points past this table's own extent
    const meta = parseSfntMetadata(buf);
    expect(meta).not.toBeNull();
    expect(meta?.family ?? null).toBeNull(); // out-of-table string rejected, not leaked from maxp's bytes
    expect(meta?.style).toBe("Regular"); // the untouched record still reads fine
  });

  it("never throws across 20 rounds of random bytes of varying lengths (mirrors the reviewer's fuzz pass)", () => {
    for (let round = 0; round < 20; round++) {
      const len = 1 + Math.floor(Math.random() * 4096);
      const bytes = new Uint8Array(len);
      for (let i = 0; i < len; i++) bytes[i] = Math.floor(Math.random() * 256);
      expect(() => parseSfntMetadata(bytes)).not.toThrow();
    }
  });
});

// parseMaxpNumGlyphs / parseNameTable are TABLE-LOCAL (CPE-1593): `bytes[0]` is the table's own first
// byte, exactly what a real font's `name` table ends up being on its own (routinely near EOF — see
// FontPreview.svelte's doc comment) once resolved via a targeted `read_file_range`, separate from
// whatever leading chunk covered the rest of the metadata sniff.

describe("parseMaxpNumGlyphs", () => {
  it("reads numGlyphs from table-local maxp bytes", () => {
    const buf = new Uint8Array(6);
    new DataView(buf.buffer).setUint32(0, 0x00010000); // maxp version 0.5/1.0 doesn't matter here
    new DataView(buf.buffer).setUint16(4, 777);
    expect(parseMaxpNumGlyphs(buf)).toBe(777);
  });

  it("returns null (never throws) for a buffer too short to hold numGlyphs", () => {
    expect(parseMaxpNumGlyphs(new Uint8Array(0))).toBeNull();
    expect(parseMaxpNumGlyphs(new Uint8Array(4))).toBeNull();
    expect(() => parseMaxpNumGlyphs(new Uint8Array(4))).not.toThrow();
  });
});

describe("parseNameTable (table-local)", () => {
  /** Build a minimal, valid, TABLE-LOCAL `name` table buffer (no surrounding sfnt/file bytes) with the
   *  given records — same binary shape as the `name` table `buildSfnt` embeds, just standalone. */
  function buildNameTableBytes(records: { nameId: number; text: string; platformId?: number; encodingId?: number; languageId?: number }[]): Uint8Array {
    const encoded = records.map((r) => {
      const bytes: number[] = [];
      const macRoman = (r.platformId ?? 3) === 1;
      for (const ch of r.text) {
        const code = ch.charCodeAt(0);
        if (macRoman) bytes.push(code & 0xff); // single-byte, matches decodeMacRoman's own reverse
        else bytes.push((code >> 8) & 0xff, code & 0xff); // UTF-16BE, matches decodeUtf16Be
      }
      return { ...r, bytes };
    });
    const headerLen = 6 + encoded.length * 12;
    let cursor = 0;
    const stringOffsets = encoded.map((e) => {
      const off = cursor;
      cursor += e.bytes.length;
      return off;
    });
    const buf = new Uint8Array(headerLen + cursor);
    const view = new DataView(buf.buffer);
    view.setUint16(0, 0); // format 0
    view.setUint16(2, encoded.length);
    view.setUint16(4, headerLen); // stringOffset, table-relative
    let rp = 6;
    encoded.forEach((e, i) => {
      view.setUint16(rp, e.platformId ?? 3);
      view.setUint16(rp + 2, e.encodingId ?? 1);
      view.setUint16(rp + 4, e.languageId ?? 0x0409);
      view.setUint16(rp + 6, e.nameId);
      view.setUint16(rp + 8, e.bytes.length);
      view.setUint16(rp + 10, stringOffsets[i]);
      rp += 12;
    });
    let sp = headerLen;
    for (const e of encoded) for (const b of e.bytes) buf[sp++] = b;
    return buf;
  }

  it("reads family/style/version, preferring Windows/Unicode English", () => {
    const buf = buildNameTableBytes([
      { nameId: 1, text: "Acme Sans" },
      { nameId: 2, text: "Bold" },
      { nameId: 5, text: "Version 3.0" },
      { nameId: 4, text: "Acme Sans Bold" }, // full name — not one this preview reads, should be ignored
    ]);
    expect(parseNameTable(buf)).toEqual({ 1: "Acme Sans", 2: "Bold", 5: "Version 3.0" });
  });

  it("falls back to Macintosh Roman when no Windows English record exists", () => {
    const buf = buildNameTableBytes([{ nameId: 1, text: "Mac Font", platformId: 1, encodingId: 0, languageId: 0 }]);
    expect(parseNameTable(buf)).toEqual({ 1: "Mac Font" });
  });

  it("prefers Windows English over a Windows non-English record for the same nameId", () => {
    const buf = buildNameTableBytes([
      { nameId: 1, text: "Nom Francais", languageId: 0x040c }, // French
      { nameId: 1, text: "English Name", languageId: 0x0409 }, // English — should win
    ]);
    expect(parseNameTable(buf)).toEqual({ 1: "English Name" });
  });

  it("returns {} (never throws) for a buffer too short to hold a header", () => {
    expect(parseNameTable(new Uint8Array(0))).toEqual({});
    expect(parseNameTable(new Uint8Array(4))).toEqual({});
  });

  it("returns {} (never throws) for a record whose string offset/length runs past this table's own bytes", () => {
    const buf = buildNameTableBytes([{ nameId: 1, text: "Real" }]);
    const view = new DataView(buf.buffer);
    view.setUint16(6 + 10, buf.length + 500); // corrupt the sole record's stringOffset field
    expect(() => parseNameTable(buf)).not.toThrow();
    expect(parseNameTable(buf)).toEqual({});
  });

  it("never throws across 20 rounds of random bytes of varying lengths", () => {
    for (let round = 0; round < 20; round++) {
      const len = Math.floor(Math.random() * 2048);
      const bytes = new Uint8Array(len);
      for (let i = 0; i < len; i++) bytes[i] = Math.floor(Math.random() * 256);
      expect(() => parseNameTable(bytes)).not.toThrow();
    }
  });
});

/** Wrap a single already-built table's bytes in a minimal, valid one-table sfnt (header + table directory
 *  + the table itself) — used to exercise {@link locateSfntTable} against real-shaped bytes for a table
 *  other than `name`/`maxp` (`buildSfnt` above only ever builds those two). */
function buildSfntWithTable(tag: string, tableBytes: Uint8Array): Uint8Array {
  const sfntHeaderLen = 12;
  const tableDirLen = 16; // one table
  const tableOffset = sfntHeaderLen + tableDirLen;
  const buf = new Uint8Array(tableOffset + tableBytes.length);
  const view = new DataView(buf.buffer);
  view.setUint32(0, 0x00010000);
  view.setUint16(4, 1); // numTables
  for (let i = 0; i < 4; i++) buf[12 + i] = tag.charCodeAt(i);
  view.setUint32(12 + 8, tableOffset);
  view.setUint32(12 + 12, tableBytes.length);
  buf.set(tableBytes, tableOffset);
  return buf;
}

/** Build a minimal, valid format-4 (segment mapping) `cmap` SUBTABLE covering exactly `codepoints`
 *  (deduped, grouped into contiguous runs — same shape a real font's segments take), each mapped to a
 *  non-zero glyph id via the idDelta-only path (idRangeOffset = 0), plus the mandatory 0xFFFF/0xFFFF
 *  terminator segment. Mirrors the real binary layout {@link parseCmapCoverage}'s format-4 branch reads. */
function buildCmapFormat4Subtable(codepoints: number[]): Uint8Array {
  const sorted = [...new Set(codepoints)].sort((a, b) => a - b);
  const segments: { start: number; end: number }[] = [];
  for (const cp of sorted) {
    const last = segments[segments.length - 1];
    if (last && cp === last.end + 1) last.end = cp;
    else segments.push({ start: cp, end: cp });
  }
  segments.push({ start: 0xffff, end: 0xffff }); // required terminator segment
  const segCountX2 = segments.length * 2;

  const endCodeOff = 14;
  const startCodeOff = endCodeOff + segCountX2 + 2; // +2 skips reservedPad
  const idDeltaOff = startCodeOff + segCountX2;
  const idRangeOff = idDeltaOff + segCountX2;
  const totalLen = idRangeOff + segCountX2;

  const buf = new Uint8Array(totalLen);
  const view = new DataView(buf.buffer);
  view.setUint16(0, 4); // format
  view.setUint16(2, totalLen); // length
  view.setUint16(6, segCountX2);
  segments.forEach((seg, i) => {
    view.setUint16(endCodeOff + i * 2, seg.end);
    view.setUint16(startCodeOff + i * 2, seg.start);
    view.setInt16(idDeltaOff + i * 2, 1); // glyphId = c + 1, never 0 for any real (non-terminator) c
    view.setUint16(idRangeOff + i * 2, 0); // use the idDelta formula directly, no glyphIdArray needed
  });
  return buf;
}

/** Build a minimal, valid format-12 (segmented coverage) `cmap` SUBTABLE covering exactly `codepoints`
 *  (deduped, grouped into contiguous runs), each group mapped to an increasing run of glyph ids starting
 *  at 1 (glyph 0 is reserved for `.notdef`). Mirrors the real binary layout {@link parseCmapCoverage}'s
 *  format-12 branch reads — this is the format used for astral-plane / full-Unicode coverage. */
function buildCmapFormat12Subtable(codepoints: number[]): Uint8Array {
  const sorted = [...new Set(codepoints)].sort((a, b) => a - b);
  const groups: { start: number; end: number }[] = [];
  for (const cp of sorted) {
    const last = groups[groups.length - 1];
    if (last && cp === last.end + 1) last.end = cp;
    else groups.push({ start: cp, end: cp });
  }
  const headerLen = 16;
  const groupLen = 12;
  const totalLen = headerLen + groups.length * groupLen;
  const buf = new Uint8Array(totalLen);
  const view = new DataView(buf.buffer);
  view.setUint16(0, 12); // format
  view.setUint32(4, totalLen); // length
  view.setUint32(12, groups.length); // numGroups
  let glyphId = 1;
  groups.forEach((g, i) => {
    const p = headerLen + i * groupLen;
    view.setUint32(p, g.start);
    view.setUint32(p + 4, g.end);
    view.setUint32(p + 8, glyphId);
    glyphId += g.end - g.start + 1;
  });
  return buf;
}

/** Wrap a single subtable in a minimal, valid full `cmap` table (version + one encoding record + the
 *  subtable itself) — what {@link locateSfntTable}`(bytes, "cmap")` slices out of a real file, and what
 *  {@link parseCmapCoverage} expects to receive (bytes starting AT the `cmap` table, not the file start). */
function buildCmapTable(format: 4 | 12, codepoints: number[], opts?: { platformId?: number; encodingId?: number }): Uint8Array {
  const subtable = format === 4 ? buildCmapFormat4Subtable(codepoints) : buildCmapFormat12Subtable(codepoints);
  const subtableOffset = 4 + 8; // header (version+numTables) + one encoding record
  const buf = new Uint8Array(subtableOffset + subtable.length);
  const view = new DataView(buf.buffer);
  view.setUint16(0, 0); // cmap version
  view.setUint16(2, 1); // numTables (encoding records)
  view.setUint16(4, opts?.platformId ?? 3);
  view.setUint16(6, opts?.encodingId ?? (format === 12 ? 10 : 1));
  view.setUint32(8, subtableOffset);
  buf.set(subtable, subtableOffset);
  return buf;
}

describe("locateSfntTable", () => {
  it("finds a table by tag with its real offset/length", () => {
    const cmap = buildCmapTable(4, [0x41, 0x42, 0x43]);
    const buf = buildSfntWithTable("cmap", cmap);
    const entry = locateSfntTable(buf, "cmap");
    expect(entry).toEqual({ offset: 28, length: cmap.length });
    expect(buf.subarray(entry!.offset, entry!.offset + entry!.length)).toEqual(cmap);
  });

  it("returns null for a table the font doesn't have", () => {
    const buf = buildSfnt({});
    expect(locateSfntTable(buf, "cmap")).toBeNull();
  });

  it("returns null for a WOFF/WOFF2/unrecognised format", () => {
    const woff = new Uint8Array(64);
    woff.set([0x77, 0x4f, 0x46, 0x46], 0);
    expect(locateSfntTable(woff, "cmap")).toBeNull();
    expect(locateSfntTable(new Uint8Array([1, 2, 3]), "cmap")).toBeNull();
  });

  it("degrades gracefully (never throws) for the same fuzz cases as parseSfntMetadata", () => {
    const pastEof = buildSfnt({});
    const view1 = new DataView(pastEof.buffer);
    view1.setUint32(12 + 8, pastEof.length + 100_000);
    expect(() => locateSfntTable(pastEof, "name")).not.toThrow();

    const bogusTtc = new Uint8Array(32);
    const view2 = new DataView(bogusTtc.buffer);
    view2.setUint32(0, 0x74746366);
    view2.setUint32(12, 0xfffffff0);
    expect(() => locateSfntTable(bogusTtc, "cmap")).not.toThrow();
    expect(locateSfntTable(bogusTtc, "cmap")).toBeNull();
  });
});

describe("parseCmapCoverage", () => {
  it("parses format-4 (BMP) coverage into the exact codepoint set", () => {
    const codepoints = [0x41, 0x42, 0x43, 0x61, 0x62, 0x63, 0x20ac]; // ABC, abc, €
    const cmap = buildCmapTable(4, codepoints);
    const coverage = parseCmapCoverage(cmap);
    expect(coverage).not.toBeNull();
    expect([...coverage!].sort((a, b) => a - b)).toEqual([...codepoints].sort((a, b) => a - b));
  });

  it("parses format-12 (full-Unicode / astral-plane) coverage into the exact codepoint set", () => {
    // A CJK-ish block plus an emoji (astral plane, > 0xFFFF) — exactly what format 4 can't represent.
    const codepoints = [0x4e00, 0x4e01, 0x4e02, 0x1f600];
    const cmap = buildCmapTable(12, codepoints);
    const coverage = parseCmapCoverage(cmap);
    expect(coverage).not.toBeNull();
    expect([...coverage!].sort((a, b) => a - b)).toEqual([...codepoints].sort((a, b) => a - b));
  });

  it("prefers a full-Unicode (format-12-style) encoding record over a BMP-only one when both are present", () => {
    const format12 = buildCmapFormat12Subtable([0x1f600]); // astral-plane codepoint, only representable here
    const format4 = buildCmapFormat4Subtable([0x41]);
    const header = 4;
    const recordLen = 8;
    const rec1Offset = header + recordLen * 2;
    const rec2Offset = rec1Offset + format4.length;
    const buf = new Uint8Array(rec2Offset + format12.length);
    const view = new DataView(buf.buffer);
    view.setUint16(0, 0);
    view.setUint16(2, 2); // two encoding records
    // Record 0: Windows BMP (format 4) — listed FIRST, but should lose to record 1's full-Unicode rank.
    view.setUint16(header, 3);
    view.setUint16(header + 2, 1);
    view.setUint32(header + 4, rec1Offset);
    // Record 1: Windows full-Unicode (format 12)
    view.setUint16(header + 8, 3);
    view.setUint16(header + 10, 10);
    view.setUint32(header + 12, rec2Offset);
    buf.set(format4, rec1Offset);
    buf.set(format12, rec2Offset);

    const coverage = parseCmapCoverage(buf);
    expect(coverage).toEqual([0x1f600]); // picked the format-12 subtable, not the format-4 one
  });

  it("returns null for a cmap table with an unsupported version", () => {
    const buf = new Uint8Array(16);
    new DataView(buf.buffer).setUint16(0, 1); // version must be 0
    expect(parseCmapCoverage(buf)).toBeNull();
  });

  it("returns null for an unsupported subtable format (e.g. format 6)", () => {
    const subtableOffset = 12;
    const buf = new Uint8Array(subtableOffset + 4);
    const view = new DataView(buf.buffer);
    view.setUint16(0, 0);
    view.setUint16(2, 1);
    view.setUint16(4, 3);
    view.setUint16(6, 1);
    view.setUint32(8, subtableOffset);
    view.setUint16(subtableOffset, 6); // format 6 — not handled, caller should fall back
    expect(parseCmapCoverage(buf)).toBeNull();
  });

  // --- Fuzz-derived regression tests (CPE-1593), mirroring parseSfntMetadata's own ---------------

  it("degrades gracefully (no throw, no hang) for a format-4 subtable with a corrupt/huge segCount", () => {
    const cmap = buildCmapTable(4, [0x41]);
    const view = new DataView(cmap.buffer);
    const subtableOffset = 12; // matches buildCmapTable's layout (4 header + 8 record)
    view.setUint16(subtableOffset + 6, 0xfffe); // implausible segCountX2 against the buffer's real size
    expect(() => parseCmapCoverage(cmap)).not.toThrow();
  });

  it("degrades gracefully (no throw, returns partial coverage) for a format-12 subtable whose declared numGroups runs past EOF", () => {
    const cmap = buildCmapTable(12, [0x4e00, 0x4e01]);
    const view = new DataView(cmap.buffer);
    const subtableOffset = 12;
    view.setUint32(subtableOffset + 12, 100_000); // declares far more groups than the buffer actually holds
    const start = performance.now();
    let coverage: number[] | null = null;
    expect(() => (coverage = parseCmapCoverage(cmap))).not.toThrow();
    expect(performance.now() - start).toBeLessThan(50); // bounded by the buffer, not the bogus count
    // Whatever real groups fit before running out of buffer are still returned — not a hard failure.
    expect(coverage).not.toBeNull();
  });

  it("degrades gracefully (no throw) for a format-12 group whose range is absurdly large", () => {
    const buf = new Uint8Array(12 + 16 + 12); // header(4)+record(8) + subtable header(16) + one group(12)
    const view = new DataView(buf.buffer);
    view.setUint16(0, 0);
    view.setUint16(2, 1);
    view.setUint16(4, 3);
    view.setUint16(6, 10);
    view.setUint32(8, 12);
    const sub = 12;
    view.setUint16(sub, 12);
    view.setUint32(sub + 4, buf.length - sub);
    view.setUint32(sub + 12, 1); // numGroups = 1
    view.setUint32(sub + 16, 0); // startCharCode
    view.setUint32(sub + 20, 0xffffffff); // endCharCode — an absurd multi-billion-codepoint range
    expect(() => parseCmapCoverage(buf)).not.toThrow();
    expect(parseCmapCoverage(buf)).toEqual([]); // the group is skipped outright, not partially expanded
  });

  it("never throws across 20 rounds of random bytes of varying lengths", () => {
    for (let round = 0; round < 20; round++) {
      const len = 1 + Math.floor(Math.random() * 4096);
      const bytes = new Uint8Array(len);
      for (let i = 0; i < len; i++) bytes[i] = Math.floor(Math.random() * 256);
      expect(() => parseCmapCoverage(bytes)).not.toThrow();
    }
  });
});
