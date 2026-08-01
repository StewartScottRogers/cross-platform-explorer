// CPE-1172 — reviewer follow-up from CPE-1170 (PR #486): `decodePng` implements all five PNG
// scanline filters (None/Sub/Up/Average/Paeth, spec section 9.2), but this module's own
// `encodeRgbaPng` only ever emits filter type 0 (None) — so the committed suite in
// `compare.test.ts` exercised only the None branch. The Sub/Up/Average/Paeth predictor branches
// are exactly the ones that decode LIVE Chromium/WebView2 screenshots (real encoders pick filters
// adaptively per row), so a silent regression there would slip straight past `npm run test:unit`.
//
// This file closes that gap by hand-constructing valid PNG buffers whose IDAT scanlines use
// filter types 1-4, independent of `encodeRgbaPng`. It re-implements the minimal chunk/CRC
// plumbing `encodeRgbaPng` uses (that plumbing isn't exported from compare.ts) and applies the
// PNG-spec *filter* transform (the encode-side inverse of the *defilter* math in `decodePng`) by
// hand, per scanline. No new dependency — `zlib.deflateSync` is a Node builtin, same as
// `encodeRgbaPng` already uses.
import assert from "node:assert/strict";
import zlib from "node:zlib";
import { describe, it } from "node:test";
import { decodePng } from "./compare.js";

const PNG_SIGNATURE = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);

// --- Minimal chunk/CRC plumbing (mirrors encodeRgbaPng's private helpers in compare.ts; not
// exported from there, so reimplemented here for fixture construction). ---
const CRC_TABLE = (() => {
  const table = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) {
      c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    }
    table[n] = c >>> 0;
  }
  return table;
})();

function crc32(buf: Buffer): number {
  let crc = 0xffffffff;
  for (let i = 0; i < buf.length; i++) {
    crc = CRC_TABLE[(crc ^ buf[i]) & 0xff] ^ (crc >>> 8);
  }
  return (crc ^ 0xffffffff) >>> 0;
}

function pngChunk(type: string, data: Buffer): Buffer {
  const typeBuf = Buffer.from(type, "ascii");
  const lenBuf = Buffer.alloc(4);
  lenBuf.writeUInt32BE(data.length, 0);
  const crcBuf = Buffer.alloc(4);
  crcBuf.writeUInt32BE(crc32(Buffer.concat([typeBuf, data])), 0);
  return Buffer.concat([lenBuf, typeBuf, data, crcBuf]);
}

// --- PNG-spec scanline *filter* transform (encode direction — the inverse of decodePng's
// defilter switch). `raw` is the TRUE, unfiltered pixel byte at position x; a/b/c are the true
// (unfiltered) neighbour bytes per spec section 9.2 (left / above / above-left). ---
function paethPredictor(a: number, b: number, c: number): number {
  const p = a + b - c;
  const pa = Math.abs(p - a);
  const pb = Math.abs(p - b);
  const pc = Math.abs(p - c);
  if (pa <= pb && pa <= pc) return a;
  if (pb <= pc) return b;
  return c;
}

/** Applies filter `type` (0-4) to one true (unfiltered) scanline, producing the filtered bytes
 * that would appear in the IDAT stream. `prevRow` is the true previous scanline's bytes (all
 * zero for row 0, per spec). */
function filterScanline(type: 0 | 1 | 2 | 3 | 4, rawRow: Buffer, prevRow: Buffer, bpp: number): Buffer {
  const stride = rawRow.length;
  const out = Buffer.alloc(stride);
  for (let x = 0; x < stride; x++) {
    const raw = rawRow[x];
    const a = x >= bpp ? rawRow[x - bpp] : 0;
    const b = prevRow[x];
    const c = x >= bpp ? prevRow[x - bpp] : 0;
    let filtered: number;
    switch (type) {
      case 0:
        filtered = raw;
        break;
      case 1:
        filtered = (raw - a) & 0xff;
        break;
      case 2:
        filtered = (raw - b) & 0xff;
        break;
      case 3:
        filtered = (raw - Math.floor((a + b) / 2)) & 0xff;
        break;
      case 4:
        filtered = (raw - paethPredictor(a, b, c)) & 0xff;
        break;
    }
    out[x] = filtered;
  }
  return out;
}

/**
 * Hand-builds a valid, minimal 8-bit non-interlaced PNG whose IDAT applies `filterTypes[y]` to
 * row `y`. `channels` must match `colorType` (3 for RGB/2, 4 for RGBA/6). `pixelRows` holds the
 * TRUE (unfiltered) pixel bytes per row, `width * channels` bytes each.
 */
