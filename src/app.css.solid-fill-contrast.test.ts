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

// Token/fallback resolution (both the `resolveCssValue`/`resolveTokenOrFallback` pair used by the
// scanner below) lives just after the WHITE_RE/HEX_RE constants a little further down, so it can
// share those patterns instead of duplicating them.

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
const HEX_RE = /^#[0-9a-fA-F]{3,8}$/;

/** Parse a `var(--token[, fallback])` call, returning the token and the RAW fallback text
 *  (unparsed — it may itself be another `var(...)`, a hex, or a keyword like `white`).
 *
 *  CPE-1632 review round 2: a plain regex like `/var\(\s*(--[\w-]+)\s*(?:,\s*([^)]+))?\)/` (this
 *  file's previous `VAR_RE`) truncates a NESTED fallback at the first `)` it sees — for
 *  `var(--border-strong, var(--border, #3a3a3a))` it would capture fallback text
 *  `"var(--border, #3a3a3a"` (missing its closing paren), which then fails every "is this a hex"
 *  check downstream and gets silently dropped. This balances parens instead, so a nested fallback
 *  round-trips intact through to `resolveCssValue` below, which can then walk it recursively.
 *  `text` must already be trimmed and start with "var(". Returns null if malformed. */
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

/** Resolve ANY CSS colour value — a literal hex, the `white` keyword / an opaque-white
 *  `rgb(a)(255,255,255...)` literal, or a (possibly nested) `var(--token[, fallback])` chain — to
 *  a concrete hex in one theme. This is the single implementation both the background side
 *  (`background`/`background-color`) and the foreground side (`color`) use below, so a token
 *  resolves identically regardless of which property it's declared on.
 *
 *  CPE-1632 review round 2 — the bug this fixes: the guard's earlier foreground detection
 *  (`WHITE_RE` alone, matched against the raw `color:` text) only recognised a LITERAL white —
 *  `var(--accent-fg, #fff)` never matched, so a background this dark ALWAYS renders literal white
 *  text (the token doesn't exist anywhere in app.css; the fallback is unconditional) sailed
 *  through with zero assertion generated. This resolves the token first (walking palette then
 *  semantic decls, matching `--danger`/`--accent`'s own reference order) and, only when the token
 *  itself isn't defined in this theme, falls back to resolving the fallback text — which may
 *  itself be another `var(...)`, so this recurses rather than requiring a bare hex. */
