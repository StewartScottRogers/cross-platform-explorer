// CPE-1661: guards the colour-blind-vision (CVD) fix for the `keyword`/`title` collision the
// independent Visual Critic found on PR #850 (pixel-sampling CPE-1648's CVD screenshots), plus the
// "load-bearing cue" gap the same review surfaced. Companion to src/app.css.hljs-contrast.test.ts
// (WCAG luminance contrast — a separate, unaffected concern) and
// .claude/qa-architecture/CVD-SIMULATION-METHOD.md (the crew's one CVD method: Machado, Oliveira &
// Fialho 2009 dichromacy matrices, the same family Chrome DevTools' "Emulate vision deficiencies"
// panel uses, applied directly in sRGB space per that doc's own SVG `feColorMatrix
// color-interpolation-filters="sRGB"` convention — no linearRGB round-trip).
//
// Two independent things are pinned here, both text+math, no jsdom/browser needed (mirrors every
// sibling app.css.*.test.ts):
//
//   1. THE FIX (acceptance criteria bullets 1-3): in `light` and `hc-light` (and the bare `:root`
//      fallback, which shares light's palette), `keyword` and `title` must be told apart by a
//      non-colour channel, because a Machado simulation shows their hues are the closest of any
//      keyword-vs-* pair in those themes — nowhere near a `string`/`number`/`tag`/`comment`-style
//      safe margin. CPE-1661 fixed this by making `title` NOT bold in those themes (`keyword` stays
//      the sole bold token), rather than re-hueing `title` — see src/app.css's CPE-1661 comments for
//      the full "why this option, why it still reads well under normal vision" reasoning. `dark`/
//      `hc-dark` never had this collision (CPE-1648), so they're asserted UNCHANGED (title still
//      bold there, exactly as before this ticket).
//
//   2. THE LOAD-BEARING CUES (acceptance criteria bullet 4): `keyword` must stay bold and `comment`
//      must stay italic in EVERY theme — CPE-1648 found these are the ONLY thing separating
//      keyword/comment/tag in dark/hc-dark, where the three collapse into one near-neutral grey
//      family under CVD. Nothing before this ticket would have noticed either cue silently
//      disappearing; this file is that guard. It was verified red -> green by deliberately reverting
//      each cue locally and re-running `npx vitest run src/app.css.hljs-cvd.test.ts` before
//      committing — see the ticket's Work Log for the exact before/after failure output.
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const SRC = join(process.cwd(), "src");
const APP_CSS_PATH = join(SRC, "app.css");
const css = readFileSync(APP_CSS_PATH, "utf8");

const stripComments = (s: string) => s.replace(/\/\*[\s\S]*?\*\//g, "");

// ---------------------------------------------------------------------------------------------
// Minimal flat CSS-rule extractor. app.css has no @media/@supports/nested rules (verified — every
// block is a plain `selector-list { declarations }`), so a brace-balanced top-level split is exact
// and doesn't need a real CSS parser, mirroring app.css.hljs-contrast.test.ts's own approach.
// ---------------------------------------------------------------------------------------------
interface Rule {
  selectors: string[]; // trimmed, comma-split
  decls: Map<string, string>;
}

function extractRules(source: string): Rule[] {
  const clean = stripComments(source);
  const rules: Rule[] = [];
  const re = /([^{}]+)\{([^{}]*)\}/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(clean)) !== null) {
    const selectorText = m[1].trim();
    if (!selectorText) continue;
    const body = m[2];
    const decls = new Map<string, string>();
    const declRe = /([a-zA-Z-]+)\s*:\s*([^;]+);/g;
    let dm: RegExpExecArray | null;
    while ((dm = declRe.exec(body)) !== null) decls.set(dm[1].trim(), dm[2].trim());
    rules.push({
      selectors: selectorText.split(",").map((s) => s.replace(/\s+/g, " ").trim()),
      decls,
    });
  }
  return rules;
}

const rules = extractRules(css);

type Theme = "light" | "dark" | "hc-light" | "hc-dark";
const THEMES: Theme[] = ["light", "dark", "hc-light", "hc-dark"];

