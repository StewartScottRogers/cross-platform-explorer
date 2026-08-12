// CPE-1649: the hc-light/hc-dark sibling of src/app.css.solid-fill-contrast.test.ts (CPE-1632).
// That guard was deliberately scoped to the normal light/dark palettes only (see its own header
// comment and CPE-1632's "Conflict surface" note) — it resolves `var(--token[, fallback])` chains
// through ONLY the light/dark palette + semantic layers, so it is blind to `:root[data-theme="hc-
// light"]`/`:root[data-theme="hc-dark"]` (CPE-1543, epic CPE-1496). That blind spot is exactly how
// hc-dark's white-on-solid-fill buttons/badges (`.btn.primary`/`.btn.primary.danger`,
// `.agent-badge.removed`/`.tl-badge.removed`/`.agent-chip.removed`, `.status.bad`, etc.) shipped
// failing WCAG's 3:1 UI-component floor WORSE than the normal dark theme's pre-CPE-1632 numbers —
// the theme built for accessibility was the least accessible one (CPE-1649).
//
// This file duplicates CPE-1632's usage-derived scanner rather than importing it, per this repo's
// established "single self-contained guard file, easy to delete" precedent (see
// src/app.css.hc-contrast.test.ts's own header comment for why it duplicates
// src/app.css.dark-contrast.test.ts's WCAG math rather than sharing it) — only the palette/theme
// resolution differs: hc-light/hc-dark primitives live in ONE shared bare `:root` block distinguished
// by a `--pal-hc-light-`/`--pal-hc-dark-` prefix (not their own block per theme, unlike light/dark),
// so resolution here uses the same prefix-scan `extractPalette` helper
// src/app.css.hc-contrast.test.ts already established, instead of the light/dark guard's
// block-finding approach.
//
// CPE-1649's negative control (recorded in the ticket's Work Log): before the fix, running this
// exact scanner against the ORIGINAL hc-dark values (`--pal-hc-dark-red-300: #ff8080`,
// `--pal-hc-dark-red-200: #ffb3b3`, `--pal-hc-dark-blue-300: #66b3ff`, `--pal-hc-dark-blue-200:
// #99ccff`, and `--danger`/`--danger-hover` still directly backing every solid-fill consumer instead
// of the new `--danger-fill` split) fails four assertions: white-on-danger 2.43:1, white-on-danger-
// hover 1.70:1, white-on-accent 2.22:1, white-on-accent-hover 1.69:1 — all under the 3:1 floor below.
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

/** Every `--pal-hc-light-*`/`--pal-hc-dark-*: #hex` primitive declaration anywhere in the file,
 *  regardless of which :root block declares it — mirrors src/app.css.hc-contrast.test.ts's own
 *  `extractPalette` (the hc primitives all share one bare :root block, unlike light/dark's
 *  one-block-per-theme layout). */
function extractPalette(source: string, prefix: string): Map<string, string> {
  const clean = stripComments(source);
  const decls = new Map<string, string>();
  const re = new RegExp(`(--${prefix}[a-zA-Z0-9-]+)\\s*:\\s*(#[0-9a-fA-F]{3,8})\\s*;`, "g");
  let m: RegExpExecArray | null;
  while ((m = re.exec(clean)) !== null) decls.set(m[1], m[2]);
  return decls;
}

