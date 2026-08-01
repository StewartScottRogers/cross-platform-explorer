// CPE-1170 — one-off generator for the two synthetic demo baselines committed under
// `gui-smoke/baselines/`. Not part of any npm script (nothing needs to regenerate these at test
// time) — kept here purely so the provenance of `demo-swatch.png` / `demo-swatch-gradient.png` is
// reproducible from source rather than being unexplained committed binaries. Run manually with:
//   npx tsx scripts/bless-demo-baselines.ts
import fs from "node:fs";
import path from "node:path";
import { encodeRgbaPng, BASELINES_DIR } from "../lib/compare.js";

function solid(width: number, height: number, r: number, g: number, b: number): Uint8Array {
  const pixels = new Uint8Array(width * height * 4);
  for (let i = 0; i < width * height; i++) {
    pixels[i * 4] = r;
    pixels[i * 4 + 1] = g;
    pixels[i * 4 + 2] = b;
    pixels[i * 4 + 3] = 255;
  }
  return pixels;
}

function gradient(width: number, height: number): Uint8Array {
  const pixels = new Uint8Array(width * height * 4);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const o = (y * width + x) * 4;
      pixels[o] = Math.round((x / (width - 1)) * 255);
      pixels[o + 1] = Math.round((y / (height - 1)) * 255);
      pixels[o + 2] = 128;
      pixels[o + 3] = 255;
    }
  }
  return pixels;
}

fs.mkdirSync(BASELINES_DIR, { recursive: true });

fs.writeFileSync(
  path.join(BASELINES_DIR, "demo-swatch.png"),
  encodeRgbaPng(16, 16, solid(16, 16, 60, 120, 200)),
);
fs.writeFileSync(
  path.join(BASELINES_DIR, "demo-swatch-gradient.png"),
  encodeRgbaPng(16, 16, gradient(16, 16)),
);

// eslint-disable-next-line no-console
console.log(`wrote demo baselines to ${BASELINES_DIR}`);
