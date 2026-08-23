// CPE-1632: guards `:root[data-theme="light"]` (CPE-1534) the same way
// src/app.css.dark-contrast.test.ts (CPE-1539) guards the dark block — a mathematical WCAG AA
// contrast check, not just structural completeness. Before this ticket, NOTHING asserted the
// light theme's foundational token contrast by the numbers: src/app.css.test.ts (CPE-1534) only
// checks that the light block's tokens exist and match the bare-:root fallback, never that they
// actually clear WCAG. That gap is exactly how CPE-1632's --text-faint failure (3.45:1 on
// --surface, 3.34:1 on --surface-alt — both under AA's 4.5:1 normal-text floor) went undetected:
// nobody had ever run the numbers. This file is the light-theme mirror of the dark guard's math
// (AA: >=4.5:1 body text, >=3:1 secondary text/non-text UI per WCAG 2.1 SC 1.4.3/1.4.11), plus the
// --text-faint assertion neither theme's guard carried before this ticket.
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
// [0] light palette (--pal-*, excluding --pal-dark-/--pal-hc-), [1] light/default semantic (--bg
// etc.) — mirrors the indexing comment in app.css.dark-contrast.test.ts.
const lightPaletteBlock = bareRootBlocks.find((b) => /--pal-(?!dark-|hc-)/.test(b));
const lightBlocks = allBlocks(css, /:root\[data-theme="light"\]\s*\{/);

if (!lightPaletteBlock) throw new Error("no bare :root block declares --pal-* (light) palette vars");
if (lightBlocks.length !== 1) throw new Error(`expected exactly one :root[data-theme="light"] block, found ${lightBlocks.length}`);

const lightPaletteDecls = extractDecls(lightPaletteBlock);
const lightSemanticDecls = extractDecls(lightBlocks[0]);

/** Resolve a semantic token's `var(--pal-...)` value to a concrete hex through the light palette
 *  map. Tokens that reference another semantic token are resolved transitively. */
function resolveHex(value: string, depth = 0): string | undefined {
  if (depth > 5) return undefined;
  const hexMatch = value.match(/^#[0-9a-fA-F]{3,8}$/);
  if (hexMatch) return value;
  const varMatch = value.match(/^var\((--[a-zA-Z0-9-]+)\)$/);
  if (!varMatch) return undefined;
  const name = varMatch[1];
  if (lightPaletteDecls.has(name)) return resolveHex(lightPaletteDecls.get(name)!, depth + 1);
  if (lightSemanticDecls.has(name)) return resolveHex(lightSemanticDecls.get(name)!, depth + 1);
  return undefined;
}

// ---------------------------------------------------------------------------------------------
// WCAG 2.1 relative luminance + contrast ratio math (inline, no dependency), duplicated from
// src/app.css.dark-contrast.test.ts per that file's own single-file-per-guard precedent.
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

describe("app.css light palette WCAG AA contrast (CPE-1632)", () => {
  const bg = resolveHex(lightSemanticDecls.get("--bg")!)!;
  const surface = resolveHex(lightSemanticDecls.get("--surface")!)!;
  const surfaceAlt = resolveHex(lightSemanticDecls.get("--surface-alt")!)!;
  const text = resolveHex(lightSemanticDecls.get("--text")!)!;
  const textDim = resolveHex(lightSemanticDecls.get("--text-dim")!)!;
  const textFaint = resolveHex(lightSemanticDecls.get("--text-faint")!)!;
  const accent = resolveHex(lightSemanticDecls.get("--accent")!)!;
  const borderStrong = resolveHex(lightSemanticDecls.get("--border-strong")!)!;
  const dialogBorder = resolveHex(lightSemanticDecls.get("--dialog-border")!)!;
  const danger = resolveHex(lightSemanticDecls.get("--danger")!)!;
  const success = resolveHex(lightSemanticDecls.get("--success")!)!;
  const textMuted = resolveHex(lightSemanticDecls.get("--text-muted")!)!;

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

  // --success is never used as text (`color: var(--success)` has zero real hits app-wide) — its
  // only real consumer is Sidebar's `.state-dot.state-connected` background dot, a graphical
  // object under WCAG 1.4.11's 3:1 non-text floor, not the 4.5:1 body-text bar. (The dark guard
  // holds this to 4.5:1 too, which happens to still pass there — left as-is rather than loosened,
  // since correctness only requires a floor, not an exact match; light's real value clears 3:1 but
  // not 4.5:1, so it needs the accurate bar to pass honestly.)
  it("--success on --bg and --surface >= 3:1 (non-text UI — a status dot, not text)", () => {
    expect(contrastRatio(success, bg)).toBeGreaterThanOrEqual(3);
    expect(contrastRatio(success, surface)).toBeGreaterThanOrEqual(3);
  });

  // Secondary/dimmed text (--text-dim) — >=3:1 pairing, mirroring the dark guard's "large text"
  // allowance (WCAG AA SC 1.4.3): supporting/large-scale labels, not primary body copy.
  it("--text-dim on --bg and --surface >= 3:1 (secondary/large text)", () => {
    expect(contrastRatio(textDim, bg)).toBeGreaterThanOrEqual(3);
    expect(contrastRatio(textDim, surface)).toBeGreaterThanOrEqual(3);
  });

  // CPE-1632's second finding: --text-faint renders real small/normal-weight body text (the log
  // viewer's TRACE badge + gutter line numbers, "This file is empty."/"Loading…" notes) at sizes
  // too small to qualify as WCAG "large text", so it gets the full 4.5:1 body-text floor — checked
  // against every surface it actually appears on (--bg, --surface, --surface-alt).
  it("--text-faint on --bg, --surface, and --surface-alt >= 4.5:1 (WCAG AA normal text)", () => {
    expect(contrastRatio(textFaint, bg)).toBeGreaterThanOrEqual(4.5);
    expect(contrastRatio(textFaint, surface)).toBeGreaterThanOrEqual(4.5);
    expect(contrastRatio(textFaint, surfaceAlt)).toBeGreaterThanOrEqual(4.5);
  });

  // CPE-1821: --text-muted was referenced (20 call sites: AgentTimeline/ConsultedFiles/FileList —
  // small uppercase labels, badge borders/fills, table headers) but never defined anywhere, so its
  // `#9a9a9a` fallback silently won in every theme, including light — where it measures only
  // 2.81:1 against white, well under this bar. Resolved to the same already-verified value as
  // --text-faint above (same de-emphasised-small-text role); asserted here directly, by name, so a
  // future edit that repoints --text-muted at a different, uncalibrated value fails immediately
  // instead of relying on --text-faint's own assertion as an indirect proxy.
  it("--text-muted on --bg, --surface, and --surface-alt >= 4.5:1 (WCAG AA normal text)", () => {
    expect(contrastRatio(textMuted, bg)).toBeGreaterThanOrEqual(4.5);
    expect(contrastRatio(textMuted, surface)).toBeGreaterThanOrEqual(4.5);
    expect(contrastRatio(textMuted, surfaceAlt)).toBeGreaterThanOrEqual(4.5);
  });

  // Non-text UI component contrast — WCAG 2.1 SC 1.4.11 requires >=3:1 for a control's visual
  // boundary and for graphical objects essential to understanding content. --border-strong is the
  // toolbar/menu/input-chrome emphasis border (address bar, search box, docs button, menus,
  // command-bar separator, per app.css' `--border-strong` doc comment) — every real usage sits on
  // --surface or --surface-alt, never directly on --bg, so those (plus --bg for margin) are what's
  // checked.
  it("--border-strong on --bg, --surface, and --surface-alt >= 3:1; --dialog-border on --bg and --surface >= 3:1 (WCAG 1.4.11 non-text UI)", () => {
    expect(contrastRatio(borderStrong, bg)).toBeGreaterThanOrEqual(3);
    expect(contrastRatio(borderStrong, surface)).toBeGreaterThanOrEqual(3);
    expect(contrastRatio(borderStrong, surfaceAlt)).toBeGreaterThanOrEqual(3);
    expect(contrastRatio(dialogBorder, bg)).toBeGreaterThanOrEqual(3);
    expect(contrastRatio(dialogBorder, surface)).toBeGreaterThanOrEqual(3);
  });

  it("--accent on --bg and --surface >= 3:1 (used as text/icon/focus-ring accent, WCAG 1.4.11)", () => {
    expect(contrastRatio(accent, bg)).toBeGreaterThanOrEqual(3);
    expect(contrastRatio(accent, surface)).toBeGreaterThanOrEqual(3);
  });

  it("the six agent colours + agent-user swatch >= 3:1 on --surface (colour-blind-safe legend/dots, WCAG 1.4.11)", () => {
    for (const name of ["--agent-1", "--agent-2", "--agent-3", "--agent-4", "--agent-5", "--agent-6", "--agent-user"]) {
      const hex = resolveHex(lightSemanticDecls.get(name)!);
      expect(hex, `${name} did not resolve to a hex value`).toMatch(/^#[0-9a-fA-F]{6}$/);
      const ratio = contrastRatio(hex!, surface);
      expect(ratio, `${name} (${hex}) vs --surface (${surface}) = ${ratio.toFixed(2)}:1, want >=3:1`).toBeGreaterThanOrEqual(
        3,
      );
    }
  });
});
