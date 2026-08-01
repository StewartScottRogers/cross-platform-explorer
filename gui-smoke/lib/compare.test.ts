// CPE-1170 — headless unit tests for the pixel-diff comparator. Runs under Node's built-in test
// runner (`node:test`) via `tsx`, so it's verifiable WITHOUT a `tauri build` or `tauri-driver` — the
// thing that makes this ticket possible to pin without WebView2 in this environment. Run with:
//   npm run test:unit          (from gui-smoke/)
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { after, before, describe, it } from "node:test";
import {
  BASELINES_DIR,
  comparePngBuffers,
  compareSnapshotToBaseline,
  decodePng,
  encodeRgbaPng,
} from "./compare.js";
import { SCREENSHOTS_DIR } from "./snap.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

/** Builds a flat RGBA buffer for a small solid-colour swatch, for synthetic test fixtures. */
function solidSwatch(width: number, height: number, r: number, g: number, b: number, a = 255): Uint8Array {
  const pixels = new Uint8Array(width * height * 4);
  for (let i = 0; i < width * height; i++) {
    pixels[i * 4] = r;
    pixels[i * 4 + 1] = g;
    pixels[i * 4 + 2] = b;
    pixels[i * 4 + 3] = a;
  }
  return pixels;
}

describe("decodePng / encodeRgbaPng round-trip", () => {
  it("decodes exactly what was encoded", () => {
    const width = 4;
    const height = 3;
    const pixels = solidSwatch(width, height, 10, 20, 30);
    // Give it some per-pixel variation so a naive all-same-byte bug can't hide.
    pixels[0] = 255;
    pixels[(width * height - 1) * 4 + 2] = 7;

    const png = encodeRgbaPng(width, height, pixels);
    const decoded = decodePng(png);

    assert.equal(decoded.width, width);
    assert.equal(decoded.height, height);
    assert.deepEqual(Array.from(decoded.pixels), Array.from(pixels));
  });

  it("rejects a buffer without the PNG signature", () => {
    assert.throws(() => decodePng(Buffer.from("not a png")), /bad 8-byte signature/);
  });
});

describe("comparePngBuffers — identical images", () => {
  it("reports zero diff for byte-identical captures", () => {
    const pixels = solidSwatch(10, 10, 100, 150, 200);
    const png = encodeRgbaPng(10, 10, pixels);

    const result = comparePngBuffers(png, png);

    assert.equal(result.diffPixels, 0);
    assert.equal(result.totalPixels, 100);
    assert.equal(result.diffRatio, 0);
    assert.equal(result.mismatchAboveThreshold, false);
    assert.equal(result.sizeMismatch, undefined);
  });

  it("reports zero diff for two separately-encoded images with the same pixels", () => {
    const pixels = solidSwatch(6, 6, 5, 5, 5);
    const a = encodeRgbaPng(6, 6, pixels);
    const b = encodeRgbaPng(6, 6, Uint8Array.from(pixels)); // distinct buffer, same content

    const result = comparePngBuffers(a, b);
    assert.equal(result.diffPixels, 0);
    assert.equal(result.mismatchAboveThreshold, false);
  });
});

describe("comparePngBuffers — one-pixel-changed images", () => {
  it("reports a nonzero diff confined to the changed pixel", () => {
    const width = 20;
    const height = 20; // 400 total pixels -> a 1px diff is 0.25%, under the 1% default threshold
    const base = solidSwatch(width, height, 0, 0, 0);
    const changed = Uint8Array.from(base);
    // Flip one pixel to white.
    changed[0] = 255;
    changed[1] = 255;
    changed[2] = 255;

    const a = encodeRgbaPng(width, height, base);
    const b = encodeRgbaPng(width, height, changed);

    const result = comparePngBuffers(a, b);
    assert.equal(result.diffPixels, 1);
    assert.equal(result.totalPixels, 400);
    assert.equal(result.diffRatio, 1 / 400);
    // Default 1% ratio threshold: a single differing pixel in 400 must NOT trip it.
    assert.equal(result.mismatchAboveThreshold, false);
  });

  it("flips mismatchAboveThreshold true once ratioThreshold is set below the observed diff", () => {
    const width = 5;
    const height = 5; // 25 total pixels -> 1 diff pixel = 4%
    const base = solidSwatch(width, height, 0, 0, 0);
    const changed = Uint8Array.from(base);
    changed[3 * 4] = 255; // change pixel index 3's red channel

    const a = encodeRgbaPng(width, height, base);
    const b = encodeRgbaPng(width, height, changed);

    const result = comparePngBuffers(a, b, { ratioThreshold: 0.01 });
    assert.equal(result.diffPixels, 1);
    assert.equal(result.mismatchAboveThreshold, true);
  });

  it("pixelTolerance absorbs a small per-channel delta", () => {
    const width = 4;
    const height = 4;
    const base = solidSwatch(width, height, 100, 100, 100);
    const changed = Uint8Array.from(base);
    changed[0] = 102; // +2 on the red channel of pixel 0

    const a = encodeRgbaPng(width, height, base);
    const b = encodeRgbaPng(width, height, changed);

    const tolerant = comparePngBuffers(a, b, { pixelTolerance: 5 });
    assert.equal(tolerant.diffPixels, 0);

    const strict = comparePngBuffers(a, b, { pixelTolerance: 0 });
    assert.equal(strict.diffPixels, 1);
  });
});

