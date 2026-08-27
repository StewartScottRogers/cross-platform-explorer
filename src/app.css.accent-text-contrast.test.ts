// CPE-1919: the guard half of the fix, and the half the ticket says matters more than the colour.
//
// THE DEFECT. The JSON tree preview paints string values with the accent colour at 12px. In the
// dark theme that measured 3.70:1 against --bg (#202020), 3.21:1 against --surface (#2b2b2b — the
// ground `.preview-pane` actually paints, which is what the pane really renders on) and 3.43:1
// against --surface-alt (#262626, the `.jt-row:hover` fill). WCAG 2.1 SC 1.4.3 wants >=4.5:1 for
// normal text. So body text in a shipped preview was a third under bar.
//
// WHY EVERY EXISTING GUARD WAS GREEN. Two independent blind spots, both worth naming because both
// recur:
//
//  (1) THE PAIRING WAS ENUMERATED AT THE WRONG BAR, not missing. src/app.css.dark-contrast.test.ts
//      does assert --accent against --bg and --surface — at >=3:1, labelled "used as text/icon/
//      focus-ring accent, WCAG 1.4.11". That is the correct bar for the roles CPE-1632 tuned
//      --accent for (a solid button fill, an icon glyph, a focus ring) and the wrong one for the
//      role JsonTreeNode.svelte actually uses it in (running text). A single token backing roles
//      with different bars will always be pinned at the LOOSEST of them, and the loosest assertion
//      reads, to anyone scanning the suite, exactly like coverage. CPE-1919's answer is to split
//      the roles into two tokens (--accent for fills/icons/rings, --accent-text for text) so each
//      can be held to its own bar, instead of choosing one number for both.
//
//  (2) NOTHING CHECKED THE PAINTED SURFACE. Every existing palette guard measures against --bg and
//      --surface because those are the plausible grounds, not because anything verified which one
//      a given component lands on. The JSON tree's worst reading (3.21:1) is on --surface via
//      `.preview-pane`, and its hover fill (--surface-alt) is a THIRD ground no palette guard
//      measures text against at all. So this file derives the surfaces from the real CSS — see
//      "painted surfaces" below — and fails if that CSS moves out from under it.
//
// WHAT THIS FILE ASSERTS. Four things:
//   (a) --accent-text resolves to a concrete hex in all five live theme selectors (bare :root,
//       light, dark, hc-light, hc-dark) — the hard-failure shape src/app.css.warn-token.test.ts
//       established, so a token missing from one theme is loud rather than silently inherited.
//   (b) --accent-text clears the text bar on every painted surface in every theme.
//   (c) EVERY colour role the JSON preview paints — not just the string value that was reported —
//       clears the text bar on every painted surface in every theme, with the role list DERIVED by
//       parsing JsonTree.svelte + JsonTreeNode.svelte at run time (CPE-1932: enumerate, don't
//       recall). A role added to that component tomorrow is measured automatically; a hand-kept
//       list would not have been, and the sibling roles beside the reported one are exactly where
//       nobody had looked.
//   (d) --accent-text is never used as a `background`. It is deliberately the brightest of the
//       accent family and white on it is only 3.53:1 (dark) / 1.90:1 (hc-dark) — under WCAG
//       1.4.11's 3:1 UI floor — so the inverse misuse is a real regression, and this is the same
//       "a token calibrated for one role, used in another" mistake that caused CPE-1919 itself.
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const SRC = join(process.cwd(), "src");
const APP_CSS_PATH = join(SRC, "app.css");
const css = readFileSync(APP_CSS_PATH, "utf8");

const JSON_TREE_FILES = [
  join(SRC, "lib", "components", "JsonTree.svelte"),
  join(SRC, "lib", "components", "JsonTreeNode.svelte"),
];

const stripComments = (s: string) => s.replace(/\/\*[\s\S]*?\*\//g, "");

/** Bodies of every top-level block matching `selector { ... }` (brace-balanced), in source order —
 *  the same brace-balanced helper every other app.css guard in this repo duplicates (single-file-
 *  per-guard precedent; see src/app.css.dark-contrast.test.ts's header for why). */
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

function extractDecls(block: string): Map<string, string> {
  const clean = stripComments(block);
  const decls = new Map<string, string>();
  const re = /(--[a-zA-Z0-9-]+)\s*:\s*([^;]+);/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(clean)) !== null) decls.set(m[1], m[2].trim());
  return decls;
}

