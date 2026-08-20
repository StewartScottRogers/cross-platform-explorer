// CPE-1810: `--warn` was referenced across five components (AgentTimeline/ConsentSheet/
// ExplorerPane/ImageCompareView/SidecarManager) as `var(--warn, <hex>)` for years without ever
// being defined anywhere in src/ — the token itself always resolved to nothing, so the literal hex
// fallback silently won at every call site, giving a "caution" colour that never changed with the
// theme (least legible in the one theme — dark — that most needed it right). This is the guard
// that CPE-1810 shipped alongside the fix, so it can't silently regress back to that shape.
//
// Two invariants, independent of every other app.css guard:
//  (a) `--warn`/`--warn-fill` resolve to a concrete hex in ALL FOUR live themes (light, dark,
//      hc-light, hc-dark) — not just light/dark. This is deliberately a HARD failure, unlike
//      src/app.css.solid-fill-contrast.test.ts's/hc-solid-fill-contrast.test.ts's own resolution,
//      which silently SKIPS asserting a pairing it can't resolve (documented there as "nothing to
//      assert against statically") — exactly the blind spot that let --warn go undefined for years
//      without any test failing. A silent skip cannot read as a pass (CPE-1806); this test makes
//      "missing from a theme" loud instead.
//  (b) no `.svelte` component still writes `var(--warn` (or `var(--warn-fill`) with a fallback —
//      the "half-migration" the ticket explicitly calls worse than none, since it would leave some
//      call sites live-themed and others silently stuck on a hex again.
import { describe, it, expect } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

const SRC = join(process.cwd(), "src");
const APP_CSS_PATH = join(SRC, "app.css");
const css = readFileSync(APP_CSS_PATH, "utf8");

const stripComments = (s: string) => s.replace(/\/\*[\s\S]*?\*\//g, "");

/** Bodies of every top-level block matching `selector { ... }` (brace-balanced), in source order —
 *  same brace-balanced helper every other app.css guard in this repo duplicates (single-file-per-
 *  guard precedent — see src/app.css.dark-contrast.test.ts's header comment for why). */
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

const HEX_RE = /^#[0-9a-fA-F]{3,8}$/;

/** All `--pal-*`/`--pal-dark-*`/`--pal-hc-light-*`/`--pal-hc-dark-*` primitive declarations,
 *  regardless of which bare :root block declares them. */
function allPaletteDecls(): Map<string, string> {
  const decls = new Map<string, string>();
  for (const block of allBlocks(css, /:root\s*\{/)) {
    for (const [name, value] of extractDecls(block)) {
      if (/^--pal-/.test(name) && HEX_RE.test(value)) decls.set(name, value);
    }
  }
  return decls;
}
const paletteDecls = allPaletteDecls();

/** Resolve a semantic token's value through the palette layer to a concrete hex, following a
 *  `var(--pal-...)` reference (or a `var(--warn)` self-reference for --warn-fill's alias case). No
 *  fallback resolution here on purpose — a token this guard checks must resolve on its own primary
 *  reference, not fall through to something else (that's exactly the "half-defined" shape being
 *  guarded against). */
function resolveHex(semanticDecls: Map<string, string>, name: string, depth = 0): string | undefined {
  if (depth > 5) return undefined;
  const value = semanticDecls.get(name);
  if (!value) return undefined;
  if (HEX_RE.test(value)) return value;
  const varMatch = value.match(/^var\((--[a-zA-Z0-9-]+)\)$/);
  if (!varMatch) return undefined;
  const ref = varMatch[1];
  if (paletteDecls.has(ref)) return paletteDecls.get(ref);
  if (semanticDecls.has(ref)) return resolveHex(semanticDecls, ref, depth + 1);
  return undefined;
}

const THEMES: { label: string; selector: RegExp }[] = [
  { label: "bare :root (default)", selector: /:root\s*\{/ },
  { label: ':root[data-theme="light"]', selector: /:root\[data-theme="light"\]\s*\{/ },
  { label: ':root[data-theme="dark"]', selector: /:root\[data-theme="dark"\]\s*\{/ },
  { label: ':root[data-theme="hc-light"]', selector: /:root\[data-theme="hc-light"\]\s*\{/ },
  { label: ':root[data-theme="hc-dark"]', selector: /:root\[data-theme="hc-dark"\]\s*\{/ },
];

/** The bare :root block that carries the semantic (not palette) layer — the one declaring --bg,
 *  same finder src/app.css.test.ts already established. */
function semanticDeclsFor(label: string, selector: RegExp): Map<string, string> {
  if (label === "bare :root (default)") {
    const bareBlocks = allBlocks(css, selector);
    // Three bare :root blocks carry --pal-* declarations (light palette, dark palette, hc palette);
    // the semantic (fallback/default) block is the one with a real --bg declaration that ISN'T
    // itself a palette block (a palette block only ever declares --pal-* raw hexes, never --bg).
    const block = bareBlocks.find((b) => /--bg\s*:/.test(b) && !/--pal-[a-zA-Z0-9-]+:\s*#/.test(b));
    if (!block) throw new Error("could not find the bare :root semantic block (the one declaring --bg)");
    return extractDecls(block);
  }
  const blocks = allBlocks(css, selector);
  if (blocks.length !== 1) throw new Error(`expected exactly one ${label} block, found ${blocks.length}`);
  return extractDecls(blocks[0]);
}

describe("--warn / --warn-fill resolve to a real hex in every live theme (CPE-1810)", () => {
  for (const { label, selector } of THEMES) {
    const semanticDecls = semanticDeclsFor(label, selector);

    it(`${label} defines --warn as a concrete hex`, () => {
      const hex = resolveHex(semanticDecls, "--warn");
      expect(hex, `--warn did not resolve to a hex in ${label} — got raw value ${JSON.stringify(semanticDecls.get("--warn"))}`).toMatch(HEX_RE);
    });

    it(`${label} defines --warn-fill as a concrete hex`, () => {
      const hex = resolveHex(semanticDecls, "--warn-fill");
      expect(hex, `--warn-fill did not resolve to a hex in ${label} — got raw value ${JSON.stringify(semanticDecls.get("--warn-fill"))}`).toMatch(HEX_RE);
    });
  }
});

// ---------------------------------------------------------------------------------------------
// No component may reintroduce the undefined-token-with-hex-fallback idiom this ticket removed.
function walkSvelte(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walkSvelte(p, out);
    else if (name.endsWith(".svelte")) out.push(p);
  }
  return out;
}

describe("no .svelte component falls back to a hard-coded hex for --warn/--warn-fill (CPE-1810)", () => {
  it('no `var(--warn` (or `var(--warn-fill`) call site carries a fallback', () => {
    const offenders: string[] = [];
    for (const f of walkSvelte(SRC)) {
      // Strip <!-- --> and /* */ comments first — AgentTimeline.svelte and TrashView.svelte both
      // carry doc comments that mention the literal string `var(--warn, <hex>)` as PROSE, quoting
      // the old idiom by name to explain why it was avoided/replaced; those aren't real CSS call
      // sites and must not trip this guard.
      const raw = readFileSync(f, "utf8");
      const content = raw.replace(/<!--[\s\S]*?-->/g, "").replace(/\/\*[\s\S]*?\*\//g, "");
      const re = /var\(\s*--warn(?:-fill)?\s*,/g;
      if (re.test(content)) offenders.push(f.replace(SRC, "src").replace(/\\/g, "/"));
    }
    expect(offenders, `component(s) still using var(--warn[-fill], <fallback>) instead of the real token: ${offenders.join(", ")}`).toEqual([]);
  });
});
