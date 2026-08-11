// CPE-1631: guards the `--hljs-*` syntax-highlighting tokens added to src/app.css (backing the
// `.hljs-*` rules that colour highlight.js's markup — see src/lib/preview/highlight.ts and the new
// CSS section near the end of app.css). Before this ticket, NOTHING in the app defined a `.hljs-*`
// rule at all, so every code view rendered flat monochrome in both themes — a silent, total failure
// of a shipped feature invisible to the whole test suite (only caught by a Visual Critic looking at
// real Chrome). This is a pure text+math test — no jsdom/browser needed, mirroring
// src/app.css.dark-contrast.test.ts and src/app.css.hc-contrast.test.ts — and asserts two things a
// browser screenshot can't be relied on to catch in CI:
//   (a) all six --hljs-* tokens are defined in EVERY theme block (bare :root fallback, light, dark,
//       hc-light, hc-dark) — a token silently missing from one would fall back to whatever the
//       cascade resolves to next, which is exactly the kind of gap this ticket exists to close;
//   (b) every token clears WCAG AA >=4.5:1 against BOTH --surface and --surface-alt in light/dark
//       (code is small monospace text — the ticket calls out the stricter body-text bar, not the
//       3:1 non-text-UI bar the --agent-* swatches use), and the stricter AAA-inspired >=7:1 bar in
//       hc-light/hc-dark, matching src/app.css.hc-contrast.test.ts's convention for that palette.
// This test does NOT and CANNOT assert that `.hljs-*` actually renders visibly correct highlighting
// in a real browser — see the ticket's work log for that verification (screenshot evidence, both
// themes, via scripts/dev-harness/hljs-theme).
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

/** Every `--<prefix>*: #hex` primitive declaration anywhere in the file, regardless of which :root
 *  block declares it (custom properties on :root don't care which rule block groups them). */
function extractPalette(source: string, prefix: string): Map<string, string> {
  const clean = stripComments(source);
  const decls = new Map<string, string>();
  const re = new RegExp(`(--${prefix}[a-zA-Z0-9-]+)\\s*:\\s*(#[0-9a-fA-F]{3,8})\\s*;`, "g");
  let m: RegExpExecArray | null;
  while ((m = re.exec(clean)) !== null) decls.set(m[1], m[2]);
  return decls;
}

