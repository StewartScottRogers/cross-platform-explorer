// CPE-1539 (epic CPE-1493, dark theme): guards the new `:root[data-theme="dark"]` block added to
// src/app.css. Two invariants:
//   (a) the dark semantic layer defines the SAME token set as `:root[data-theme="light"]` (CPE-1534)
//       — a token silently missing from the dark block would fall back to the light value, which
//       is a bug (e.g. a light-grey border on a near-black surface).
//   (b) every WCAG-relevant token pairing in the dark palette meets AA contrast: >=4.5:1 for body
//       text, >=3:1 for secondary text / non-text UI (borders, focus rings, icon-ish agent swatches)
//       per WCAG 2.1 SC 1.4.3 (text) and 1.4.11 (non-text UI contrast).
// This is a pure text+math test — no jsdom/browser needed — mirroring how CPE-1534's guard test
// (src/app.css.test.ts) regex-parses app.css. The dark theme is inert until CPE-1540 wires
// data-theme="dark" onto the document; this test only checks the authored values are correct, not
// that anything currently renders them.
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

const bareRootBlocks = allBlocks(css, /:root\s*\{/);
// The bare :root blocks, in source order: [0] light palette (--pal-*), [1] light/default semantic
// (--bg etc.), [2] dark palette (--pal-dark-*), [3] unrelated --filelist-cols layout block
// (out of scope, app.css:~432 — has neither --pal- nor --bg so it's excluded by these finders).
const lightPaletteBlock = bareRootBlocks.find((b) => /--pal-(?!dark-)/.test(b));
const darkPaletteBlock = bareRootBlocks.find((b) => /--pal-dark-/.test(b));
const lightBlocks = allBlocks(css, /:root\[data-theme="light"\]\s*\{/);
const darkBlocks = allBlocks(css, /:root\[data-theme="dark"\]\s*\{/);

if (!lightPaletteBlock) throw new Error("no bare :root block declares --pal-* (light) palette vars");
if (!darkPaletteBlock) throw new Error("no bare :root block declares --pal-dark-* palette vars");
if (lightBlocks.length !== 1) throw new Error(`expected exactly one :root[data-theme="light"] block, found ${lightBlocks.length}`);
if (darkBlocks.length !== 1) throw new Error(`expected exactly one :root[data-theme="dark"] block, found ${darkBlocks.length}`);

const darkPaletteDecls = extractDecls(darkPaletteBlock);
const lightSemanticDecls = extractDecls(lightBlocks[0]);
const darkSemanticDecls = extractDecls(darkBlocks[0]);

/** Resolve a semantic token's `var(--pal-dark-...)` value to a concrete hex through the dark
 *  palette map. Tokens that are plain values (e.g. `6px`, `30px`) or reference another semantic
 *  token (e.g. --agent-unknown -> var(--text-faint)) are resolved transitively. */
function resolveHex(value: string, depth = 0): string | undefined {
  if (depth > 5) return undefined;
  const hexMatch = value.match(/^#[0-9a-fA-F]{3,8}$/);
  if (hexMatch) return value;
  const varMatch = value.match(/^var\((--[a-zA-Z0-9-]+)\)$/);
  if (!varMatch) return undefined;
  const name = varMatch[1];
  if (darkPaletteDecls.has(name)) return resolveHex(darkPaletteDecls.get(name)!, depth + 1);
  if (darkSemanticDecls.has(name)) return resolveHex(darkSemanticDecls.get(name)!, depth + 1);
  return undefined;
}

// ---------------------------------------------------------------------------------------------
// WCAG 2.1 relative luminance + contrast ratio math (inline, no dependency —
// https://www.w3.org/TR/WCAG21/#dfn-relative-luminance / #dfn-contrast-ratio).
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

/** Every semantic token name that :root[data-theme="light"] resolves (CPE-1534's fixture) — the
 *  dark block must resolve the same set, with no drops. */
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

describe("app.css dark palette completeness (CPE-1539)", () => {
  it(':root[data-theme="dark"] resolves the same semantic token set as :root[data-theme="light"]', () => {
    const missing = SEMANTIC_TOKENS.filter((name) => !darkSemanticDecls.has(name));
    expect(missing, `tokens missing from :root[data-theme="dark"]: ${missing.join(", ")}`).toEqual([]);

    // Symmetric check: nothing in the light block is absent from SEMANTIC_TOKENS either (keeps the
    // fixture itself honest if a future ticket adds a new semantic token to light but not dark).
    const lightOnly = [...lightSemanticDecls.keys()].filter(
      (name) => !SEMANTIC_TOKENS.includes(name) && !darkSemanticDecls.has(name),
    );
    expect(lightOnly, `tokens present in light but missing from dark: ${lightOnly.join(", ")}`).toEqual([]);
  });

  it("sets color-scheme: dark (not light) on the dark block", () => {
    expect(stripComments(darkBlocks[0])).toMatch(/color-scheme\s*:\s*dark\s*;/);
  });

  it("every --pal-dark-* reference from the dark semantic layer resolves to a palette var that exists", () => {
    const dangling: string[] = [];
    for (const [name, value] of darkSemanticDecls) {
      const ref = value.match(/^var\((--pal-dark-[a-zA-Z0-9-]+)\)$/);
      if (ref && !darkPaletteDecls.has(ref[1])) dangling.push(`${name} -> ${ref[1]}`);
    }
    expect(dangling, `dark semantic tokens referencing a non-existent dark palette var: ${dangling.join(", ")}`).toEqual(
      [],
    );
  });
});

describe("app.css dark palette WCAG AA contrast (CPE-1539)", () => {
  const bg = resolveHex(darkSemanticDecls.get("--bg")!)!;
  const surface = resolveHex(darkSemanticDecls.get("--surface")!)!;
  const surfaceAlt = resolveHex(darkSemanticDecls.get("--surface-alt")!)!;
  const text = resolveHex(darkSemanticDecls.get("--text")!)!;
  const textDim = resolveHex(darkSemanticDecls.get("--text-dim")!)!;
  const textFaint = resolveHex(darkSemanticDecls.get("--text-faint")!)!;
  const accent = resolveHex(darkSemanticDecls.get("--accent")!)!;
  const borderStrong = resolveHex(darkSemanticDecls.get("--border-strong")!)!;
  const dialogBorder = resolveHex(darkSemanticDecls.get("--dialog-border")!)!;
  const danger = resolveHex(darkSemanticDecls.get("--danger")!)!;
  const success = resolveHex(darkSemanticDecls.get("--success")!)!;
  const textMuted = resolveHex(darkSemanticDecls.get("--text-muted")!)!;

  it("all key tokens resolved to a concrete hex through the palette layer", () => {
    for (const [name, hex] of [
      ["--bg", bg],
      ["--surface", surface],
      ["--surface-alt", surfaceAlt],
      ["--text", text],
      ["--text-dim", textDim],
      ["--text-faint", textFaint],
      ["--accent", accent],
      ["--border-strong", borderStrong],
      ["--dialog-border", dialogBorder],
      ["--danger", danger],
      ["--success", success],
      ["--text-muted", textMuted],
    ] as const) {
      expect(hex, `${name} did not resolve to a hex value`).toMatch(/^#[0-9a-fA-F]{6}$/);
    }
  });

  // Normal body text — WCAG AA 1.4.3 requires >=4.5:1.
  it("--text on --bg and --surface >= 4.5:1 (WCAG AA normal text)", () => {
    expect(contrastRatio(text, bg)).toBeGreaterThanOrEqual(4.5);
    expect(contrastRatio(text, surface)).toBeGreaterThanOrEqual(4.5);
  });

  it("--danger on --bg and --surface >= 4.5:1 (error text is body-text weight)", () => {
    expect(contrastRatio(danger, bg)).toBeGreaterThanOrEqual(4.5);
    expect(contrastRatio(danger, surface)).toBeGreaterThanOrEqual(4.5);
  });

  it("--success on --bg and --surface >= 4.5:1 (status text is body-text weight)", () => {
    expect(contrastRatio(success, bg)).toBeGreaterThanOrEqual(4.5);
    expect(contrastRatio(success, surface)).toBeGreaterThanOrEqual(4.5);
  });

  // Secondary/dimmed text (--text-dim) — treated as a >=3:1 pairing here: it's used for
  // supporting/large-scale text (subtitles, secondary labels), not primary body copy, matching the
  // WCAG AA "large text" allowance (SC 1.4.3). --text (primary body copy) is held to the stricter
  // 4.5:1 bar above.
  it("--text-dim on --bg and --surface >= 3:1 (secondary/large text)", () => {
    expect(contrastRatio(textDim, bg)).toBeGreaterThanOrEqual(3);
    expect(contrastRatio(textDim, surface)).toBeGreaterThanOrEqual(3);
  });

  // CPE-1632: --text-faint renders real small/normal-weight body text (the log viewer's TRACE
  // badge + gutter line numbers, "This file is empty."/"Loading…" notes) at sizes too small to
  // qualify as WCAG "large text", so — unlike --text-dim above — it gets the full 4.5:1 body-text
  // floor, checked against every surface it actually appears on. (Dark theme already clears this;
  // the light-theme mirror in src/app.css.light-contrast.test.ts is where CPE-1632 found the real
  // failure — 3.45:1/3.34:1, since fixed.)
  it("--text-faint on --bg, --surface, and --surface-alt >= 4.5:1 (WCAG AA normal text)", () => {
    expect(contrastRatio(textFaint, bg)).toBeGreaterThanOrEqual(4.5);
    expect(contrastRatio(textFaint, surface)).toBeGreaterThanOrEqual(4.5);
    expect(contrastRatio(textFaint, surfaceAlt)).toBeGreaterThanOrEqual(4.5);
  });

  // CPE-1821: --text-muted was referenced (20 call sites: AgentTimeline/ConsultedFiles/FileList)
  // but never defined anywhere, so its `#9a9a9a` fallback silently won in every theme. Resolved to
  // the same already-verified value as --text-faint above (same de-emphasised-small-text role);
  // asserted here directly, by name, so a future edit that repoints --text-muted at a different,
  // uncalibrated value fails immediately instead of relying on --text-faint's own assertion.
  it("--text-muted on --bg, --surface, and --surface-alt >= 4.5:1 (WCAG AA normal text)", () => {
    expect(contrastRatio(textMuted, bg)).toBeGreaterThanOrEqual(4.5);
    expect(contrastRatio(textMuted, surface)).toBeGreaterThanOrEqual(4.5);
    expect(contrastRatio(textMuted, surfaceAlt)).toBeGreaterThanOrEqual(4.5);
  });

  // Non-text UI component contrast — WCAG 2.1 SC 1.4.11 requires >=3:1 for a control's visual
  // boundary (borders that carry meaning) and for graphical objects essential to understanding
  // content (here: --accent used as focus rings/active-state fills/icon accents).
  it("--border-strong and --dialog-border on --bg and --surface >= 3:1 (WCAG 1.4.11 non-text UI)", () => {
    expect(contrastRatio(borderStrong, bg)).toBeGreaterThanOrEqual(3);
    expect(contrastRatio(borderStrong, surface)).toBeGreaterThanOrEqual(3);
    expect(contrastRatio(dialogBorder, bg)).toBeGreaterThanOrEqual(3);
    expect(contrastRatio(dialogBorder, surface)).toBeGreaterThanOrEqual(3);
  });

  it("--accent on --bg and --surface >= 3:1 (used as text/icon/focus-ring accent, WCAG 1.4.11)", () => {
    expect(contrastRatio(accent, bg)).toBeGreaterThanOrEqual(3);
    expect(contrastRatio(accent, surface)).toBeGreaterThanOrEqual(3);
  });

  it("the six agent colours + agent-user swatch >= 3:1 on --surface (colour-blind-safe legend/dots, WCAG 1.4.11)", () => {
    for (const name of ["--agent-1", "--agent-2", "--agent-3", "--agent-4", "--agent-5", "--agent-6", "--agent-user"]) {
      const hex = resolveHex(darkSemanticDecls.get(name)!);
      expect(hex, `${name} did not resolve to a hex value`).toMatch(/^#[0-9a-fA-F]{6}$/);
      const ratio = contrastRatio(hex!, surface);
      expect(ratio, `${name} (${hex}) vs --surface (${surface}) = ${ratio.toFixed(2)}:1, want >=3:1`).toBeGreaterThanOrEqual(
        3,
      );
    }
  });
});
