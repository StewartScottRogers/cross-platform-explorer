// CPE-1534 (epic CPE-1492, theme-token foundation): guards the palette/semantic split in
// src/app.css so the seam this ticket builds doesn't silently erode as the follow-on theme
// epics (CPE-1493 dark, CPE-1494 native accent, CPE-1495 window materials, CPE-1496 picker a11y)
// land on top of it. Two invariants:
//   (a) every semantic token that existed before this split still resolves — with the SAME
//       value — under bare `:root` (today's fallback) and under `:root[data-theme="light"]`
//       (the explicit selector a future runtime, CPE-1535, will select), and every semantic
//       value that points at a palette var actually resolves to a var that exists; and
//   (b) the semantic layer never hard-codes a hex literal directly (it must go through a
//       `--pal-*` var), and no `.svelte` component's hard-coded-hex-literal footprint grows
//       past today's baseline — a ratchet so a future edit can't quietly regress into inline
//       hex instead of reaching for a token.
import { describe, it, expect } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

const SRC = join(process.cwd(), "src");
const APP_CSS_PATH = join(SRC, "app.css");
const css = readFileSync(APP_CSS_PATH, "utf8");

/**
 * Every semantic token name that existed in the single `:root` block before this ticket
 * (src/app.css, pre-CPE-1534, lines 2-73). Captured as a fixture so a future edit that silently
 * drops or renames a token (instead of updating it everywhere) fails here.
 */
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
const paletteBlock = bareRootBlocks.find((b) => /--pal-/.test(b));
const semanticBareBlock = bareRootBlocks.find((b) => /--bg\s*:/.test(b) && b !== paletteBlock);
const lightBlocks = allBlocks(css, /:root\[data-theme="light"\]\s*\{/);

describe("app.css theme-token layering (CPE-1534)", () => {
  it("has exactly one bare :root palette block, one bare :root semantic block, and one :root[data-theme=\"light\"] block", () => {
    // There's also an unrelated third bare :root block for layout (`--filelist-cols`, app.css:432,
    // out of scope for this ticket) — that one has neither --pal- nor --bg, so it's excluded above.
    expect(paletteBlock, "no bare :root block declares --pal-* palette vars").toBeDefined();
    expect(semanticBareBlock, "no bare :root block declares --bg (the semantic layer)").toBeDefined();
    expect(lightBlocks.length, ':root[data-theme="light"] block count').toBe(1);
  });

  it('every pre-existing semantic token resolves under bare :root with an unchanged value', () => {
    const decls = extractDecls(semanticBareBlock!);
    const missing = SEMANTIC_TOKENS.filter((name) => !decls.has(name));
    expect(missing, `tokens missing from the bare :root semantic block: ${missing.join(", ")}`).toEqual([]);
  });

  it('every pre-existing semantic token resolves under :root[data-theme="light"], value-identical to bare :root', () => {
    const bareDecls = extractDecls(semanticBareBlock!);
    const lightDecls = extractDecls(lightBlocks[0]);
    const missing = SEMANTIC_TOKENS.filter((name) => !lightDecls.has(name));
    expect(missing, `tokens missing from :root[data-theme="light"]: ${missing.join(", ")}`).toEqual([]);

    const mismatched = SEMANTIC_TOKENS.filter((name) => bareDecls.get(name) !== lightDecls.get(name));
    expect(
      mismatched,
      `tokens whose value differs between bare :root and :root[data-theme="light"]: ${mismatched
        .map((n) => `${n} (${bareDecls.get(n)} vs ${lightDecls.get(n)})`)
        .join(", ")}`,
    ).toEqual([]);
  });

  it("every --pal-* reference from the semantic layer resolves to a palette var that exists", () => {
    const paletteDecls = extractDecls(paletteBlock!);
    const semanticDecls = extractDecls(semanticBareBlock!);
    const dangling: string[] = [];
    for (const [name, value] of semanticDecls) {
      const ref = value.match(/^var\((--pal-[a-zA-Z0-9-]+)\)$/);
      if (ref && !paletteDecls.has(ref[1])) dangling.push(`${name} -> ${ref[1]}`);
    }
    expect(dangling, `semantic tokens referencing a non-existent palette var: ${dangling.join(", ")}`).toEqual([]);
  });

  it("the semantic layer never hard-codes a hex literal directly (only var(--pal-*) or plain non-colour values)", () => {
    const hexPattern = /#[0-9a-fA-F]{3,8}\b/;
    for (const [block, label] of [
      [semanticBareBlock!, "bare :root semantic block"],
      [lightBlocks[0], ':root[data-theme="light"] block'],
    ] as const) {
      const clean = stripComments(block);
      const offenders = clean
        .split(";")
        .map((decl) => decl.trim())
        .filter((decl) => hexPattern.test(decl));
      expect(offenders, `${label} has hex literals outside the palette layer: ${offenders.join(" | ")}`).toEqual([]);
    }
  });
});

// ---------------------------------------------------------------------------------------------
// Component hard-coded-hex ratchet: today's footprint, captured as a baseline. A future PR may
// migrate a file onto tokens (shrinking these numbers — update the baseline down when that
// happens) but must not grow past it, i.e. no new inline hex sneaking into a component instead
// of reaching for a --token. Pre-existing hex in icons/terminal-theme mirrors/etc. is intentional
// and out of scope for CPE-1534 (a pure app.css refactor) — this is a growth guard, not a
// zero-tolerance rule.
const HEX_LITERAL = /#[0-9a-fA-F]{3,8}\b/g;
const BASELINE_FILES_WITH_HEX = 90;
const BASELINE_TOTAL_HEX_OCCURRENCES = 466;

function walkSvelte(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walkSvelte(p, out);
    else if (name.endsWith(".svelte")) out.push(p);
  }
  return out;
}

describe("component hard-coded-hex ratchet (CPE-1534)", () => {
  it("no growth in hard-coded hex literals across .svelte components vs. the CPE-1534 baseline", () => {
    const files = walkSvelte(SRC);
    let filesWithHex = 0;
    let totalOccurrences = 0;
    for (const f of files) {
      const matches = readFileSync(f, "utf8").match(HEX_LITERAL);
      if (matches && matches.length) {
        filesWithHex++;
        totalOccurrences += matches.length;
      }
    }
    expect(filesWithHex, "files containing a hard-coded hex literal").toBeLessThanOrEqual(BASELINE_FILES_WITH_HEX);
    expect(totalOccurrences, "total hard-coded hex literal occurrences").toBeLessThanOrEqual(
      BASELINE_TOTAL_HEX_OCCURRENCES,
    );
  });
});