const bareRootBlocks = allBlocks(css, /:root\s*\{/);
// Same discriminators src/app.css.test.ts (CPE-1534) and src/app.css.dark-contrast.test.ts (CPE-1539)
// use for these same three bare :root blocks — `.find()` returns the first match in source order,
// and only the true palette blocks declare raw `--pal-*`/`--pal-dark-*` primitives at all, so this is
// safe even though the SEMANTIC block's `--hljs-keyword: var(--pal-hljs-keyword)` value also contains
// the substring "--pal-".
const lightPaletteBlock = bareRootBlocks.find((b) => /--pal-(?!dark-)/.test(b));
const darkPaletteBlock = bareRootBlocks.find((b) => /--pal-dark-/.test(b));
// Only the bare semantic block declares `--hljs-keyword:` as an actual property (the palette blocks
// only declare `--pal-hljs-keyword:`), so this alone uniquely picks it out.
const bareSemanticBlock = bareRootBlocks.find((b) => /--hljs-keyword\s*:/.test(b));
const lightBlocks = allBlocks(css, /:root\[data-theme="light"\]\s*\{/);
const darkBlocks = allBlocks(css, /:root\[data-theme="dark"\]\s*\{/);
const hcLightBlocks = allBlocks(css, /:root\[data-theme="hc-light"\]\s*\{/);
const hcDarkBlocks = allBlocks(css, /:root\[data-theme="hc-dark"\]\s*\{/);

if (!lightPaletteBlock) throw new Error("no bare :root block declares --pal-hljs-* (light) primitives");
if (!darkPaletteBlock) throw new Error("no bare :root block declares --pal-dark-hljs-* primitives");
if (!bareSemanticBlock) throw new Error("no bare :root semantic block declares --hljs-keyword");
if (lightBlocks.length !== 1) throw new Error(`expected exactly one :root[data-theme="light"] block, found ${lightBlocks.length}`);
if (darkBlocks.length !== 1) throw new Error(`expected exactly one :root[data-theme="dark"] block, found ${darkBlocks.length}`);
if (hcLightBlocks.length !== 1) throw new Error(`expected exactly one :root[data-theme="hc-light"] block, found ${hcLightBlocks.length}`);
if (hcDarkBlocks.length !== 1) throw new Error(`expected exactly one :root[data-theme="hc-dark"] block, found ${hcDarkBlocks.length}`);

const lightPaletteDecls = extractDecls(lightPaletteBlock);
const darkPaletteDecls = extractDecls(darkPaletteBlock);
const hcLightPaletteDecls = extractPalette(css, "pal-hc-light-");
const hcDarkPaletteDecls = extractPalette(css, "pal-hc-dark-");

const bareSemanticDecls = extractDecls(bareSemanticBlock);
const lightSemanticDecls = extractDecls(lightBlocks[0]);
const darkSemanticDecls = extractDecls(darkBlocks[0]);
const hcLightSemanticDecls = extractDecls(hcLightBlocks[0]);
const hcDarkSemanticDecls = extractDecls(hcDarkBlocks[0]);

/** Resolve a semantic token's value to a concrete hex through the given palette + semantic maps
 *  (transitively — a token may reference another semantic token, or a plain palette var). */
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

// WCAG 2.1 relative luminance + contrast ratio math (inline, no dependency — duplicated from the
// sibling guard tests per their own precedent: keeps each guard file self-contained).
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

const HLJS_TOKENS = ["--hljs-keyword", "--hljs-title", "--hljs-string", "--hljs-comment", "--hljs-number", "--hljs-tag"];

describe("app.css hljs token completeness (CPE-1631)", () => {
  it("every --hljs-* token is defined in the bare :root fallback", () => {
    const missing = HLJS_TOKENS.filter((n) => !bareSemanticDecls.has(n));
    expect(missing, `missing from bare :root: ${missing.join(", ")}`).toEqual([]);
  });
  it("every --hljs-* token is defined in :root[data-theme=\"light\"]", () => {
    const missing = HLJS_TOKENS.filter((n) => !lightSemanticDecls.has(n));
    expect(missing, `missing from light: ${missing.join(", ")}`).toEqual([]);
  });
  it("every --hljs-* token is defined in :root[data-theme=\"dark\"]", () => {
    const missing = HLJS_TOKENS.filter((n) => !darkSemanticDecls.has(n));
    expect(missing, `missing from dark: ${missing.join(", ")}`).toEqual([]);
  });
  it("every --hljs-* token is defined in :root[data-theme=\"hc-light\"]", () => {
    const missing = HLJS_TOKENS.filter((n) => !hcLightSemanticDecls.has(n));
    expect(missing, `missing from hc-light: ${missing.join(", ")}`).toEqual([]);
  });
  it("every --hljs-* token is defined in :root[data-theme=\"hc-dark\"]", () => {
    const missing = HLJS_TOKENS.filter((n) => !hcDarkSemanticDecls.has(n));
    expect(missing, `missing from hc-dark: ${missing.join(", ")}`).toEqual([]);
  });
  it("the semantic layer never hard-codes a hex literal for an --hljs-* token (only var(--pal-*))", () => {
    for (const [label, decls] of [
      ["bare :root", bareSemanticDecls],
      ["light", lightSemanticDecls],
      ["dark", darkSemanticDecls],
      ["hc-light", hcLightSemanticDecls],
      ["hc-dark", hcDarkSemanticDecls],
    ] as const) {
      for (const name of HLJS_TOKENS) {
        const value = decls.get(name);
        expect(value, `${label} ${name} is not declared`).toBeDefined();
        expect(value, `${label} ${name} = ${value} — hard-coded hex, must be var(--pal-*)`).toMatch(
          /^var\(--[a-zA-Z0-9-]+\)$/,
        );
      }
    }
  });
});

describe("app.css hljs token WCAG contrast (CPE-1631)", () => {
  // Code is small monospace text — the stricter body-text bar (>=4.5:1), not the 3:1 non-text-UI bar
  // the --agent-* swatches use. Checked against BOTH --surface and --surface-alt since the plain code
  // preview (PreviewPane.svelte) and the notebook code cell (NotebookPreview.svelte) both ultimately
  // render on --surface (`.main { background: var(--surface) }`), and --surface-alt is close enough in
  // luminance in both palettes that checking both is cheap insurance against a future layout change.
  function checkAA(label: string, semanticDecls: Map<string, string>, paletteDecls: Map<string, string>) {
    it(`${label}: every --hljs-* token >= 4.5:1 on --surface and --surface-alt (WCAG AA normal text)`, () => {
      const surface = resolveHex(semanticDecls.get("--surface")!, paletteDecls, semanticDecls)!;
      const surfaceAlt = resolveHex(semanticDecls.get("--surface-alt")!, paletteDecls, semanticDecls)!;
      expect(surface, `${label} --surface did not resolve to a hex value`).toMatch(/^#[0-9a-fA-F]{6}$/);
      expect(surfaceAlt, `${label} --surface-alt did not resolve to a hex value`).toMatch(/^#[0-9a-fA-F]{6}$/);
      for (const name of HLJS_TOKENS) {
        const hex = resolveHex(semanticDecls.get(name)!, paletteDecls, semanticDecls);
        expect(hex, `${label} ${name} did not resolve to a hex value`).toMatch(/^#[0-9a-fA-F]{6}$/);
        const rSurface = contrastRatio(hex!, surface);
        const rSurfaceAlt = contrastRatio(hex!, surfaceAlt);
        expect(rSurface, `${label} ${name} (${hex}) vs --surface (${surface}) = ${rSurface.toFixed(2)}:1, want >=4.5:1`).toBeGreaterThanOrEqual(4.5);
        expect(rSurfaceAlt, `${label} ${name} (${hex}) vs --surface-alt (${surfaceAlt}) = ${rSurfaceAlt.toFixed(2)}:1, want >=4.5:1`).toBeGreaterThanOrEqual(4.5);
      }
    });
  }
  checkAA("light", lightSemanticDecls, lightPaletteDecls);
  checkAA("dark", darkSemanticDecls, darkPaletteDecls);

  // hc-light/hc-dark get the same stricter AAA-inspired >=7:1 bar src/app.css.hc-contrast.test.ts
  // applies to --text/--danger/--success there — a high-contrast theme gives every pairing the
  // treatment normal body text gets elsewhere, code included.
  function checkAAA(label: string, semanticDecls: Map<string, string>, paletteDecls: Map<string, string>) {
    it(`${label}: every --hljs-* token >= 7:1 on --surface (AAA-inspired, matches the hc palette's bar)`, () => {
      const surface = resolveHex(semanticDecls.get("--surface")!, paletteDecls, semanticDecls)!;
      expect(surface, `${label} --surface did not resolve to a hex value`).toMatch(/^#[0-9a-fA-F]{6}$/);
      for (const name of HLJS_TOKENS) {
        const hex = resolveHex(semanticDecls.get(name)!, paletteDecls, semanticDecls);
        expect(hex, `${label} ${name} did not resolve to a hex value`).toMatch(/^#[0-9a-fA-F]{6}$/);
        const ratio = contrastRatio(hex!, surface);
        expect(ratio, `${label} ${name} (${hex}) vs --surface (${surface}) = ${ratio.toFixed(2)}:1, want >=7:1`).toBeGreaterThanOrEqual(7);
      }
    });
  }
  checkAAA("hc-light", hcLightSemanticDecls, hcLightPaletteDecls);
  checkAAA("hc-dark", hcDarkSemanticDecls, hcDarkPaletteDecls);
});
