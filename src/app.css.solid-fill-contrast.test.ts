// CPE-1632: the guard's real deliverable. The dark-theme guard (src/app.css.dark-contrast.test.ts,
// CPE-1539) only ever checked "foreground text/icon token on --bg/--surface" pairings. It never
// checked the SOLID-FILL pattern — a token used as a `background`, paired with literal white
// (`color: #fff`) text — even though that's exactly how every `.btn.primary`-style button, the
// agent-activity/timeline badges, and several status pills render app-wide. That blind spot is
// what let white-on-solid-`--danger` ship at 2.88:1 (dark theme) — under WCAG's 3:1 UI-component
// floor — without any test noticing.
//
// Rather than hand-list the handful of components the ticket happened to name (exactly the kind of
// hand-maintained list that was already proven incomplete once), this file DERIVES the real
// on-screen pairings by parsing every component's actual CSS: for each rule with a
// `background`/`background-color` declaration, it resolves the literal foreground colour that
// paints on top of it — either declared directly on the same rule, or (the app's dominant pattern,
// e.g. `.btn.primary { color: #fff }` + `.btn.primary.danger { background: var(--danger) }`)
// inherited from a same-file rule whose class list is a subset of this rule's (so the element
// matches both selectors — real CSS cascade behaviour for non-conflicting properties). Any pairing
// where that resolved foreground is white/near-white is asserted against WCAG's 3:1 UI-component
// floor (buttons, pills, badges — not body paragraphs) in BOTH themes. New `.btn.primary`-alikes or
// solid badges are picked up automatically; nothing needs to be added to a list by hand.
import { describe, it, expect } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";

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
const lightPaletteBlock = bareRootBlocks.find((b) => /--pal-(?!dark-|hc-)/.test(b))!;
const darkPaletteBlock = bareRootBlocks.find((b) => /--pal-dark-/.test(b))!;
const lightBlocks = allBlocks(css, /:root\[data-theme="light"\]\s*\{/);
const darkBlocks = allBlocks(css, /:root\[data-theme="dark"\]\s*\{/);
if (lightBlocks.length !== 1) throw new Error(`expected exactly one :root[data-theme="light"] block, found ${lightBlocks.length}`);
if (darkBlocks.length !== 1) throw new Error(`expected exactly one :root[data-theme="dark"] block, found ${darkBlocks.length}`);

const lightPaletteDecls = extractDecls(lightPaletteBlock);
const darkPaletteDecls = extractDecls(darkPaletteBlock);
const lightSemanticDecls = extractDecls(lightBlocks[0]);
const darkSemanticDecls = extractDecls(darkBlocks[0]);

function resolveHex(
  value: string | undefined,
  paletteDecls: Map<string, string>,
  semanticDecls: Map<string, string>,
  depth = 0,
): string | undefined {
  if (!value || depth > 5) return undefined;
  const hexMatch = value.match(/^#[0-9a-fA-F]{3,8}$/);
  if (hexMatch) return value;
  const varMatch = value.match(/^var\(\s*(--[a-zA-Z0-9-]+)/);
  if (!varMatch) return undefined;
  const name = varMatch[1];
  if (paletteDecls.has(name)) return resolveHex(paletteDecls.get(name), paletteDecls, semanticDecls, depth + 1);
  if (semanticDecls.has(name)) return resolveHex(semanticDecls.get(name), paletteDecls, semanticDecls, depth + 1);
  return undefined;
}

/** Resolve a semantic/palette token name to a concrete hex in one theme; falls back to a literal
 *  fallback hex captured at the CSS usage site (`var(--undefined-token, #abc)`) when the token
 *  itself isn't defined anywhere in that theme (e.g. `--accent-2`, `--warn` — components that lean
 *  on a CSS custom-property fallback instead of a real theme token). Returns undefined only when
 *  neither resolves (e.g. a runtime-set custom property like TagEditor's `--sw`, which is
 *  genuinely dynamic per-tag data, not a static theme value this guard can check). */
function resolveTokenOrFallback(theme: "light" | "dark", token: string, fallbackHex: string | undefined): string | undefined {
  const [paletteDecls, semanticDecls] = theme === "light" ? [lightPaletteDecls, lightSemanticDecls] : [darkPaletteDecls, darkSemanticDecls];
  const direct = resolveHex(semanticDecls.get(token) ?? paletteDecls.get(token), paletteDecls, semanticDecls);
  if (direct) return direct;
  return resolveHex(fallbackHex, paletteDecls, semanticDecls);
}

// ---------------------------------------------------------------------------------------------
// WCAG 2.1 relative luminance + contrast ratio math (inline, no dependency), duplicated per this
// repo's single-file-per-guard precedent (see app.css.dark-contrast.test.ts).
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
const WHITE = "#ffffff";

// ---------------------------------------------------------------------------------------------
// Usage scanner: find every "solid-fill background paired with white foreground text" rule that
// actually exists in the app's real CSS (app.css + every .svelte component's <style> block).

/** Top-level `{selector, body}` pairs, brace-balanced; recurses into `@media`/`@supports` bodies
 *  (their "selector" is the at-rule prelude) so nested real rules are still found, and skips
 *  `@keyframes` percentage/from/to steps naturally (they never carry a `.class` selector so they
 *  can't match anything below). */
function parseTopLevelRules(cssText: string): { selector: string; body: string }[] {
  const rules: { selector: string; body: string }[] = [];
  let depth = 0;
  let selStart = 0;
  let bodyStart = -1;
  for (let i = 0; i < cssText.length; i++) {
    const ch = cssText[i];
    if (ch === "{") {
      if (depth === 0) bodyStart = i + 1;
      depth++;
    } else if (ch === "}") {
      depth--;
      if (depth === 0) {
        const selector = cssText.slice(selStart, bodyStart - 1).trim();
        const body = cssText.slice(bodyStart, i);
        if (selector.startsWith("@")) rules.push(...parseTopLevelRules(body));
        else if (selector) rules.push({ selector, body });
        selStart = i + 1;
      }
    }
  }
  return rules;
}

/** Split a selector list on top-level commas (ignoring commas inside `:not(...)` etc). */
function splitSelectorBranches(selector: string): string[] {
  const branches: string[] = [];
  let depth = 0;
  let start = 0;
  for (let i = 0; i < selector.length; i++) {
    const ch = selector[i];
    if (ch === "(") depth++;
    else if (ch === ")") depth--;
    else if (ch === "," && depth === 0) {
      branches.push(selector.slice(start, i).trim());
      start = i + 1;
    }
  }
  branches.push(selector.slice(start).trim());
  return branches.filter(Boolean);
}

/** The `.class` tokens a simple selector requires, after unwrapping `:global(...)` and stripping
 *  pseudo-classes/elements (`:hover`, `:not(...)`, `::before`, ...) — used only to test "does
 *  every class rule R' requires also appear on rule R" (a real-CSS-cascade subset check), not to
 *  fully parse selectors. */
function classesOf(branchSelector: string): string[] {
  let s = branchSelector.replace(/:global\(([^)]*)\)/g, "$1");
  s = s.replace(/:[a-zA-Z-]+(\([^)]*\))?/g, "");
  const out: string[] = [];
  const re = /\.[a-zA-Z_][\w-]*/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(s)) !== null) out.push(m[0].slice(1));
  return out;
}