function buildFilteredPng(
  width: number,
  height: number,
  colorType: 2 | 6,
  channels: number,
  pixelRows: Buffer[],
  filterTypes: (0 | 1 | 2 | 3 | 4)[],
): Buffer {
  assert.equal(pixelRows.length, height);
  assert.equal(filterTypes.length, height);

  const stride = width * channels;
  const scanlines: Buffer[] = [];
  let prevRow: Buffer = Buffer.alloc(stride); // zero row, per spec, for row 0's "above" neighbour
  for (let y = 0; y < height; y++) {
    const rawRow: Buffer = pixelRows[y];
    assert.equal(rawRow.length, stride, `row ${y} must be ${stride} bytes`);
    const filtered = filterScanline(filterTypes[y], rawRow, prevRow, channels);
    scanlines.push(Buffer.concat([Buffer.from([filterTypes[y]]), filtered]));
    prevRow = rawRow;
  }

  const rawImageData = Buffer.concat(scanlines);
  const idatData = zlib.deflateSync(rawImageData);

  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr.writeUInt8(8, 8); // bit depth
  ihdr.writeUInt8(colorType, 9);
  ihdr.writeUInt8(0, 10); // compression method
  ihdr.writeUInt8(0, 11); // filter method
  ihdr.writeUInt8(0, 12); // interlace method: none

  return Buffer.concat([
    PNG_SIGNATURE,
    pngChunk("IHDR", ihdr),
    pngChunk("IDAT", idatData),
    pngChunk("IEND", Buffer.alloc(0)),
  ]);
}

/** Deliberately-non-uniform byte generator so neighbouring bytes (a/b/c) are never trivially
 * equal — a broken predictor that e.g. ignores `b` or `c` entirely would still "accidentally"
 * decode correctly against all-same-value fixtures. */
function syntheticByte(x: number, y: number, channel: number): number {
  return (x * 37 + y * 61 + channel * 17 + 5) % 256;
}

function buildRawRows(width: number, height: number, channels: number): Buffer[] {
  const rows: Buffer[] = [];
  for (let y = 0; y < height; y++) {
    const row = Buffer.alloc(width * channels);
    for (let x = 0; x < width; x++) {
      for (let c = 0; c < channels; c++) {
        row[x * channels + c] = syntheticByte(x, y, c);
      }
    }
    rows.push(row);
  }
  return rows;
}

/** Reads back the filter-type byte (the first byte of every scanline) straight out of the
 * inflated IDAT stream — independent of `decodePng` — so the test can assert the fixture really
 * does carry non-zero filter bytes rather than trusting the construction code blindly. */
function inflatedFilterBytes(png: Buffer, height: number, stride: number): number[] {
  // Re-parse just enough to find IDAT: PNG signature (8) + IHDR chunk (8 header + 13 data + 4
  // crc = 25 bytes) puts IDAT's length field at byte 33.
  let offset = 8;
  const chunks: { type: string; data: Buffer }[] = [];
  while (offset < png.length) {
    const length = png.readUInt32BE(offset);
    const type = png.toString("ascii", offset + 4, offset + 8);
    const data = png.subarray(offset + 8, offset + 8 + length);
    chunks.push({ type, data });
    offset += 8 + length + 4;
    if (type === "IEND") break;
  }
  const idat = Buffer.concat(chunks.filter((c) => c.type === "IDAT").map((c) => c.data));
  const raw = zlib.inflateSync(idat);
  const bytes: number[] = [];
  let pos = 0;
  for (let y = 0; y < height; y++) {
    bytes.push(raw[pos]);
    pos += 1 + stride;
  }
  return bytes;
}

function expectedRgbaPixels(rows: Buffer[], width: number, height: number, channels: number): Uint8Array {
  const pixels = new Uint8Array(width * height * 4);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const srcOff = x * channels;
      const dstOff = (y * width + x) * 4;
      pixels[dstOff] = rows[y][srcOff];
      pixels[dstOff + 1] = rows[y][srcOff + 1];
      pixels[dstOff + 2] = rows[y][srcOff + 2];
      pixels[dstOff + 3] = channels === 4 ? rows[y][srcOff + 3] : 255;
    }
  }
  return pixels;
}