describe("comparePngBuffers — size mismatch", () => {
  it("flags a clear sizeMismatch instead of a meaningless pixel diff", () => {
    const a = encodeRgbaPng(4, 4, solidSwatch(4, 4, 1, 2, 3));
    const b = encodeRgbaPng(8, 4, solidSwatch(8, 4, 1, 2, 3));

    const result = comparePngBuffers(a, b);
    assert.equal(result.sizeMismatch, true);
    assert.equal(result.mismatchAboveThreshold, true);
    assert.match(result.error ?? "", /size mismatch/);
  });
});

describe("committed demo baselines under gui-smoke/baselines/", () => {
  // Proves the two small binaries committed alongside this ticket (generated by
  // scripts/bless-demo-baselines.ts, documented in baselines/README.md) are valid, decodable PNGs
  // and that comparing a baseline file against itself round-trips to zero diff — the same code path
  // `compareSnapshotToBaseline` runs, just exercised directly against the real committed files
  // rather than a scratch fixture.
  for (const name of ["demo-swatch", "demo-swatch-gradient"]) {
    it(`${name}.png decodes and self-compares to zero diff`, () => {
      const buf = fs.readFileSync(path.join(BASELINES_DIR, `${name}.png`));
      const decoded = decodePng(buf);
      assert.ok(decoded.width > 0 && decoded.height > 0);

      const result = comparePngBuffers(buf, buf);
      assert.equal(result.diffPixels, 0);
      assert.equal(result.mismatchAboveThreshold, false);
    });
  }
});

describe("compareSnapshotToBaseline — filesystem-integrated flow", () => {
  const name = "cpe-1170-unit-test-fixture";
  const capturedPath = path.join(SCREENSHOTS_DIR, `${name}.png`);
  const baselinePath = path.join(BASELINES_DIR, `${name}.png`);

  before(() => {
    fs.mkdirSync(SCREENSHOTS_DIR, { recursive: true });
    fs.mkdirSync(BASELINES_DIR, { recursive: true });
  });

  after(() => {
    // Never leave unit-test scratch fixtures behind — `baselines/` is committed, so a leaked file
    // here would look like a real golden baseline to the next `git status`.
    for (const p of [capturedPath, baselinePath]) {
      if (fs.existsSync(p)) fs.rmSync(p);
    }
    delete process.env.GUI_SMOKE_VISUAL_STRICT;
    delete process.env.GUI_SMOKE_UPDATE_BASELINE;
  });

  it("reports an advisory error (not a throw) when no baseline exists yet", async () => {
    fs.writeFileSync(capturedPath, encodeRgbaPng(4, 4, solidSwatch(4, 4, 9, 9, 9)));
    if (fs.existsSync(baselinePath)) fs.rmSync(baselinePath);

    const result = await compareSnapshotToBaseline(name);
    assert.equal(result.mismatchAboveThreshold, false);
    assert.match(result.error ?? "", /no baseline/);
  });

  it("GUI_SMOKE_UPDATE_BASELINE=1 blesses the current capture as the new baseline", async () => {
    const pixels = solidSwatch(4, 4, 42, 43, 44);
    fs.writeFileSync(capturedPath, encodeRgbaPng(4, 4, pixels));

    process.env.GUI_SMOKE_UPDATE_BASELINE = "1";
    try {
      await compareSnapshotToBaseline(name);
    } finally {
      delete process.env.GUI_SMOKE_UPDATE_BASELINE;
    }

    assert.equal(fs.existsSync(baselinePath), true);
    const blessed = decodePng(fs.readFileSync(baselinePath));
    assert.deepEqual(Array.from(blessed.pixels), Array.from(pixels));
  });

  it("compares a fresh capture against the just-blessed baseline: identical -> zero diff", async () => {
    const result = await compareSnapshotToBaseline(name);
    assert.equal(result.diffPixels, 0);
    assert.equal(result.mismatchAboveThreshold, false);
    assert.equal(result.error, undefined);
  });

  it("advisory by default: a captured drift beyond threshold logs, but does not throw", async () => {
    // Overwrite the capture with something wildly different from the blessed baseline.
    fs.writeFileSync(capturedPath, encodeRgbaPng(4, 4, solidSwatch(4, 4, 255, 0, 0)));

    const result = await compareSnapshotToBaseline(name);
    assert.equal(result.mismatchAboveThreshold, true);
  });

  it("GUI_SMOKE_VISUAL_STRICT=1 turns the same over-threshold drift into a thrown rejection", async () => {
    fs.writeFileSync(capturedPath, encodeRgbaPng(4, 4, solidSwatch(4, 4, 255, 0, 0)));

    process.env.GUI_SMOKE_VISUAL_STRICT = "1";
    try {
      await assert.rejects(() => compareSnapshotToBaseline(name), /visual diff/);
    } finally {
      delete process.env.GUI_SMOKE_VISUAL_STRICT;
    }
  });
});