/** True if `selector` targets `className` (a bare `.hljs-foo`, or scoped
 *  `:root[data-theme="X"] .hljs-foo`) and, for a scoped selector, X === theme. A bare (unscoped)
 *  selector matches every theme — it's the fallback that a scoped override on the same property
 *  supersedes (later source order = higher or equal specificity in this file, verified by eye:
 *  every `:root[data-theme="X"] .foo` scoped rule in this file appears strictly AFTER the bare
 *  `.foo` rule it overrides). */
function selectorMatches(selector: string, className: string, theme: Theme): boolean {
  const bare = new RegExp(`(^|\\s)\\.${className}($|[\\s,])`);
  const scoped = new RegExp(`^:root\\[data-theme="([a-z-]+)"\\]\\s+\\.${className}$`);
  const scopedMatch = selector.match(scoped);
  if (scopedMatch) return scopedMatch[1] === theme;
  return bare.test(`${selector} `);
}

/** Effective value of `property` for `className` under `theme`: the LAST matching rule (bare or
 *  theme-scoped) in source order wins — this file's cascade for `.hljs-*` typographic rules is
 *  exactly "later declaration overrides earlier" (no rule here relies on specificity beating source
 *  order the other way). Returns undefined if no rule sets the property at all. */
function resolve(className: string, theme: Theme, property: string): string | undefined {
  let value: string | undefined;
  for (const rule of rules) {
    if (!rule.decls.has(property)) continue;
    if (rule.selectors.some((sel) => selectorMatches(sel, className, theme))) {
      value = rule.decls.get(property);
    }
  }
  return value;
}

// ---------------------------------------------------------------------------------------------
// Part 1: the fix. keyword and title must differ by a non-colour channel in light/hc-light (font
// weight, since CPE-1661 chose "stop bolding title" over re-hueing) and must be UNCHANGED (both
// still bold) in dark/hc-dark.
// ---------------------------------------------------------------------------------------------
describe("CPE-1661: keyword vs title CVD fix (light/hc-light get a non-colour cue; dark/hc-dark untouched)", () => {
  it("title is NOT bold in light, hc-light, or the bare :root fallback", () => {
    for (const theme of ["light", "hc-light"] as const) {
      const weight = resolve("hljs-title", theme, "font-weight");
      expect(weight, `${theme} .hljs-title font-weight`).toBe("400");
    }
  });

  it("keyword IS bold everywhere (the sole bold token in light/hc-light)", () => {
    for (const theme of THEMES) {
      const weight = resolve("hljs-keyword", theme, "font-weight");
      expect(weight, `${theme} .hljs-keyword font-weight`).toBe("600");
    }
  });

  it("keyword and title differ in font-weight in light and hc-light — the non-colour cue the ticket requires", () => {
    for (const theme of ["light", "hc-light"] as const) {
      const kw = resolve("hljs-keyword", theme, "font-weight");
      const title = resolve("hljs-title", theme, "font-weight");
      expect(kw, `${theme} keyword weight resolved`).toBeDefined();
      expect(title, `${theme} title weight resolved`).toBeDefined();
      expect(
        kw,
        `${theme}: keyword (${kw}) and title (${title}) must differ in font-weight — they no ` +
          `longer have a reliable hue-only distinction under CVD (see the redmean-distance test below)`,
      ).not.toBe(title);
    }
  });

  it("dark and hc-dark are UNCHANGED — title stays bold, same as keyword (no collision there, ticket forbids touching these)", () => {
    for (const theme of ["dark", "hc-dark"] as const) {
      const kw = resolve("hljs-keyword", theme, "font-weight");
      const title = resolve("hljs-title", theme, "font-weight");
      expect(title, `${theme} .hljs-title font-weight`).toBe("600");
      expect(kw, `${theme} .hljs-keyword font-weight`).toBe(title);
    }
  });
});

// ---------------------------------------------------------------------------------------------
// Part 2: quantify WHY a non-colour cue is required — Machado 2009 simulation shows keyword/title
// in light+hc-light are the closest-distance pair of all keyword-vs-* comparisons, nowhere near the
// safe margin every other pair in this palette has. Pure documentation/pinning — this does NOT
// gate on an arbitrary absolute threshold (which would be exactly the "matrix-chasing" anti-pattern
// CVD-SIMULATION-METHOD.md warns against); it gates on a RELATIVE ranking, which is robust to the
// exact matrix/rounding convention.
// ---------------------------------------------------------------------------------------------