const hcLightBlocks = allBlocks(css, /:root\[data-theme="hc-light"\]\s*\{/);
const hcDarkBlocks = allBlocks(css, /:root\[data-theme="hc-dark"\]\s*\{/);
if (hcLightBlocks.length !== 1) throw new Error(`expected exactly one :root[data-theme="hc-light"] block, found ${hcLightBlocks.length}`);
if (hcDarkBlocks.length !== 1) throw new Error(`expected exactly one :root[data-theme="hc-dark"] block, found ${hcDarkBlocks.length}`);

const hcLightPaletteDecls = extractPalette(css, "pal-hc-light-");
const hcDarkPaletteDecls = extractPalette(css, "pal-hc-dark-");
const hcLightSemanticDecls = extractDecls(hcLightBlocks[0]);
const hcDarkSemanticDecls = extractDecls(hcDarkBlocks[0]);
if (hcLightPaletteDecls.size === 0) throw new Error("no --pal-hc-light-* primitive declarations found");
if (hcDarkPaletteDecls.size === 0) throw new Error("no --pal-hc-dark-* primitive declarations found");

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
// Usage scanner — identical logic to src/app.css.solid-fill-contrast.test.ts (CPE-1632); see that
// file's header comment for the full rationale of deriving pairings from real CSS instead of a
// hand-maintained list. Only the resolution layer (below) differs: hc-light/hc-dark instead of
// light/dark.

/** Top-level `{selector, body}` pairs, brace-balanced; recurses into `@media`/`@supports` bodies. */
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
 *  pseudo-classes/elements — used only to test the real-CSS-cascade subset check below. */
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
const HEX_RE = /^#[0-9a-fA-F]{3,8}$/;

type HcTheme = "hc-light" | "hc-dark";

/** Parse a `var(--token[, fallback])` call, balancing nested parens in the fallback — see
 *  src/app.css.solid-fill-contrast.test.ts's identical helper for the CPE-1632 review-round-2 bug
 *  this shape fixes (a naive regex truncates a nested fallback at its first `)`). */
function parseVarCall(text: string): { token: string; fallback?: string } | null {
  const head = text.match(/^var\(\s*(--[a-zA-Z0-9-]+)\s*/i);
  if (!head) return null;
  let i = head[0].length;
  if (text[i] === ")") return { token: head[1] };
  if (text[i] !== ",") return null;
  i++;
  while (text[i] === " ") i++;
  const start = i;
  let depth = 0;
  for (; i < text.length; i++) {
    if (text[i] === "(") depth++;
    else if (text[i] === ")") {
      if (depth === 0) break;
      depth--;
    }
  }
  const fallback = text.slice(start, i).trim();
  return { token: head[1], fallback: fallback || undefined };
}

/** Resolve ANY CSS colour value to a concrete hex in one hc theme — the hc-theme analogue of
 *  src/app.css.solid-fill-contrast.test.ts's `resolveCssValue`. */
function resolveCssValue(rawValue: string | undefined, theme: HcTheme, depth = 0): string | undefined {
  if (!rawValue || depth > 8) return undefined;
  const value = rawValue.trim();
  if (HEX_RE.test(value)) return value;
  if (WHITE_RE.test(value)) return "#ffffff";
  if (/^var\(/i.test(value)) {
    const parsed = parseVarCall(value);
    if (!parsed) return undefined;
    const [paletteDecls, semanticDecls] =
      theme === "hc-light" ? [hcLightPaletteDecls, hcLightSemanticDecls] : [hcDarkPaletteDecls, hcDarkSemanticDecls];
    const declared = semanticDecls.get(parsed.token) ?? paletteDecls.get(parsed.token);
    if (declared !== undefined) {
      const resolved = resolveCssValue(declared, theme, depth + 1);
      if (resolved) return resolved;
    }
    return resolveCssValue(parsed.fallback, theme, depth + 1);
  }
  return undefined;
}

function resolveTokenOrFallback(theme: HcTheme, token: string, fallbackRaw: string | undefined): string | undefined {
  return resolveCssValue(fallbackRaw !== undefined ? `var(${token}, ${fallbackRaw})` : `var(${token})`, theme);
}

function isWhiteHex(hex: string): boolean {
  const h = hex.replace("#", "");
  const full = h.length === 3 ? h.split("").map((c) => c + c).join("") : h;
  return full.slice(0, 6).toLowerCase() === "ffffff";
}

/** The general body-text roles — excluded from "white foreground" classification below. This is
 *  NOT a scope hack: CPE-1632's own guard explicitly targets buttons/pills/badges, "not body
 *  paragraphs" — --text/--text-dim/--text-faint ARE the body-paragraph tokens. In hc-dark
 *  specifically (unlike anywhere in light/dark, where CPE-1632's guard runs unmodified), --text
 *  legitimately resolves to a LITERAL #ffffff (--pal-hc-dark-white) — that's the actual definition
 *  of "high contrast dark": near-white body text everywhere. Without this exclusion, that single
 *  fact makes the scanner misclassify ~200 completely ordinary `.foo:hover { background: var(--x);
 *  color: var(--text) }`-shaped rules across the app as if they were deliberate solid-fill buttons
 *  (CPE-1649 discovery: e.g. `.tab:hover { background: var(--hover); color: var(--text); }`,
 *  `FileList.svelte`'s `.rename { background: #fff; color: var(--text); }`), drowning the real
 *  danger/accent regression signal in noise unrelated to this ticket's scope. --accent-fg and a
 *  literal `#fff`/`white` stay in scope — those ARE the deliberate solid-fill-foreground signal. */
const BODY_TEXT_TOKENS = new Set(["--text", "--text-dim", "--text-faint"]);

/** Does this `color:` declaration's value render literal or near-white text in AT LEAST ONE hc
 *  theme — mirrors src/app.css.solid-fill-contrast.test.ts's `isWhiteishForeground`, plus the
 *  body-text exclusion above (light/dark never needed it — see that constant's comment). */
function isWhiteishForeground(rawValue: string): boolean {
  const trimmed = rawValue.trim();
  if (/^var\(/i.test(trimmed)) {
    const parsed = parseVarCall(trimmed);
    if (parsed && BODY_TEXT_TOKENS.has(parsed.token)) return false;
  }
  for (const theme of ["hc-light", "hc-dark"] as const) {
    const hex = resolveCssValue(rawValue, theme);
    if (hex && isWhiteHex(hex)) return true;
  }
  return false;
}

interface Pairing {
  token?: string;
  fallbackRaw?: string;
  literalHex?: string;
  examples: Set<string>;
}
const tokenPairings = new Map<string, Pairing>();
const literalPairings = new Map<string, Pairing>();

function scanCss(label: string, cssText: string) {
  interface Record_ {
    classSet: string[];
    hasWhite: boolean;
    hasOwnNonWhiteColor: boolean;
    bgToken?: string;
    bgFallbackRaw?: string;
    bgHex?: string;
    selector: string;
  }
  const records: Record_[] = [];
  for (const { selector, body } of parseTopLevelRules(stripComments(cssText))) {
    const bodyNorm = ";" + body;
    const bgMatch = bodyNorm.match(/;\s*background(?:-color)?\s*:\s*([^;]+);/);
    let bgToken: string | undefined;
    let bgFallbackRaw: string | undefined;
    let bgHex: string | undefined;
    if (bgMatch) {
      const val = bgMatch[1].trim();
      if (/^var\(/i.test(val)) {
        const parsed = parseVarCall(val);
        if (parsed) {
          bgToken = parsed.token;
          if (parsed.fallback && HEX_RE.test(parsed.fallback)) bgFallbackRaw = parsed.fallback;
        }
      } else if (HEX_RE.test(val)) {
        bgHex = val.toLowerCase();
      }
      // See src/app.css.solid-fill-contrast.test.ts's identical comment: non-`var()`/non-hex
      // backgrounds (rgba/hsl/color-mix/gradients) are a disclosed, audited-safe scanner gap, not
      // asserted here either.
    }
    const colorMatch = bodyNorm.match(/;\s*color\s*:\s*([^;]+);/);
    const hasWhite = !!(colorMatch && isWhiteishForeground(colorMatch[1].trim()));
    const hasOwnNonWhiteColor = !!colorMatch && !hasWhite;
    for (const branchSel of splitSelectorBranches(selector)) {
      records.push({ classSet: classesOf(branchSel), hasWhite, hasOwnNonWhiteColor, bgToken, bgFallbackRaw, bgHex, selector: branchSel });
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
      const p = tokenPairings.get(r.bgToken) ?? { token: r.bgToken, fallbackRaw: r.bgFallbackRaw, examples: new Set<string>() };
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

scanCss("app.css", css);
for (const f of walkSvelte(SRC)) {
  const content = readFileSync(f, "utf8");
  for (const block of extractStyleBlocks(content)) scanCss(relative(SRC, f).replace(/\\/g, "/"), block);
}

if (tokenPairings.size === 0) {
  throw new Error(
    "the hc solid-fill scanner found zero white-on-token background pairings — it should have found " +
      "at least --accent/--danger-fill via `.btn.primary`/`.btn.primary.danger`; the scanner itself is " +
      "broken, not the palette",
  );
}

// WCAG 2.1 SC 1.4.11 (non-text UI contrast) floor for buttons/badges/pills — same 3:1 bar the
// normal light/dark solid-fill guard uses (CPE-1632). The hc themes' STRICTER 7:1/4.5:1 bars
// (src/app.css.hc-contrast.test.ts) apply to the *foreground-text* role, not this solid-fill-
// background role — see the CPE-1649 comment on --pal-hc-dark-red-300/--pal-hc-dark-red-fill in
// app.css for why those two roles were split into separate tokens for hc-dark specifically.
const UI_FLOOR = 3;

describe("hc solid-fill white-on-token backgrounds clear WCAG's 3:1 UI-component floor (CPE-1649)", () => {
  for (const [token, pairing] of tokenPairings) {
    for (const theme of ["hc-light", "hc-dark"] as const) {
      const hex = resolveTokenOrFallback(theme, token, pairing.fallbackRaw);
      if (!hex) continue;
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

describe("hc solid-fill scanner sanity (CPE-1649)", () => {
  it("found the known real solid-fill consumers (regression pin — if these disappear, the scanner broke, not the app)", () => {
    expect(
      tokenPairings.has("--danger-fill"),
      "--danger-fill should be found via .btn.primary.danger / .agent-badge.removed / .tl-badge.removed " +
        "/ .agent-chip.removed (CPE-1649 split --danger's solid-fill role into this token for hc-dark)",
    ).toBe(true);
    expect(tokenPairings.has("--accent"), "--accent should be found via .btn.primary (app-wide)").toBe(true);
    expect(tokenPairings.has("--danger-hover"), "--danger-hover should be found via .btn.primary.danger:hover").toBe(true);
    expect(tokenPairings.has("--accent-hover"), "--accent-hover should be found via .btn.primary:hover (app-wide)").toBe(true);
  });
});
