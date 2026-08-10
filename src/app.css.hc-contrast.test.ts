// CPE-1543 (epic CPE-1496, high contrast): guards the new `:root[data-theme="hc-light"]` and
// `:root[data-theme="hc-dark"]` blocks added to src/app.css. Three invariants:
//   (a) each hc semantic layer defines the SAME token set as `:root[data-theme="light"]` (CPE-1534)
//       — a token silently missing from an hc block would fall back to the light value, which is a
//       bug (e.g. a low-contrast light-grey border leaking into an otherwise high-contrast theme).
//   (b) every WCAG-relevant token pairing in BOTH hc palettes meets a STRICTER, AAA-inspired bar than
//       the normal light/dark guard (src/app.css.dark-contrast.test.ts, which asserts AA: >=4.5:1
//       text / >=3:1 non-text UI): here primary text and danger/status text must clear >=7:1 (WCAG
//       2.1 SC 1.4.6, AAA), and dimmed text / non-text UI (borders, accent, agent swatches) must
//       clear >=4.5:1 — well above the normal palette's 3:1 non-text bar. That's the point of a
//       high-contrast theme: every pairing gets the treatment normal body text gets elsewhere.
//   (c) both hc palettes set the matching `color-scheme` (hc-light -> light, hc-dark -> dark) so the
//       OS/browser chrome (scrollbars, form controls) doesn't render backwards.
// This is a pure text+math test — no jsdom/browser needed — mirroring CPE-1539's dark-contrast guard
// and CPE-1534's original completeness guard (src/app.css.test.ts). Both hc themes are inert until
// CPE-1544/1545/1546 let something set data-theme="hc-light"/"hc-dark" on the document; this test
// only checks the authored values are correct, not that anything currently renders them.
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const SRC = join(process.cwd(), "src");
const APP_CSS_PATH = join(SRC, "app.css");
const css = readFileSync(APP_CSS_PATH, "utf8");

const stripComments = (s: string) => s.replace(/\/\*[\s\S]*?\*\//g, "");

/** Bodies of every top-level block matching `selector { ... }` (brace-balanced), in source order. */
function allBlocks(source: string, selector: RegExp): string[] {
  const out: string[] = [];
  let m: RegExpExecArray | null;
  const re = new RegExp(selector, selector.flags.includes("g") ? selector.flags : selector.flags + "g");
  while ((m = re.exec(source)) !== null) {
    const open = source.indexOf("{", m.index);
    let depth = 0;
    for (let i = open; i < source.length; i++) {
      if (source[i] === "{") depth++;
      else if (source[i] === "}") {
        depth--;
        if (depth === 0) {
          out.push(source.slice(open + 1, i));
          break;
        }
      }
    }
  }
  return out;
}

/** name -> declared value (trimmed, comments stripped) for `--foo: value;` declarations in a block. */
function extractDecls(block: string): Map<string, string> {
  const clean = stripComments(block);
  const decls = new Map<string, string>();
  const re = /(--[a-zA-Z0-9-]+)\s*:\s*([^;]+);/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(clean)) !== null) decls.set(m[1], m[2].trim());
  return decls;
}

/** Every `--pal-hc-light-*`/`--pal-hc-dark-*: #hex` primitive declaration anywhere in the file,
 *  regardless of which :root block declares it (custom properties on :root don't care which rule
 *  block they're grouped under). */
function extractPalette(source: string, prefix: string): Map<string, string> {
  const clean = stripComments(source);
  const decls = new Map<string, string>();
  const re = new RegExp(`(--${prefix}[a-zA-Z0-9-]+)\\s*:\\s*(#[0-9a-fA-F]{3,8})\\s*;`, "g");
  let m: RegExpExecArray | null;
  while ((m = re.exec(clean)) !== null) decls.set(m[1], m[2]);
  return decls;
}

