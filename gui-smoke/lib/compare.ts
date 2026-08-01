// CPE-1170 — visual-regression baseline/diff comparator for gui-smoke screenshots (MVD row 3).
//
// `lib/snap.ts` only *captures* a PNG. Nothing compared it to anything, so a visual regression
// (wrong theme colour, a broken layout, a menu going invisible-text-on-invisible-bg — see
// docs/design/MENUS.md) was only caught if a human, or the CPE-1148 Visual Critic, happened to open
// the shot. This module adds the missing half: a **pixel-diff comparator** that decodes two PNGs and
// reports `{ diffPixels, totalPixels, diffRatio, mismatchAboveThreshold }`.
//
// Zero new heavy dependencies. `gui-smoke/package-lock.json` has no `pngjs`/`pixelmatch` already
// installed (checked before writing this), and the ticket asks to keep it that way. The captures this
// harness produces are small, deterministic, non-interlaced 8-bit PNGs (WebView2/Chromium screenshot
// output), so a minimal hand-rolled PNG reader is enough — `decodePng` below parses chunks by hand and
// leans on Node's **built-in** `zlib` (not a new dependency) for the actual DEFLATE/zlib inflate.
//
// Scope / honesty about limits (do not paper over this): this decoder supports 8-bit-depth,
// non-interlaced PNGs with color type 0 (grayscale), 2 (truecolor/RGB), 4 (grayscale+alpha), or 6
// (truecolor+alpha/RGBA) — which covers every PNG a browser screenshot or `encodeRgbaPng` (below)
// produces. It deliberately does NOT support 16-bit channels, interlacing, or palette (color type 3)
// images — those throw a clear, named error rather than silently misreading pixels. Nothing in this
// harness needs them.
import fs from "node:fs";
import path from "node:path";
import zlib from "node:zlib";
import { fileURLToPath } from "node:url";
import { SCREENSHOTS_DIR } from "./snap.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

/** Committed golden baselines live here (NOT gitignored — these are the checked-in reference PNGs). */
export const BASELINES_DIR = path.resolve(__dirname, "..", "baselines");

const PNG_SIGNATURE = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);

export interface DecodedPng {
  width: number;
  height: number;
  /** RGBA, 4 bytes/pixel, row-major, top-to-bottom (PNG's native scanline order). */
  pixels: Uint8Array;
}

interface RawChunk {
  type: string;
  data: Buffer;
}

/** Splits a PNG file into its chunks. CRCs are not verified — this decoder trusts the encoder (a
 * real screenshot or our own `encodeRgbaPng`), the same trust boundary a full PNG library extends to
 * anything that isn't attacker-controlled input. */
function readChunks(buf: Buffer): RawChunk[] {
  if (buf.length < 8 || !buf.subarray(0, 8).equals(PNG_SIGNATURE)) {
    throw new Error("decodePng: not a PNG file (bad 8-byte signature)");
  }
  const chunks: RawChunk[] = [];
  let offset = 8;
  while (offset < buf.length) {
    if (offset + 8 > buf.length) throw new Error("decodePng: truncated chunk header");
    const length = buf.readUInt32BE(offset);
    const type = buf.toString("ascii", offset + 4, offset + 8);
    const dataStart = offset + 8;
    const dataEnd = dataStart + length;
    if (dataEnd + 4 > buf.length) throw new Error(`decodePng: truncated "${type}" chunk`);
    chunks.push({ type, data: buf.subarray(dataStart, dataEnd) });
    offset = dataEnd + 4; // skip the 4-byte CRC we don't verify
    if (type === "IEND") break;
  }
  return chunks;
}

/** Channel count per PNG color type, for the 8-bit-depth subset this decoder supports. Color type 3
 * (palette) is deliberately absent — it throws below instead of silently misreading index bytes as
 * samples. */
const CHANNELS_BY_COLOR_TYPE: Record<number, number> = { 0: 1, 2: 3, 4: 2, 6: 4 };

function paeth(a: number, b: number, c: number): number {
  const p = a + b - c;
  const pa = Math.abs(p - a);
  const pb = Math.abs(p - b);
  const pc = Math.abs(p - c);
  if (pa <= pb && pa <= pc) return a;
  if (pb <= pc) return b;
  return c;
}