// ---------------------------------------------------------------------------------------------
// Theme resolution. Palette primitives (--pal-*) live on bare :root blocks shared by every theme;
// each theme's own block re-points the semantic layer at its palette. A token is resolved by
// looking in the theme block first, then any palette block, transitively.
const bareRootBlocks = allBlocks(css, /:root\s*\{/);
const paletteDecls = new Map<string, string>();
for (const block of bareRootBlocks) {
  for (const [name, value] of extractDecls(block)) {
    if (!paletteDecls.has(name)) paletteDecls.set(name, value);
  }
}

/** The bare `:root` semantic block (the light/default one) is a live theme selector too — an app
 *  that never sets data-theme paints from it — so it is guarded alongside the four named themes. */
const bareSemanticBlock = bareRootBlocks.find((b) => /--bg\s*:/.test(b));
if (!bareSemanticBlock) throw new Error("no bare :root block declares --bg (the semantic layer)");

const THEMES: { label: string; decls: Map<string, string>; textBar: number; hoverBar: number }[] = [
  // Normal themes: WCAG AA, >=4.5:1 for normal text on every painted surface.
  { label: "bare :root (default)", decls: extractDecls(bareSemanticBlock), textBar: 4.5, hoverBar: 4.5 },
  { label: "light", decls: themeDecls("light"), textBar: 4.5, hoverBar: 4.5 },
  { label: "dark", decls: themeDecls("dark"), textBar: 4.5, hoverBar: 4.5 },
  // High-contrast themes hold TEXT tokens to the AAA-inspired >=7:1 bar on --bg/--surface, matching
  // src/app.css.hc-contrast.test.ts's own convention for --text/--danger/--success/--text-muted.
  // --surface-alt is not a ground that file measures at all, so it is held to AA here rather than
  // invented at AAA: this guard's job is to stop a regression below AA on a real painted surface,
  // not to retroactively raise a bar the hc palettes were never tuned against.
  { label: "hc-light", decls: themeDecls("hc-light"), textBar: 7, hoverBar: 4.5 },
  { label: "hc-dark", decls: themeDecls("hc-dark"), textBar: 7, hoverBar: 4.5 },
];

function themeDecls(name: string): Map<string, string> {
  const blocks = allBlocks(css, new RegExp(`:root\\[data-theme="${name}"\\]\\s*\\{`));
  if (blocks.length !== 1) {
    throw new Error(`expected exactly one :root[data-theme="${name}"] block, found ${blocks.length}`);
  }
  return extractDecls(blocks[0]);
}

function resolveHex(themeIndex: number, value: string | undefined, depth = 0): string | undefined {
  if (!value || depth > 8) return undefined;
  if (/^#[0-9a-fA-F]{3,8}$/.test(value)) return value;
  const ref = value.match(/^var\((--[a-zA-Z0-9-]+)\)$/);
  if (!ref) return undefined;
  const name = ref[1];
  const themeValue = THEMES[themeIndex].decls.get(name);
  if (themeValue !== undefined) return resolveHex(themeIndex, themeValue, depth + 1);
  return resolveHex(themeIndex, paletteDecls.get(name), depth + 1);
}

const tokenHex = (themeIndex: number, token: string): string | undefined =>
  resolveHex(themeIndex, THEMES[themeIndex].decls.get(token) ?? paletteDecls.get(token));

// ---------------------------------------------------------------------------------------------
// WCAG 2.1 relative luminance + contrast ratio (https://www.w3.org/TR/WCAG21/#dfn-contrast-ratio).
function relativeLuminance(hex: string): number {
  let h = hex.replace("#", "");
  if (h.length === 3) h = h.split("").map((c) => c + c).join("");
  const [r, g, b] = [0, 2, 4]
    .map((i) => parseInt(h.substring(i, i + 2), 16) / 255)
    .map((s) => (s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4)));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrastRatio(a: string, b: string): number {
  const [lighter, darker] = [relativeLuminance(a), relativeLuminance(b)].sort((x, y) => y - x);
  return (lighter + 0.05) / (darker + 0.05);
}

// ---------------------------------------------------------------------------------------------
// The painted surfaces, DERIVED — not assumed.
//
// The JSON tree renders inside `aside.preview` (PreviewPane.svelte), which sets no background of
// its own, so it inherits from `.preview-pane` in app.css. `.jt-row:hover` then repaints the row
// under the cursor. Both are read out of the real CSS below rather than written down here, and
// each read asserts it found something: if a future ticket repaints the pane or the hover fill,
// this guard's surface list would be measuring the wrong ground — the exact failure mode named in
// the ticket ("is the guard checking a nominal background rather than the painted one?") — so it
// must fail loudly instead of quietly grading against a surface nobody paints any more.
function backgroundTokenOf(source: string, ruleSelector: RegExp, label: string): string {
  const blocks = allBlocks(stripComments(source), ruleSelector);
  if (blocks.length !== 1) {
    throw new Error(`${label}: expected exactly one matching CSS rule, found ${blocks.length}`);
  }
  const bg = blocks[0].match(/background(?:-color)?\s*:\s*var\((--[a-zA-Z0-9-]+)\)/);
  if (!bg) throw new Error(`${label}: rule no longer sets \`background: var(--token)\``);
  return bg[1];
}

const jsonTreeNodeCss = readFileSync(JSON_TREE_FILES[1], "utf8");

/** `.preview-pane`'s background — the ground the whole preview, JSON tree included, sits on. */
const PANE_SURFACE = backgroundTokenOf(css, /\.preview-pane\s*\{/, ".preview-pane (app.css)");
/** `.jt-row:hover`'s fill — a third ground, measured by no other palette guard. */
const HOVER_SURFACE = backgroundTokenOf(
  jsonTreeNodeCss,
  /\.jt-row:hover\s*\{/,
  ".jt-row:hover (JsonTreeNode.svelte)",
);

/** `--bg` is the window ground behind everything (and the surface the ticket's own screenshot
 *  measurement of 3.70:1 was taken against), so it is measured too even though the in-app pane
 *  paints --surface over it. */
const PAINTED_SURFACES: { token: string; note: string; isHover: boolean }[] = [
  { token: "--bg", note: "window ground", isHover: false },
  { token: PANE_SURFACE, note: ".preview-pane background", isHover: false },
  { token: HOVER_SURFACE, note: ".jt-row:hover fill", isHover: true },
];

// ---------------------------------------------------------------------------------------------
// The JSON preview's colour roles, DERIVED from the component CSS (CPE-1932: enumerate, don't
// recall). Every `color: var(--token)` in JsonTree.svelte + JsonTreeNode.svelte, comments stripped
// first so a token named in prose can't be mistaken for a call site. `border-*-color` /
// `background-color` are excluded by the lookbehind — they are non-text roles with a different bar.
function colorRolesOf(path: string): { token: string; file: string }[] {
  const content = stripComments(readFileSync(path, "utf8"));
  const out: { token: string; file: string }[] = [];
  const re = /(?<![-\w])color\s*:\s*var\((--[a-zA-Z0-9-]+)\)/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(content)) !== null) out.push({ token: m[1], file: path.split(/[\\/]/).pop()! });
  return out;
}

const JSON_TREE_ROLES = JSON_TREE_FILES.flatMap(colorRolesOf);
const JSON_TREE_TOKENS = [...new Set(JSON_TREE_ROLES.map((r) => r.token))].sort();

describe("--accent-text (CPE-1919): the accent's body-text role, split out of --accent", () => {
  it("resolves to a concrete hex in every live theme selector", () => {
    const unresolved = THEMES.map((t, i) => [t.label, tokenHex(i, "--accent-text")] as const).filter(
      ([, hex]) => !hex || !/^#[0-9a-fA-F]{6}$/.test(hex),
    );
    expect(
      unresolved.map(([label, hex]) => `${label} -> ${hex ?? "(unresolved)"}`),
      "--accent-text must resolve in every theme; an unresolved token silently inherits another theme's value",
    ).toEqual([]);
  });

  it("clears the text bar on every painted surface, in every theme", () => {
    const failures: string[] = [];
    THEMES.forEach((theme, i) => {
      const fg = tokenHex(i, "--accent-text")!;
      for (const surface of PAINTED_SURFACES) {
        const bgHex = tokenHex(i, surface.token)!;
        const bar = surface.isHover ? theme.hoverBar : theme.textBar;
        const ratio = contrastRatio(fg, bgHex);
        if (ratio < bar) {
          failures.push(
            `${theme.label}: --accent-text (${fg}) on ${surface.token} (${bgHex}, ${surface.note}) = ` +
              `${ratio.toFixed(2)}:1, want >=${bar}:1`,
          );
        }
      }
    });
    expect(failures, `--accent-text under the text bar:\n${failures.join("\n")}`).toEqual([]);
  });

  it("is never used as a background — it is a text-only colour (white on it is under the 3:1 UI floor)", () => {
    const offenders: string[] = [];
    const bgRe = /background(?:-color)?\s*:[^;]*var\(\s*--accent-text\b/;
    for (const [label, source] of [
      ["app.css", css],
      ...JSON_TREE_FILES.map((p) => [p.split(/[\\/]/).pop()!, readFileSync(p, "utf8")] as const),
    ] as const) {
      for (const line of stripComments(source).split("\n")) {
        if (bgRe.test(line)) offenders.push(`${label}: ${line.trim()}`);
      }
    }
    expect(offenders, `--accent-text used as a fill: ${offenders.join(" | ")}`).toEqual([]);
  });
});

describe("JSON preview palette (CPE-1919): every colour role, every surface, every theme", () => {
  it("derived a plausible set of colour roles from the JSON tree components", () => {
    // Tripwire for the guard going blind: if the parse ever comes back near-empty (a component
    // renamed, moved, or restyled through a mechanism this regex doesn't see) every assertion below
    // would vacuously pass. A silent skip must not read as a pass (CPE-1806). Five distinct tokens
    // is comfortably under today's set, so ordinary edits don't trip it — only a parse that broke.
    expect(
      JSON_TREE_TOKENS.length,
      `only found ${JSON_TREE_TOKENS.length} colour tokens (${JSON_TREE_TOKENS.join(", ")}) in ` +
        `${JSON_TREE_FILES.join(", ")} — the derivation is probably broken, not the palette`,
    ).toBeGreaterThanOrEqual(5);
    // The reported defect's own role must be among them, by name: the string value.
    expect(JSON_TREE_TOKENS, "the JSON tree must paint string values with --accent-text").toContain("--accent-text");
    expect(JSON_TREE_TOKENS, "--accent is a fill/icon colour, not a body-text colour").not.toContain("--accent");
  });

  it("every derived role clears the text bar on --bg, the pane surface, and the hover fill, in all five themes", () => {
    const failures: string[] = [];
    THEMES.forEach((theme, i) => {
      for (const token of JSON_TREE_TOKENS) {
        const fg = tokenHex(i, token);
        if (!fg || !/^#[0-9a-fA-F]{6}$/.test(fg)) {
          failures.push(`${theme.label}: ${token} did not resolve to a hex (got ${fg ?? "nothing"})`);
          continue;
        }
        for (const surface of PAINTED_SURFACES) {
          const bgHex = tokenHex(i, surface.token)!;
          const bar = surface.isHover ? theme.hoverBar : theme.textBar;
          const ratio = contrastRatio(fg, bgHex);
          if (ratio < bar) {
            const where = JSON_TREE_ROLES.filter((r) => r.token === token)
              .map((r) => r.file)
              .filter((f, n, a) => a.indexOf(f) === n)
              .join("/");
            failures.push(
              `${theme.label}: ${token} (${fg}, ${where}) on ${surface.token} (${bgHex}, ` +
                `${surface.note}) = ${ratio.toFixed(2)}:1, want >=${bar}:1`,
            );
          }
        }
      }
    });
    expect(failures, `JSON preview roles under the text bar:\n${failures.join("\n")}`).toEqual([]);
  });

  it("keeps the string value distinguishable from the keys and from the numbers beside it", () => {
    // The ticket's second acceptance criterion: a fix that makes strings legible but identical to
    // keys trades one defect for another. Luminance contrast is the wrong measure for "are these
    // two colours telling apart" (the light theme's accent and --text-dim differ by 1.09:1 yet are
    // obviously a blue and a grey), so this asserts the thing that actually carries the difference:
    // the string colour must be a DIFFERENT resolved hex from the key colour and from the number/
    // boolean colour, in every theme.
    const collisions: string[] = [];
    THEMES.forEach((theme, i) => {
      const stringHex = tokenHex(i, "--accent-text");
      for (const [role, token] of [
        ["key", "--text-dim"],
        ["number/boolean", "--text"],
        ["null/punctuation", "--text-faint"],
      ] as const) {
        if (tokenHex(i, token) === stringHex) {
          collisions.push(`${theme.label}: string value and ${role} both resolve to ${stringHex}`);
        }
      }
    });
    expect(collisions, collisions.join("\n")).toEqual([]);
  });
});