// Machado, Oliveira & Fialho (2009) full-severity dichromacy matrices — copied verbatim from
// .claude/qa-architecture/CVD-SIMULATION-METHOD.md, applied directly to sRGB [0,1] channel values
// per that doc's own `color-interpolation-filters="sRGB"` convention (no linearRGB round-trip).
const PROTANOPIA: number[][] = [
  [0.152286, 1.052583, -0.204868],
  [0.114503, 0.786281, 0.099216],
  [-0.003882, -0.048116, 1.051998],
];
const DEUTERANOPIA: number[][] = [
  [0.367322, 0.860646, -0.227968],
  [0.280085, 0.672501, 0.047413],
  [-0.01182, 0.04294, 0.968881],
];

function hexToRgb01(hex: string): [number, number, number] {
  const h = hex.replace("#", "");
  return [
    parseInt(h.substring(0, 2), 16) / 255,
    parseInt(h.substring(2, 4), 16) / 255,
    parseInt(h.substring(4, 6), 16) / 255,
  ];
}
const clamp01 = (x: number) => Math.max(0, Math.min(1, x));
function simulate(hex: string, matrix: number[][]): [number, number, number] {
  const [r, g, b] = hexToRgb01(hex);
  return [
    clamp01(matrix[0][0] * r + matrix[0][1] * g + matrix[0][2] * b) * 255,
    clamp01(matrix[1][0] * r + matrix[1][1] * g + matrix[1][2] * b) * 255,
    clamp01(matrix[2][0] * r + matrix[2][1] * g + matrix[2][2] * b) * 255,
  ];
}
/** "redmean" perceptual colour distance — the same low-cost approximation CPE-1648's work log used. */
function redmean(a: [number, number, number], b: [number, number, number]): number {
  const [r1, g1, b1] = a;
  const [r2, g2, b2] = b;
  const rmean = (r1 + r2) / 2;
  const dR = r1 - r2,
    dG = g1 - g2,
    dB = b1 - b2;
  return Math.sqrt((2 + rmean / 256) * dR * dR + 4 * dG * dG + (2 + (255 - rmean) / 256) * dB * dB);
}

/** Every `--<prefix>*: #hex` primitive declaration anywhere in the file, regardless of which :root
 *  block declares it — same helper as app.css.hljs-contrast.test.ts. */
function extractPalette(source: string, prefix: string): Map<string, string> {
  const clean = stripComments(source);
  const decls = new Map<string, string>();
  const re = new RegExp(`(--${prefix}[a-zA-Z0-9-]+)\\s*:\\s*(#[0-9a-fA-F]{3,8})\\s*;`, "g");
  let m: RegExpExecArray | null;
  while ((m = re.exec(clean)) !== null) decls.set(m[1], m[2]);
  return decls;
}

const lightPalette = extractPalette(css, "pal-hljs-");
const hcLightPalette = extractPalette(css, "pal-hc-light-hljs-");

const BUCKETS = ["keyword", "title", "string", "comment", "number", "tag"] as const;

function paletteHex(palette: Map<string, string>, prefix: string, bucket: string): string {
  const hex = palette.get(`--${prefix}${bucket}`);
  if (!hex) throw new Error(`no --${prefix}${bucket} declaration found`);
  return hex;
}