/**
 * Decode an 8-bit, non-interlaced PNG buffer into flat RGBA pixels. Implements the full PNG
 * scanline-filter set (None/Sub/Up/Average/Paeth, spec section 9.2) — real encoders (including
 * Chromium's screenshot writer) pick filters adaptively per row, not just filter 0, so all five are
 * needed even though this module's own test fixtures only ever emit filter 0.
 */
export function decodePng(buf: Buffer): DecodedPng {
  const chunks = readChunks(buf);
  const ihdr = chunks.find((c) => c.type === "IHDR");
  if (!ihdr) throw new Error("decodePng: missing IHDR chunk");

  const width = ihdr.data.readUInt32BE(0);
  const height = ihdr.data.readUInt32BE(4);
  const bitDepth = ihdr.data.readUInt8(8);
  const colorType = ihdr.data.readUInt8(9);
  const compressionMethod = ihdr.data.readUInt8(10);
  const filterMethod = ihdr.data.readUInt8(11);
  const interlaceMethod = ihdr.data.readUInt8(12);

  if (bitDepth !== 8) {
    throw new Error(`decodePng: unsupported bit depth ${bitDepth} (only 8-bit PNGs are supported)`);
  }
  if (compressionMethod !== 0 || filterMethod !== 0) {
    throw new Error("decodePng: unsupported PNG compression/filter method");
  }
  if (interlaceMethod !== 0) {
    throw new Error("decodePng: interlaced PNGs are not supported");
  }
  const channels = CHANNELS_BY_COLOR_TYPE[colorType];
  if (!channels) {
    throw new Error(`decodePng: unsupported color type ${colorType} (palette images are not supported)`);
  }

  const idat = Buffer.concat(chunks.filter((c) => c.type === "IDAT").map((c) => c.data));
  const raw = zlib.inflateSync(idat); // IDAT payload is zlib-wrapped DEFLATE — a Node builtin handles it

  const bytesPerPixel = channels; // 8-bit depth => exactly 1 byte per channel
  const stride = width * bytesPerPixel;
  const pixels = new Uint8Array(width * height * 4);
  const prevRow = new Uint8Array(stride);
  const curRow = new Uint8Array(stride);

  let pos = 0;
  for (let y = 0; y < height; y++) {
    const filterType = raw[pos];
    pos += 1;
    for (let x = 0; x < stride; x++) {
      const rawByte = raw[pos + x];
      const a = x >= bytesPerPixel ? curRow[x - bytesPerPixel] : 0;
      const b = prevRow[x];
      const c = x >= bytesPerPixel ? prevRow[x - bytesPerPixel] : 0;
      switch (filterType) {
        case 0:
          curRow[x] = rawByte;
          break;
        case 1:
          curRow[x] = (rawByte + a) & 0xff;
          break;
        case 2:
          curRow[x] = (rawByte + b) & 0xff;
          break;
        case 3:
          curRow[x] = (rawByte + ((a + b) >> 1)) & 0xff;
          break;
        case 4:
          curRow[x] = (rawByte + paeth(a, b, c)) & 0xff;
          break;
        default:
          throw new Error(`decodePng: unsupported scanline filter type ${filterType}`);
      }
    }
    pos += stride;

    for (let px = 0; px < width; px++) {
      const srcOff = px * bytesPerPixel;
      const dstOff = (y * width + px) * 4;
      if (colorType === 6) {
        pixels[dstOff] = curRow[srcOff];
        pixels[dstOff + 1] = curRow[srcOff + 1];
        pixels[dstOff + 2] = curRow[srcOff + 2];
        pixels[dstOff + 3] = curRow[srcOff + 3];
      } else if (colorType === 2) {
        pixels[dstOff] = curRow[srcOff];
        pixels[dstOff + 1] = curRow[srcOff + 1];
        pixels[dstOff + 2] = curRow[srcOff + 2];
        pixels[dstOff + 3] = 255;
      } else if (colorType === 4) {
        const gray = curRow[srcOff];
        pixels[dstOff] = gray;
        pixels[dstOff + 1] = gray;
        pixels[dstOff + 2] = gray;
        pixels[dstOff + 3] = curRow[srcOff + 1];
      } else {
        // colorType === 0 (grayscale, no alpha)
        const gray = curRow[srcOff];
        pixels[dstOff] = gray;
        pixels[dstOff + 1] = gray;
        pixels[dstOff + 2] = gray;
        pixels[dstOff + 3] = 255;
      }
    }

    prevRow.set(curRow);
  }

  return { width, height, pixels };
}

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