function resolveCssValue(rawValue: string | undefined, theme: "light" | "dark", depth = 0): string | undefined {
  if (!rawValue || depth > 8) return undefined;
  const value = rawValue.trim();
  if (HEX_RE.test(value)) return value;
  if (WHITE_RE.test(value)) return "#ffffff";
  if (/^var\(/i.test(value)) {
    const parsed = parseVarCall(value);
    if (!parsed) return undefined;
    const [paletteDecls, semanticDecls] = theme === "light" ? [lightPaletteDecls, lightSemanticDecls] : [darkPaletteDecls, darkSemanticDecls];
    const declared = semanticDecls.get(parsed.token) ?? paletteDecls.get(parsed.token);
    if (declared !== undefined) {
      const resolved = resolveCssValue(declared, theme, depth + 1);
      if (resolved) return resolved;
    }
    return resolveCssValue(parsed.fallback, theme, depth + 1);
  }
  return undefined;
}

/** Resolve a semantic/palette token name to a concrete hex in one theme; falls back to a literal
 *  fallback (captured at the CSS usage site, `var(--undefined-token, <fallback>)`) when the token
 *  itself isn't defined anywhere in that theme (e.g. `--warn` — components that lean on a CSS
 *  custom-property fallback instead of a real theme token). Returns undefined only when neither
 *  resolves (e.g. a runtime-set custom property like TagEditor's `--sw`, which is genuinely
 *  dynamic per-tag data, not a static theme value this guard can check). Thin wrapper over
 *  `resolveCssValue` so background-token resolution and foreground-token resolution
 *  (`isWhiteishForeground` below) share one code path. */
function resolveTokenOrFallback(theme: "light" | "dark", token: string, fallbackRaw: string | undefined): string | undefined {
  return resolveCssValue(fallbackRaw !== undefined ? `var(${token}, ${fallbackRaw})` : `var(${token})`, theme);
}

function isWhiteHex(hex: string): boolean {
  const h = hex.replace("#", "");
  const full = h.length === 3 ? h.split("").map((c) => c + c).join("") : h;
  return full.slice(0, 6).toLowerCase() === "ffffff";
}

/** Does this `color:` declaration's value render literal or near-white text in AT LEAST ONE theme
 *  — resolving `var(--token[, fallback])` (including an undefined token falling back to its
 *  literal, and chained/nested fallbacks) exactly like the background side does, instead of only
 *  matching a literal white written directly in the declaration. This is the fix for the
 *  reviewer's regression #2: `color: var(--nonexistent-fg-token, #fff)` now resolves to `#ffffff`
 *  in both light and dark (the token is undefined in both, so both fall through to the literal
 *  `#fff` fallback) and is correctly classified as white. */
function isWhiteishForeground(rawValue: string): boolean {
  for (const theme of ["light", "dark"] as const) {
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
const tokenPairings = new Map<string, Pairing>(); // key: token name
const literalPairings = new Map<string, Pairing>(); // key: lowercased literal hex

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
          // Only a LITERAL hex fallback is trusted here (unchanged from before this review round —
          // this file's `parseVarCall` now parses a nested fallback like `var(--sw, var(--surface-
          // alt))` correctly instead of truncating it, but a background token's own CSS fallback is
          // deliberately NOT walked into further tokens: e.g. TagEditor's `.swatch { background:
          // var(--sw, var(--surface-alt)) }` — `--sw` is a per-row *inline* style the component sets
          // for every real swatch except `.swatch.none`, which separately overrides BOTH background
          // and color, so the base rule's `--surface-alt` fallback never actually paints text; a
          // dynamic-per-instance custom property standing in front of a token fallback is exactly the
          // "genuinely dynamic, not a static theme value this guard can check" case the sanity-check
          // comment below already calls out, not a new pairing to assert against).
          if (parsed.fallback && HEX_RE.test(parsed.fallback)) bgFallbackRaw = parsed.fallback;
        }
      } else if (HEX_RE.test(val)) {
        bgHex = val.toLowerCase();
      }
      // CPE-1632 review round 2 — disclosed scanner limitation: a `background`/`background-color`
      // value that is anything other than a literal hex or a `var(--token[, fallback])` chain —
      // `rgba(...)`, `hsl(...)`, `color-mix(...)`, or a gradient (`linear-gradient(...)` etc.) — is
      // silently SKIPPED here (bgToken/bgHex both stay undefined, so no pairing is recorded and no
      // assertion is ever generated for that rule). This is a real, currently-audited-safe gap, not
      // a false pass: every such background paired with white text in this codebase today was
      // hand-checked and clears WCAG (see the CPE-1632 ticket's Work Log), but this scanner cannot
      // verify that claim itself, and a FUTURE `background: rgba(...)`-with-white-text rule would
      // sail through unchecked exactly like the `var(--undefined-token, #fff)` foreground bug this
      // same review round fixed. `rgba()`/`hsl()` were deliberately left unextended: unlike a solid
      // `var()`/hex fill, their actual on-screen colour depends on alpha-compositing against
      // whatever sits behind them, which this static per-rule scanner has no way to know — "cheap"
      // support here would mean treating them as opaque and asserting a contrast ratio that doesn't
      // match what actually renders, which is worse than not asserting at all.
    }
    const colorMatch = bodyNorm.match(/;\s*color\s*:\s*([^;]+);/);
    const hasWhite = !!(colorMatch && isWhiteishForeground(colorMatch[1].trim()));
    // A rule that declares `color` to something OTHER than white explicitly OVERRIDES any base
    // rule's white text and must not fall back to it via the subset heuristic below (e.g.
    // `.swatch.none { color: var(--text-dim); }` overrides `.swatch { color: #fff; }`).
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
      const hex = resolveTokenOrFallback(theme, token, pairing.fallbackRaw);
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