describe("CPE-1661: Machado 2009 CVD simulation documents the keyword/title collision (light + hc-light)", () => {
  function distanceTo(
    palette: Map<string, string>,
    prefix: string,
    matrix: number[][],
    bucketA: string,
    bucketB: string,
  ): number {
    return redmean(
      simulate(paletteHex(palette, prefix, bucketA), matrix),
      simulate(paletteHex(palette, prefix, bucketB), matrix),
    );
  }
  function closestKeywordPair(palette: Map<string, string>, prefix: string, matrix: number[][]) {
    const kwSim = simulate(paletteHex(palette, prefix, "keyword"), matrix);
    let closest = { bucket: "", dist: Infinity };
    for (const bucket of BUCKETS) {
      if (bucket === "keyword") continue;
      const dist = redmean(kwSim, simulate(paletteHex(palette, prefix, bucket), matrix));
      if (dist < closest.dist) closest = { bucket, dist };
    }
    return closest;
  }

  // Protanopia: keyword/title is unambiguously the tightest keyword-vs-* pair in both light palettes
  // — clearly below every other pair (next-closest is ~2x farther) — which is exactly the collision
  // the ticket's own before/after table cites (`keyword` and `title` both landing in one
  // indigo-blue family). This is the strict, matrix-verifies-the-ticket assertion.
  for (const [label, palette, prefix, maxDist] of [
    ["light", lightPalette, "pal-hljs-", 40],
    ["hc-light", hcLightPalette, "pal-hc-light-hljs-", 45],
  ] as const) {
    it(`${label}/protanopia: title is keyword's closest-distance bucket, and the gap is tight`, () => {
      const closest = closestKeywordPair(palette, prefix, PROTANOPIA);
      expect(
        closest.bucket,
        `${label}/protanopia: expected 'title' to be keyword's closest simulated colour ` +
          `(distance ${closest.dist.toFixed(2)}), got '${closest.bucket}' — if this changed, the ` +
          `underlying collision this ticket fixed with a font-weight cue may have moved to a ` +
          `different pair, which would need its own non-colour cue`,
      ).toBe("title");
      expect(
        closest.dist,
        `${label}/protanopia: keyword-title distance (${closest.dist.toFixed(2)}) grew past the ` +
          `tight-collision band this test pins — re-check whether the pair still needs its cue`,
      ).toBeLessThan(maxDist);
    });
  }

  // Deuteranopia: keyword/title is NOT the single closest pair under this matrix (keyword/comment is
  // slightly tighter — but that pair already has its own long-standing cue, comment=italic, so it's
  // not a gap this ticket needs to touch). keyword/title is still a genuinely tight pair here, though
  // — pinned to a measured band so a future palette change that pushes it outside gets caught, without
  // asserting the brittle "must be the single minimum" claim that doesn't hold under this matrix.
  for (const [label, palette, prefix, min, max] of [
    ["light", lightPalette, "pal-hljs-", 70, 110],
    ["hc-light", hcLightPalette, "pal-hc-light-hljs-", 65, 105],
  ] as const) {
    it(`${label}/deuteranopia: keyword-title distance stays within its measured tight band`, () => {
      const dist = distanceTo(palette, prefix, DEUTERANOPIA, "keyword", "title");
      expect(dist, `${label}/deuteranopia keyword-title distance`).toBeGreaterThan(min);
      expect(dist, `${label}/deuteranopia keyword-title distance`).toBeLessThan(max);
    });
  }
});

// ---------------------------------------------------------------------------------------------
// Part 3: the load-bearing cues (CPE-1648's finding, ticket's 4th acceptance-criteria bullet).
// keyword/comment/tag collapse to one near-neutral grey family under CVD in dark/hc-dark; ONLY
// keyword=bold + comment=italic keep them apart there. Nothing before this ticket enforced either
// cue — this is that guard. Deliberately broken and confirmed red before being committed (see the
// ticket's Work Log for the exact vitest failure output from each broken variant).
// ---------------------------------------------------------------------------------------------
describe("CPE-1661: load-bearing typographic CVD cues (must never silently disappear)", () => {
  it("keyword renders bold (font-weight: 600) in every theme", () => {
    for (const theme of THEMES) {
      expect(resolve("hljs-keyword", theme, "font-weight"), `${theme} .hljs-keyword`).toBe("600");
    }
  });
  it("selector-tag (shares the .hljs-keyword rule) renders bold in every theme", () => {
    for (const theme of THEMES) {
      expect(resolve("hljs-selector-tag", theme, "font-weight"), `${theme} .hljs-selector-tag`).toBe("600");
    }
  });
  it("comment renders italic (font-style: italic) in every theme", () => {
    for (const theme of THEMES) {
      expect(resolve("hljs-comment", theme, "font-style"), `${theme} .hljs-comment`).toBe("italic");
    }
  });
  it("quote (shares the .hljs-comment rule) renders italic in every theme", () => {
    for (const theme of THEMES) {
      expect(resolve("hljs-quote", theme, "font-style"), `${theme} .hljs-quote`).toBe("italic");
    }
  });
  it("tag renders PLAIN (neither bold nor italic) in every theme — it's the un-cued member of the trio", () => {
    for (const theme of THEMES) {
      expect(resolve("hljs-tag", theme, "font-weight"), `${theme} .hljs-tag font-weight`).toBeUndefined();
      expect(resolve("hljs-tag", theme, "font-style"), `${theme} .hljs-tag font-style`).toBeUndefined();
    }
  });
});
