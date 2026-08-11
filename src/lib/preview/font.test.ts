// CPE-1586 (epic CPE-1568 slice 5): pure helpers behind the font preview's glyph grid + metadata sniff.
import { describe, it, expect } from "vitest";
import {
  FONT_GLYPH_CANDIDATES,
  GLYPH_GRID_CAP,
  capGlyphs,
  glyphChar,
  codepointLabel,
  sniffFontFormat,
  formatLabelForExt,
  parseSfntMetadata,
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
  it("passes a list at or under the cap through unchanged", () => {
    const result = capGlyphs([1, 2, 3], 5);
    expect(result).toEqual({ shown: [1, 2, 3], total: 3, truncated: false });
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
});
