// CPE-1618 Visual Critic follow-up (PR 829): guards the active filter-chip LABEL text contrast in
// LogPreview.svelte. The chip's active state tints its background via
// `color-mix(in srgb, <level-color> <pct>%, var(--surface))`; the critic measured the label text (which
// used to render in that same level colour) at 3.92-4.49:1 across the six theme/level combinations —
// four of six under WCAG AA's 4.5:1 floor for normal text. Fix: the label renders in `var(--text)` and
// the border + tinted fill carry the level identity instead (component-scoped change, no new/changed
// global token per CLAUDE.md's per-component-not-shared-token guidance).
//
// This is a pure text+math test (no jsdom/browser) mirroring app.css.dark-contrast.test.ts's approach:
// it regex-parses BOTH the real app.css token values AND LogPreview.svelte's actual <style> block, so it
// computes against the real `color-mix` background and the real declared label colour — not hardcoded
// numbers. Reverting the component's `color:` declarations back to the level colour (today's pre-fix
// value) makes this test fail — see the Work Log for the observed negative-control run.
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const SRC = join(process.cwd(), "src");
const css = readFileSync(join(SRC, "app.css"), "utf8");
const componentSrc = readFileSync(join(SRC, "lib", "components", "LogPreview.svelte"), "utf8");

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
const lightPaletteBlock = bareRootBlocks.find((b) => /--pal-(?!dark-)/.test(b));
const darkPaletteBlock = bareRootBlocks.find((b) => /--pal-dark-/.test(b));
const lightSemanticBlock = allBlocks(css, /:root\[data-theme="light"\]\s*\{/)[0];
const darkSemanticBlock = allBlocks(css, /:root\[data-theme="dark"\]\s*\{/)[0];

if (!lightPaletteBlock) throw new Error("no bare :root block declares --pal-* (light) palette vars");
if (!darkPaletteBlock) throw new Error("no bare :root block declares --pal-dark-* palette vars");
if (!lightSemanticBlock) throw new Error('no :root[data-theme="light"] block found');
if (!darkSemanticBlock) throw new Error('no :root[data-theme="dark"] block found');

const lightPaletteDecls = extractDecls(lightPaletteBlock);
const darkPaletteDecls = extractDecls(darkPaletteBlock);
const lightSemanticDecls = extractDecls(lightSemanticBlock);
const darkSemanticDecls = extractDecls(darkSemanticBlock);

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

function resolveToken(name: string, theme: "light" | "dark"): string {
  const [paletteDecls, semanticDecls] = theme === "light" ? [lightPaletteDecls, lightSemanticDecls] : [darkPaletteDecls, darkSemanticDecls];
  const raw = semanticDecls.get(name);
  if (!raw) throw new Error(`token ${name} not declared in :root[data-theme="${theme}"]`);
  const hex = resolveHex(raw, paletteDecls, semanticDecls);
  if (!hex) throw new Error(`token ${name} (${raw}) did not resolve to a hex value in theme ${theme}`);
  return hex;
}

// ---------------------------------------------------------------------------------------------
// WCAG 2.1 relative luminance + contrast ratio math (inline, no dependency).
function hexToRgb(hex: string): [number, number, number] {
  let h = hex.replace("#", "");
  if (h.length === 3) h = h.split("").map((c) => c + c).join("");
  return [parseInt(h.substring(0, 2), 16), parseInt(h.substring(2, 4), 16), parseInt(h.substring(4, 6), 16)];
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
/** `color-mix(in srgb, A pct%, B)` — CSS mixes raw (gamma-encoded) sRGB channel bytes, not linear light. */
function colorMix(hexA: string, pct: number, hexB: string): string {
  const [ra, ga, ba] = hexToRgb(hexA);
  const [rb, gb, bb] = hexToRgb(hexB);
  const mix = (a: number, b: number) => Math.round(a * (pct / 100) + b * (1 - pct / 100));
  const toHex = (n: number) => n.toString(16).padStart(2, "0");
  return `#${toHex(mix(ra, rb))}${toHex(mix(ga, gb))}${toHex(mix(ba, bb))}`;
}

// ---------------------------------------------------------------------------------------------
// Parse the component's actual <style> block so this test reflects the real declared rules, not a
// hand-copied snapshot of them.
const styleMatch = componentSrc.match(/<style>([\s\S]*)<\/style>/);
if (!styleMatch) throw new Error("LogPreview.svelte has no <style> block");
const componentCss = stripComments(styleMatch[1]);

function findRule(selectorRe: RegExp): string {
  const m = componentCss.match(selectorRe);
  if (!m) throw new Error(`rule not found for ${selectorRe}`);
  const open = componentCss.indexOf("{", m.index!);
  const close = componentCss.indexOf("}", open);
  return componentCss.slice(open + 1, close);
}

/** Extract the mix-source token, the mix percentage, and the label `color:` token from one
 *  `.log-chip...active { ... }` rule body. */
function parseChipRule(rule: string): { mixVar: string; pct: number; labelVar: string } {
  const bgMatch = rule.match(/background:\s*color-mix\(in srgb,\s*var\((--[a-zA-Z0-9-]+)\)\s*(\d+)%,\s*var\(--surface\)\)/);
  if (!bgMatch) throw new Error(`could not parse color-mix background from rule: ${rule}`);
  const colorMatch = rule.match(/color:\s*var\((--[a-zA-Z0-9-]+)\)/);
  if (!colorMatch) throw new Error(`could not parse label color from rule: ${rule}`);
  return { mixVar: bgMatch[1], pct: Number(bgMatch[2]), labelVar: colorMatch[1] };
}

const genericRule = parseChipRule(findRule(/\.log-chip\.active\s*\{/));
const errorRule = parseChipRule(findRule(/\.log-chip\[data-level="error"\]\.active\s*\{/));
const warnRule = parseChipRule(findRule(/\.log-chip\[data-level="warn"\]\.active\s*\{/));

type Combo = { name: string; rule: { mixVar: string; pct: number; labelVar: string } };
const COMBOS: Combo[] = [
  { name: "generic (Info/Debug/Trace/Other)", rule: genericRule },
  { name: "error", rule: errorRule },
  { name: "warn", rule: warnRule },
];

describe("LogPreview active filter-chip label contrast (CPE-1618, Visual Critic PR 829)", () => {
  for (const theme of ["light", "dark"] as const) {
    for (const combo of COMBOS) {
      it(`${theme} theme, ${combo.name}: label >= 4.5:1 against the real color-mix background`, () => {
        const surface = resolveToken("--surface", theme);
        const mixColor = resolveToken(combo.rule.mixVar, theme);
        const label = resolveToken(combo.rule.labelVar, theme);
        const bg = colorMix(mixColor, combo.rule.pct, surface);
        const ratio = contrastRatio(label, bg);
        expect(
          ratio,
          `${theme}/${combo.name}: label ${combo.rule.labelVar}=${label} on color-mix bg ${bg} (from ${combo.rule.mixVar} ${combo.rule.pct}% + --surface ${surface}) = ${ratio.toFixed(2)}:1, want >=4.5:1`,
        ).toBeGreaterThanOrEqual(4.5);
      });
    }
  }
});