describe("decodePng — scanline filter predictor coverage (CPE-1172)", () => {
  it("decodes an RGBA image using a different filter on each row: Sub(1)/Up(2)/Average(3)/Paeth(4)", () => {
    const width = 5;
    const height = 4;
    const channels = 4;
    const filterTypes: (0 | 1 | 2 | 3 | 4)[] = [1, 2, 3, 4];
    const rows = buildRawRows(width, height, channels);

    const png = buildFilteredPng(width, height, 6, channels, rows, filterTypes);

    // Sanity check, independent of decodePng: confirm the fixture's own IDAT bytes really do carry
    // filter types 1-4 (not 0) before trusting anything decodePng says about it.
    const actualFilterBytes = inflatedFilterBytes(png, height, width * channels);
    assert.deepEqual(actualFilterBytes, filterTypes, "fixture must actually use filters 1-4, not 0");

    const decoded = decodePng(png);
    assert.equal(decoded.width, width);
    assert.equal(decoded.height, height);
    assert.deepEqual(Array.from(decoded.pixels), Array.from(expectedRgbaPixels(rows, width, height, channels)));
  });

  it("decodes an all-Paeth(4) RGB (colorType 2) image", () => {
    const width = 6;
    const height = 5;
    const channels = 3;
    const filterTypes: (0 | 1 | 2 | 3 | 4)[] = [4, 4, 4, 4, 4];
    const rows = buildRawRows(width, height, channels);

    const png = buildFilteredPng(width, height, 2, channels, rows, filterTypes);

    const actualFilterBytes = inflatedFilterBytes(png, height, width * channels);
    assert.deepEqual(actualFilterBytes, filterTypes, "fixture must actually use filter 4 throughout, not 0");
    // Rows after the first have a real (non-zero) previous row, so Paeth genuinely chooses among
    // a/b/c rather than degenerating to the row-0 zero-prevRow case.
    assert.ok(actualFilterBytes.every((f) => f === 4));

    const decoded = decodePng(png);
    assert.equal(decoded.width, width);
    assert.equal(decoded.height, height);
    assert.deepEqual(Array.from(decoded.pixels), Array.from(expectedRgbaPixels(rows, width, height, channels)));
  });

  it("decodes a single-row Up(2)-filtered RGBA image (prior row is the spec's all-zero row)", () => {
    // Row 0's "prior" row is defined as all-zero, so filter type 2 (Up) on row 0 alone is a
    // distinct, minimal case: filtered byte = raw byte (since b=0), exercising the decode-side
    // `(rawByte + b) & 0xff` arithmetic at its edge (b=0) as well as the general case via
    // additional rows below.
    const width = 4;
    const height = 2;
    const channels = 4;
    const filterTypes: (0 | 1 | 2 | 3 | 4)[] = [2, 2];
    const rows = buildRawRows(width, height, channels);

    const png = buildFilteredPng(width, height, 6, channels, rows, filterTypes);
    const actualFilterBytes = inflatedFilterBytes(png, height, width * channels);
    assert.deepEqual(actualFilterBytes, [2, 2]);

    const decoded = decodePng(png);
    assert.deepEqual(Array.from(decoded.pixels), Array.from(expectedRgbaPixels(rows, width, height, channels)));
  });
});

describe("decodePng — unsupported formats throw a clear, named error", () => {
  function minimalIhdrOnlyPng(overrides: Partial<{
    bitDepth: number;
    colorType: number;
    interlace: number;
  }>): Buffer {
    const ihdr = Buffer.alloc(13);
    ihdr.writeUInt32BE(2, 0); // width
    ihdr.writeUInt32BE(2, 4); // height
    ihdr.writeUInt8(overrides.bitDepth ?? 8, 8);
    ihdr.writeUInt8(overrides.colorType ?? 6, 9);
    ihdr.writeUInt8(0, 10); // compression method
    ihdr.writeUInt8(0, 11); // filter method
    ihdr.writeUInt8(overrides.interlace ?? 0, 12);
    return Buffer.concat([PNG_SIGNATURE, pngChunk("IHDR", ihdr), pngChunk("IEND", Buffer.alloc(0))]);
  }

  it("rejects 16-bit depth with a named error", () => {
    const png = minimalIhdrOnlyPng({ bitDepth: 16 });
    assert.throws(() => decodePng(png), /unsupported bit depth 16/);
  });

  it("rejects interlaced PNGs with a named error", () => {
    const png = minimalIhdrOnlyPng({ interlace: 1 });
    assert.throws(() => decodePng(png), /interlaced PNGs are not supported/);
  });

  it("rejects palette (colorType 3) PNGs with a named error", () => {
    const png = minimalIhdrOnlyPng({ colorType: 3 });
    assert.throws(() => decodePng(png), /unsupported color type 3.*palette/);
  });
});