const WHITE_RE = /^(#fff(fff)?|white|rgba?\(\s*255\s*,\s*255\s*,\s*255\b)/i;
const VAR_RE = /var\(\s*(--[a-zA-Z0-9-]+)\s*(?:,\s*([^)]+))?\)/;
const HEX_RE = /^#[0-9a-fA-F]{3,8}$/;

interface Pairing {
  token?: string;
  fallbackHex?: string;
  literalHex?: string;
  examples: Set<string>;
}
const tokenPairings = new Map<string, Pairing>(); // key: token name
const literalPairings = new Map<string, Pairing>(); // key: lowercased literal hex

function scanCss(label: string, cssText: string) {
  interface Record_ {
    classSet: string[];
    hasWhite: boolean;
    hasOwnNonWhiteColor: boolean;
    bgToken?: string;
    bgFallbackHex?: string;
    bgHex?: string;
    selector: string;
  }
  const records: Record_[] = [];
  for (const { selector, body } of parseTopLevelRules(stripComments(cssText))) {
    const bodyNorm = ";" + body;
    const bgMatch = bodyNorm.match(/;\s*background(?:-color)?\s*:\s*([^;]+);/);
    let bgToken: string | undefined;
    let bgFallbackHex: string | undefined;
    let bgHex: string | undefined;
    if (bgMatch) {
      const val = bgMatch[1].trim();
      const vm = val.match(VAR_RE);
      if (vm) {
        bgToken = vm[1];
        if (vm[2] && HEX_RE.test(vm[2].trim())) bgFallbackHex = vm[2].trim();
      } else if (HEX_RE.test(val)) {
        bgHex = val.toLowerCase();
      }
    }
    const colorMatch = bodyNorm.match(/;\s*color\s*:\s*([^;]+);/);
    const hasWhite = !!(colorMatch && WHITE_RE.test(colorMatch[1].trim()));
    // A rule that declares `color` to something OTHER than white explicitly OVERRIDES any base
    // rule's white text and must not fall back to it via the subset heuristic below (e.g.
    // `.swatch.none { color: var(--text-dim); }` overrides `.swatch { color: #fff; }`).
    const hasOwnNonWhiteColor = !!(colorMatch && !WHITE_RE.test(colorMatch[1].trim()));
    for (const branchSel of splitSelectorBranches(selector)) {
      records.push({ classSet: classesOf(branchSel), hasWhite, hasOwnNonWhiteColor, bgToken, bgFallbackHex, bgHex, selector: branchSel });
    }
  }
  for (const r of records) {
    if (!r.bgToken && !r.bgHex) continue;
    let matched = r.hasWhite;
    if (!matched && !r.hasOwnNonWhiteColor) {
      for (const other of records) {
        if (other === r || !other.hasWhite || other.classSet.length === 0) continue;
        if (other.classSet.every((c) => r.classSet.includes(c))) {
          matched = true;
          break;
        }
      }
    }
    if (!matched) continue;
    const example = `${label}: ${r.selector}`;
    if (r.bgToken) {
      const p = tokenPairings.get(r.bgToken) ?? { token: r.bgToken, fallbackHex: r.bgFallbackHex, examples: new Set<string>() };
      p.examples.add(example);
      tokenPairings.set(r.bgToken, p);
    } else if (r.bgHex) {
      const p = literalPairings.get(r.bgHex) ?? { literalHex: r.bgHex, examples: new Set<string>() };
      p.examples.add(example);
      literalPairings.set(r.bgHex, p);
    }
  }
}