/**
 * Encode flat RGBA pixels into a minimal (filter-type-0, non-interlaced, 8-bit RGBA) PNG buffer.
 * This exists for **test/fixture generation** — synthesizing the tiny synthetic PNGs
 * `compare.test.ts` feeds through `decodePng`/`comparePngBuffers`, and for blessing the demo
 * baselines under `gui-smoke/baselines/`. Production capture always goes through
 * `browser.saveScreenshot` (see `snap.ts`); this encoder is never used against a live app.
 */
export function encodeRgbaPng(width: number, height: number, pixels: Uint8Array): Buffer {
  if (pixels.length !== width * height * 4) {
    throw new Error("encodeRgbaPng: pixels.length must equal width*height*4");
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr.writeUInt8(8, 8); // bit depth
  ihdr.writeUInt8(6, 9); // color type: truecolor + alpha
  ihdr.writeUInt8(0, 10); // compression method
  ihdr.writeUInt8(0, 11); // filter method
  ihdr.writeUInt8(0, 12); // interlace method: none

  const stride = width * 4;
  const raw = Buffer.alloc((stride + 1) * height);
  for (let y = 0; y < height; y++) {
    const rowStart = y * (stride + 1);
    raw[rowStart] = 0; // scanline filter type: None
    Buffer.from(pixels.buffer, pixels.byteOffset + y * stride, stride).copy(raw, rowStart + 1);
  }
  const idat = zlib.deflateSync(raw);

  return Buffer.concat([
    PNG_SIGNATURE,
    pngChunk("IHDR", ihdr),
    pngChunk("IDAT", idat),
    pngChunk("IEND", Buffer.alloc(0)),
  ]);
}

export interface CompareOptions {
  /** Per-channel (0-255) absolute difference tolerated before a pixel counts as "different". Default
   * 0 (exact match required) — bump this if a future capture surface has legitimate sub-pixel
   * anti-aliasing noise. */
  pixelTolerance?: number;
  /** Fraction of differing pixels (0-1) above which `mismatchAboveThreshold` flips true. Default
   * 0.01 (1% of pixels). */
  ratioThreshold?: number;
}

export interface CompareResult {
  diffPixels: number;
  totalPixels: number;
  diffRatio: number;
  mismatchAboveThreshold: boolean;
  /** True when the two images are different dimensions — `diffPixels`/`diffRatio` are meaningless
   * (0 / 1 by convention) and the comparison is structurally invalid, not just "very different". */
  sizeMismatch?: boolean;
  /** Set on any non-fatal problem (decode failure, missing baseline, size mismatch) so a caller can
   * log/report it without the comparator throwing. */
  error?: string;
}

/** Pure, synchronous, in-memory pixel-diff between two decoded PNG buffers. No filesystem access —
 * see `compareSnapshotToBaseline` below for the baseline-file-aware wrapper specs actually call. */
export function comparePngBuffers(a: Buffer, b: Buffer, opts: CompareOptions = {}): CompareResult {
  const pixelTolerance = opts.pixelTolerance ?? 0;
  const ratioThreshold = opts.ratioThreshold ?? 0.01;

  const imgA = decodePng(a);
  const imgB = decodePng(b);

  if (imgA.width !== imgB.width || imgA.height !== imgB.height) {
    return {
      diffPixels: 0,
      totalPixels: 0,
      diffRatio: 1,
      mismatchAboveThreshold: true,
      sizeMismatch: true,
      error: `size mismatch: ${imgA.width}x${imgA.height} vs ${imgB.width}x${imgB.height}`,
    };
  }

  const totalPixels = imgA.width * imgA.height;
  let diffPixels = 0;
  for (let i = 0; i < totalPixels; i++) {
    const o = i * 4;
    const dr = Math.abs(imgA.pixels[o] - imgB.pixels[o]);
    const dg = Math.abs(imgA.pixels[o + 1] - imgB.pixels[o + 1]);
    const db = Math.abs(imgA.pixels[o + 2] - imgB.pixels[o + 2]);
    const da = Math.abs(imgA.pixels[o + 3] - imgB.pixels[o + 3]);
    if (dr > pixelTolerance || dg > pixelTolerance || db > pixelTolerance || da > pixelTolerance) {
      diffPixels++;
    }
  }
  const diffRatio = totalPixels === 0 ? 0 : diffPixels / totalPixels;

  return {
    diffPixels,
    totalPixels,
    diffRatio,
    mismatchAboveThreshold: diffRatio > ratioThreshold,
  };
}

/** `GUI_SMOKE_VISUAL_STRICT=1` promotes an over-threshold visual diff from an advisory log line into
 * a thrown Error (failing the calling spec). Documented in gui-smoke/README.md. Default: unset =
 * advisory-only, matching every other non-blocking gui-smoke signal (CPE-1048). */
export function visualStrictMode(): boolean {
  return process.env.GUI_SMOKE_VISUAL_STRICT === "1";
}

/** `GUI_SMOKE_UPDATE_BASELINE=1` — the "bless" step: instead of comparing, copy the just-captured PNG
 * over the golden baseline. Run once (locally or as a one-off CI job) after an intentional visual
 * change, review the diff in the PR, then commit the updated `gui-smoke/baselines/<name>.png`. */
function updateBaselineMode(): boolean {
  return process.env.GUI_SMOKE_UPDATE_BASELINE === "1";
}

/**
 * The wrapper a spec actually calls: compares the PNG `snap(name)` just wrote in
 * `gui-smoke/.screenshots/<name>.png` against the committed golden baseline
 * `gui-smoke/baselines/<name>.png`.
 *
 * Advisory by default — mirrors `snap()`'s own "never mask a real assertion" swallow: any I/O or
 * decode failure (missing capture, missing baseline, corrupt PNG) is logged and returned as an
 * `error` on the result rather than thrown, and an over-threshold diff is only a `console.warn` by
 * default. Set `GUI_SMOKE_VISUAL_STRICT=1` to make an over-threshold diff `throw` instead — the one
 * intentional exception to "never masks/never fails", for whoever wants this to gate a build.
 *
 * No baseline yet? Returns an advisory `error` (not a throw) — run with
 * `GUI_SMOKE_UPDATE_BASELINE=1` to bless one first (see gui-smoke/README.md).
 */
export async function compareSnapshotToBaseline(
  name: string,
  opts?: CompareOptions,
): Promise<CompareResult> {
  const capturedPath = path.join(SCREENSHOTS_DIR, `${name}.png`);
  const baselinePath = path.join(BASELINES_DIR, `${name}.png`);
  const emptyResult: CompareResult = {
    diffPixels: 0,
    totalPixels: 0,
    diffRatio: 0,
    mismatchAboveThreshold: false,
  };

  if (updateBaselineMode()) {
    try {
      fs.mkdirSync(BASELINES_DIR, { recursive: true });
      fs.copyFileSync(capturedPath, baselinePath);
      // eslint-disable-next-line no-console
      console.log(`[gui-smoke] blessed baseline "${name}" <- ${capturedPath}`);
      return emptyResult;
    } catch (err) {
      const msg = `bless failed for "${name}" (non-fatal): ${err instanceof Error ? err.message : String(err)}`;
      console.error(`[gui-smoke] ${msg}`);
      return { ...emptyResult, error: msg };
    }
  }

  if (!fs.existsSync(baselinePath)) {
    const msg = `no baseline at ${baselinePath} for "${name}" — run with GUI_SMOKE_UPDATE_BASELINE=1 to bless one`;
    console.warn(`[gui-smoke] ${msg}`);
    return { ...emptyResult, error: msg };
  }

  let result: CompareResult;
  try {
    const captured = fs.readFileSync(capturedPath);
    const baseline = fs.readFileSync(baselinePath);
    result = comparePngBuffers(baseline, captured, opts);
  } catch (err) {
    const msg = `comparison failed for "${name}" (non-fatal, advisory only): ${err instanceof Error ? err.message : String(err)}`;
    console.error(`[gui-smoke] ${msg}`);
    return { ...emptyResult, error: msg };
  }

  if (result.mismatchAboveThreshold) {
    const pct = (result.diffRatio * 100).toFixed(2);
    const msg = `visual diff "${name}": ${result.diffPixels}/${result.totalPixels}px (${pct}%)${
      result.sizeMismatch ? " — SIZE MISMATCH" : ""
    } exceeds threshold`;
    if (visualStrictMode()) {
      // Intentional: strict mode turns this into a real assertion failure, so it must propagate —
      // never caught/swallowed on this path, unlike every I/O failure above.
      throw new Error(`[gui-smoke] ${msg}`);
    }
    console.warn(`[gui-smoke] ${msg} (advisory only — set GUI_SMOKE_VISUAL_STRICT=1 to fail on this)`);
  }

  return result;
}