const lightBlocks = allBlocks(css, /:root\[data-theme="light"\]\s*\{/);
const hcLightBlocks = allBlocks(css, /:root\[data-theme="hc-light"\]\s*\{/);
const hcDarkBlocks = allBlocks(css, /:root\[data-theme="hc-dark"\]\s*\{/);

if (lightBlocks.length !== 1) throw new Error(`expected exactly one :root[data-theme="light"] block, found ${lightBlocks.length}`);
if (hcLightBlocks.length !== 1) throw new Error(`expected exactly one :root[data-theme="hc-light"] block, found ${hcLightBlocks.length}`);
if (hcDarkBlocks.length !== 1) throw new Error(`expected exactly one :root[data-theme="hc-dark"] block, found ${hcDarkBlocks.length}`);

const hcLightPaletteDecls = extractPalette(css, "pal-hc-light-");
const hcDarkPaletteDecls = extractPalette(css, "pal-hc-dark-");
const lightSemanticDecls = extractDecls(lightBlocks[0]);
const hcLightSemanticDecls = extractDecls(hcLightBlocks[0]);
const hcDarkSemanticDecls = extractDecls(hcDarkBlocks[0]);

if (hcLightPaletteDecls.size === 0) throw new Error("no --pal-hc-light-* primitive declarations found");
if (hcDarkPaletteDecls.size === 0) throw new Error("no --pal-hc-dark-* primitive declarations found");

/** Resolve a semantic token's value to a concrete hex through the given palette + semantic maps.
 *  Tokens that are plain values (e.g. `6px`, `30px`) or reference another semantic token (e.g.
 *  --agent-unknown -> var(--text-faint)) are resolved transitively. */
function resolveHex(
  value: string,
  paletteDecls: Map<string, string>,
  semanticDecls: Map<string, string>,
  depth = 0,
): string | undefined {
  if (depth > 5) return undefined;
  const hexMatch = value.match(/^#[0-9a-fA-F]{3,8}$/);
  if (hexMatch) return value;
  const varMatch = value.match(/^var\((--[a-zA-Z0-9-]+)\)$/);
  if (!varMatch) return undefined;
  const name = varMatch[1];
  if (paletteDecls.has(name)) return resolveHex(paletteDecls.get(name)!, paletteDecls, semanticDecls, depth + 1);
  if (semanticDecls.has(name)) return resolveHex(semanticDecls.get(name)!, paletteDecls, semanticDecls, depth + 1);
  return undefined;
}

// ---------------------------------------------------------------------------------------------
// WCAG 2.1 relative luminance + contrast ratio math (inline, no dependency —
// https://www.w3.org/TR/WCAG21/#dfn-relative-luminance / #dfn-contrast-ratio). Duplicated from
// src/app.css.dark-contrast.test.ts rather than shared, per that file's own precedent (keeps each
// guard test a single self-contained file, easy to delete if its palette is ever dropped).
function hexToRgb(hex: string): [number, number, number] {
  let h = hex.replace("#", "");
  if (h.length === 3) h = h.split("").map((c) => c + c).join("");
  const r = parseInt(h.substring(0, 2), 16);
  const g = parseInt(h.substring(2, 4), 16);
  const b = parseInt(h.substring(4, 6), 16);
  return [r, g, b];
}

function relativeLuminance(hex: string): number {
  const [r, g, b] = hexToRgb(hex).map((c) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrastRatio(hexA: string, hexB: string): number {
  const lA = relativeLuminance(hexA);
  const lB = relativeLuminance(hexB);
  const lighter = Math.max(lA, lB);
  const darker = Math.min(lA, lB);
  return (lighter + 0.05) / (darker + 0.05);
}

/** Same semantic token list CPE-1534/CPE-1539 established for :root[data-theme="light"]/"dark" —
 *  both hc blocks must resolve this exact set, with no drops. */
const SEMANTIC_TOKENS = [
  "--bg",
  "--surface",
  "--surface-alt",
  "--hover",
  "--active",
  "--border",
  "--border-strong",
  "--dialog-border",
  "--danger",
  "--success",
  "--danger-hover",
  "--text",
  "--text-dim",
  "--text-faint",
  "--accent",
  "--accent-hover",
  "--selection",
  "--selection-hover",
  "--radius",
  "--radius-lg",
  "--row-h",
  "--agent-1",
  "--agent-2",
  "--agent-3",
  "--agent-4",
  "--agent-5",
  "--agent-6",
  "--agent-user",
  "--agent-unknown",
];

describe("app.css hc palette completeness (CPE-1543)", () => {
  it(':root[data-theme="hc-light"] resolves the same semantic token set as :root[data-theme="light"]', () => {
    const missing = SEMANTIC_TOKENS.filter((name) => !hcLightSemanticDecls.has(name));
    expect(missing, `tokens missing from :root[data-theme="hc-light"]: ${missing.join(", ")}`).toEqual([]);
  });

  it(':root[data-theme="hc-dark"] resolves the same semantic token set as :root[data-theme="light"]', () => {
    const missing = SEMANTIC_TOKENS.filter((name) => !hcDarkSemanticDecls.has(name));
    expect(missing, `tokens missing from :root[data-theme="hc-dark"]: ${missing.join(", ")}`).toEqual([]);
  });

  it("sets color-scheme: light on hc-light and color-scheme: dark on hc-dark", () => {
    expect(stripComments(hcLightBlocks[0])).toMatch(/color-scheme\s*:\s*light\s*;/);
    expect(stripComments(hcDarkBlocks[0])).toMatch(/color-scheme\s*:\s*dark\s*;/);
  });

  it("every hc-light semantic token referencing a palette var points at one that exists", () => {
    const dangling: string[] = [];
    for (const [name, value] of hcLightSemanticDecls) {
      const ref = value.match(/^var\((--pal-hc-light-[a-zA-Z0-9-]+)\)$/);
      if (ref && !hcLightPaletteDecls.has(ref[1])) dangling.push(`${name} -> ${ref[1]}`);
    }
    expect(dangling, `hc-light semantic tokens referencing a non-existent palette var: ${dangling.join(", ")}`).toEqual([]);
  });

  it("every hc-dark semantic token referencing a palette var points at one that exists", () => {
    const dangling: string[] = [];
    for (const [name, value] of hcDarkSemanticDecls) {
      const ref = value.match(/^var\((--pal-hc-dark-[a-zA-Z0-9-]+)\)$/);
      if (ref && !hcDarkPaletteDecls.has(ref[1])) dangling.push(`${name} -> ${ref[1]}`);
    }
    expect(dangling, `hc-dark semantic tokens referencing a non-existent palette var: ${dangling.join(", ")}`).toEqual([]);
  });
});

/** Runs the full AAA-inspired assertion suite against one hc theme's resolved hex values. */
function describeContrast(
  label: string,
  semanticDecls: Map<string, string>,
  paletteDecls: Map<string, string>,
) {
  describe(`app.css ${label} palette AAA-inspired contrast (CPE-1543)`, () => {
    const resolve = (name: string) => resolveHex(semanticDecls.get(name)!, paletteDecls, semanticDecls);

    const bg = resolve("--bg")!;
    const surface = resolve("--surface")!;
    const text = resolve("--text")!;
    const textDim = resolve("--text-dim")!;
    const accent = resolve("--accent")!;
    const borderStrong = resolve("--border-strong")!;
    const dialogBorder = resolve("--dialog-border")!;
    const danger = resolve("--danger")!;
    const success = resolve("--success")!;

    it("all key tokens resolved to a concrete hex through the palette layer", () => {
      for (const [name, hex] of [
        ["--bg", bg],
        ["--surface", surface],
        ["--text", text],
        ["--text-dim", textDim],
        ["--accent", accent],
        ["--border-strong", borderStrong],
        ["--dialog-border", dialogBorder],
        ["--danger", danger],
        ["--success", success],
      ] as const) {
        expect(hex, `${name} did not resolve to a hex value`).toMatch(/^#[0-9a-fA-F]{6}$/);
      }
    });

    // Primary body text — AAA-inspired bar, WCAG 2.1 SC 1.4.6 recommends >=7:1 for normal text.
    it("--text on --bg and --surface >= 7:1 (AAA-inspired)", () => {
      expect(contrastRatio(text, bg)).toBeGreaterThanOrEqual(7);
      expect(contrastRatio(text, surface)).toBeGreaterThanOrEqual(7);
    });

    // Status/error text carries the same weight as primary text in a high-contrast theme — no
    // "secondary" pairing gets the normal palette's relaxed bar here.
    it("--danger on --bg and --surface >= 7:1 (AAA-inspired)", () => {
      expect(contrastRatio(danger, bg)).toBeGreaterThanOrEqual(7);
      expect(contrastRatio(danger, surface)).toBeGreaterThanOrEqual(7);
    });

    it("--success on --bg and --surface >= 7:1 (AAA-inspired)", () => {
      expect(contrastRatio(success, bg)).toBeGreaterThanOrEqual(7);
      expect(contrastRatio(success, surface)).toBeGreaterThanOrEqual(7);
    });

    // Dimmed/secondary text — still well above the normal palette's 3:1 large-text allowance.
    it("--text-dim on --bg >= 4.5:1 (stricter than the normal palette's 3:1 large-text bar)", () => {
      expect(contrastRatio(textDim, bg)).toBeGreaterThanOrEqual(4.5);
    });

    // Non-text UI (borders, accent) — stricter than the normal palette's 3:1 WCAG 1.4.11 bar.
    it("--border-strong and --dialog-border on --bg and --surface >= 4.5:1 (stricter non-text UI bar)", () => {
      expect(contrastRatio(borderStrong, bg)).toBeGreaterThanOrEqual(4.5);
      expect(contrastRatio(borderStrong, surface)).toBeGreaterThanOrEqual(4.5);
      expect(contrastRatio(dialogBorder, bg)).toBeGreaterThanOrEqual(4.5);
      expect(contrastRatio(dialogBorder, surface)).toBeGreaterThanOrEqual(4.5);
    });

    it("--accent on --bg and --surface >= 4.5:1 (stricter non-text UI bar)", () => {
      expect(contrastRatio(accent, bg)).toBeGreaterThanOrEqual(4.5);
      expect(contrastRatio(accent, surface)).toBeGreaterThanOrEqual(4.5);
    });

    it("the six agent colours + agent-user swatch >= 4.5:1 on --surface (stricter than the normal palette's 3:1)", () => {
      for (const name of ["--agent-1", "--agent-2", "--agent-3", "--agent-4", "--agent-5", "--agent-6", "--agent-user"]) {
        const hex = resolve(name);
        expect(hex, `${name} did not resolve to a hex value`).toMatch(/^#[0-9a-fA-F]{6}$/);
        const ratio = contrastRatio(hex!, surface);
        expect(ratio, `${name} (${hex}) vs --surface (${surface}) = ${ratio.toFixed(2)}:1, want >=4.5:1`).toBeGreaterThanOrEqual(
          4.5,
        );
      }
    });
  });
}

describeContrast("hc-light", hcLightSemanticDecls, hcLightPaletteDecls);
describeContrast("hc-dark", hcDarkSemanticDecls, hcDarkPaletteDecls);