function extractStyleBlocks(fileContent: string): string[] {
  return [...fileContent.matchAll(/<style[^>]*>([\s\S]*?)<\/style>/g)].map((m) => m[1]);
}

function walkSvelte(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walkSvelte(p, out);
    else if (name.endsWith(".svelte")) out.push(p);
  }
  return out;
}

// Scan app.css itself (e.g. `.pill.active`) plus every component's <style> block.
scanCss("app.css", css);
for (const f of walkSvelte(SRC)) {
  const content = readFileSync(f, "utf8");
  for (const block of extractStyleBlocks(content)) scanCss(relative(SRC, f).replace(/\\/g, "/"), block);
}

if (tokenPairings.size === 0) {
  throw new Error(
    "the solid-fill scanner found zero white-on-token background pairings — it should have found " +
      "at least --accent/--danger via `.btn.primary`/`.btn.primary.danger`; the scanner itself is " +
      "broken (selector/class-set extraction regressed), not the palette",
  );
}

// WCAG 2.1 SC 1.4.11 (non-text UI contrast) floor for buttons/badges/pills — these are UI
// components and small/bold labels, not body paragraphs, so 3:1 is the applicable bar (the same
// bar CPE-1632's own bug report applied to the white-on-danger case it found by eye).
const UI_FLOOR = 3;

describe("solid-fill white-on-token backgrounds clear WCAG's 3:1 UI-component floor (CPE-1632)", () => {
  for (const [token, pairing] of tokenPairings) {
    for (const theme of ["light", "dark"] as const) {
      const hex = resolveTokenOrFallback(theme, token, pairing.fallbackHex);
      if (!hex) {
        // Genuinely dynamic (e.g. TagEditor's per-tag `--sw` custom property) or a token this
        // guard doesn't know how to resolve — nothing to assert against statically.
        continue;
      }
      it(`white on var(${token}) [${theme}] (${hex}) >= ${UI_FLOOR}:1 — e.g. ${[...pairing.examples][0]}`, () => {
        const ratio = contrastRatio(WHITE, hex);
        expect(
          ratio,
          `white text on var(${token})=${hex} in ${theme} theme = ${ratio.toFixed(2)}:1, want >=${UI_FLOOR}:1. ` +
            `Real usages: ${[...pairing.examples].join(", ")}`,
        ).toBeGreaterThanOrEqual(UI_FLOOR);
      });
    }
  }

  for (const [hex, pairing] of literalPairings) {
    it(`white on hard-coded ${hex} >= ${UI_FLOOR}:1 (theme-invariant literal) — e.g. ${[...pairing.examples][0]}`, () => {
      const ratio = contrastRatio(WHITE, hex);
      expect(
        ratio,
        `white text on hard-coded ${hex} = ${ratio.toFixed(2)}:1, want >=${UI_FLOOR}:1. ` +
          `Real usages: ${[...pairing.examples].join(", ")}`,
      ).toBeGreaterThanOrEqual(UI_FLOOR);
    });
  }
});

describe("solid-fill scanner sanity (CPE-1632)", () => {
  it("found the known real solid-fill consumers (regression pin — if these disappear, the scanner broke, not the app)", () => {
    expect(tokenPairings.has("--danger"), "--danger should be found via .btn.primary.danger / .agent-badge.removed / .tl-badge.removed").toBe(true);
    expect(tokenPairings.has("--accent"), "--accent should be found via .btn.primary (app-wide)").toBe(true);
  });
});
