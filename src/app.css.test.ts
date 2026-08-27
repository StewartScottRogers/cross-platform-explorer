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
import { join, relative } from "node:path";

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
// zero-tolerance rule. Deliberate exemptions, and why each is a decision rather than an accident:
//   - Icon.svelte's SVG `fill=`/`stroke=` art — decorative icon colour, not app theming.
//   - TerminalPanel.svelte's xterm `theme: {...}` object — xterm paints to canvas, which cannot
//     take a CSS var; the literals mirror app.css tokens by the file's own comment.
//   - ColorRulesDialog.svelte's `newColor = "#e2504b"` default and `rule.color ?? "#888888"`
//     fallback (CPE-1931 Reviewer round) — a starting swatch and a colour-picker fallback are
//     VALUES THE USER CHOOSES FROM (like a `<input type="color">`'s own default), not the app's
//     own chrome, so they were never tokenizable in the first place.
const HEX_LITERAL = /#[0-9a-fA-F]{3,8}\b/g;

// CPE-1931: a colour can only ever land in a `.svelte` file in a CSS value position — inside a
// `<style>` block or an inline `style="..."` attribute. The pre-CPE-1931 matcher ran HEX_LITERAL
// over the WHOLE FILE TEXT, comments included, so a `// PR #1044` or `<!-- see #1892 -->` counted
// a ticket/PR reference as a hard-coded colour the moment its digits happened to all be valid hex
// — which is every PR/ticket number from here on, now that this repo is in the CPE-1900s. Fixed by
// narrowing the scan to the two places a colour can actually appear, and stripping CSS `/* */`
// comments from each before matching (this file's own `stripComments` helper, reused rather than
// re-derived, matches the same shape used above for app.css's own semantic-layer check).
//
// SVG icon `fill="#.."` / `stroke="#.."` are markup attributes, not `style=`, so they fall outside
// this scan too — that is not a new exemption, just this file's existing "icons are intentional and
// out of scope" note (above) now enforced structurally instead of by accident.
const STYLE_BLOCK = /<style\b[^>]*>([\s\S]*?)<\/style>/gi;
const STYLE_ATTR = /\b(?:style|style:[a-zA-Z][\w-]*)\s*=\s*(?:"((?:[^"\\]|\\.)*)"|'((?:[^'\\]|\\.)*)')/gi;
const STYLE_ASSIGNMENT_START = /\.style\.(?:cssText|[a-zA-Z][\w-]*)\s*=\s*/g;

// CPE-1931 Reviewer round (UAT on PR #1049): the first cut of this narrowing dropped a real, live
// hard-coded colour -- FileList.svelte's `badge.style.cssText = "...color:#fff;..."` -- because
// neither STYLE_BLOCK nor STYLE_ATTR reaches a `<script>`-side `.style` assignment. Closed by
// `scriptStyleAssignmentValues` below, which walks character-by-character (same "track quote
// state" shape as `stripShellComment`/`logicalLines` in `releaseHangHardening.test.ts`, but --
// unlike that precedent -- WITH backslash-escape awareness inside quotes, since a `\"` inside a
// `.style.cssText` string is exactly the shape that precedent's own cautionary tale got wrong) so
// a string-concatenated assignment like FileList.svelte's (two literals joined by `+`, with `;`
// characters INSIDE each CSS-value string long before the real statement-ending `;`) is read whole
// rather than truncated at the first semicolon.
//
// The same round widened STYLE_ATTR (above) to also match the literal form of a `style:` directive
// (`style:color="#fff"`) -- `style:display={...}` (TerminalPanel.svelte:240, no hex today) still
// is not covered, because a `{...}` expression's value lives in JS, not the template text; a
// literal hex written INSIDE that expression as `style:color={"#fff"}` would still evade this
// scan. No live site does that today -- documented here rather than fixed, since a general
// JS-expression scan is out of this ticket's proportion.

/** Every literal string segment assigned to a DOM node's `.style` (`.style.cssText = "..."` /
 *  `.style.color = "..."`), concatenation included, up to the real (outside-any-quote) statement
 *  terminator. Quote-aware and backslash-escape-aware char-by-char, on purpose: a naive
 *  `/\.style\.\w+\s*=\s*([^;]+);/` stops at the FIRST `;` it sees, which for `.cssText` is almost
 *  always INSIDE the first quoted CSS-declaration string (CSS uses `;` as its own separator), long
 *  before the statement's real end. */
function scriptStyleAssignmentValues(source: string): string[] {
  const values: string[] = [];
  const startRe = new RegExp(STYLE_ASSIGNMENT_START.source, STYLE_ASSIGNMENT_START.flags);
  let m: RegExpExecArray | null;
  while ((m = startRe.exec(source)) !== null) {
    let i = m.index + m[0].length;
    let quote: string | null = null;
    let value = "";
    while (i < source.length) {
      const ch = source[i];
      if (quote) {
        if (ch === "\\" && i + 1 < source.length) {
          value += ch + source[i + 1];
          i += 2;
          continue;
        }
        if (ch === quote) {
          quote = null;
          i += 1;
          continue;
        }
        value += ch;
        i += 1;
        continue;
      }
      if (ch === '"' || ch === "'" || ch === "`") {
        quote = ch;
        i += 1;
        continue;
      }
      if (ch === "/" && source[i + 1] === "/") {
        while (i < source.length && source[i] !== "\n") i += 1;
        continue;
      }
      if (ch === "/" && source[i + 1] === "*") {
        i += 2;
        while (i < source.length && !(source[i] === "*" && source[i + 1] === "/")) i += 1;
        i += 2;
        continue;
      }
      if (ch === ";") break;
      i += 1;
    }
    values.push(value);
  }
  return values;
}

/** Every hex-literal match sitting in a real CSS value position in `.svelte` source: inside
 *  `<style>` block bodies, inline `style="..."`/`style:prop="..."` attribute values, and JS
 *  `.style.*` string assignments -- each with comments stripped first, so a comment sitting in
 *  any of the three still can't be miscounted. */
function hexColourSites(source: string): string[] {
  const sites: string[] = [];
  let m: RegExpExecArray | null;
  const styleBlockRe = new RegExp(STYLE_BLOCK.source, STYLE_BLOCK.flags);
  while ((m = styleBlockRe.exec(source)) !== null) {
    const clean = stripComments(m[1]);
    const hits = clean.match(HEX_LITERAL);
    if (hits) sites.push(...hits);
  }
  const styleAttrRe = new RegExp(STYLE_ATTR.source, STYLE_ATTR.flags);
  while ((m = styleAttrRe.exec(source)) !== null) {
    const value = m[1] !== undefined ? m[1] : (m[2] ?? "");
    const clean = stripComments(value);
    const hits = clean.match(HEX_LITERAL);
    if (hits) sites.push(...hits);
  }
  for (const value of scriptStyleAssignmentValues(source)) {
    const hits = value.match(HEX_LITERAL);
    if (hits) sites.push(...hits);
  }
  return sites;
}
// CPE-1810 ratcheted these down in two passes:
//  1. Migrating AgentTimeline/ConsentSheet/ExplorerPane/ImageCompareView/SidecarManager off the
//     undefined `--warn` token's `var(--warn, <hex>)` fallback idiom (onto the now-real
//     `--warn`/`--warn-fill` semantic tokens) deleted one hard-coded hex literal per fallback call
//     site — 24 in total across those 5 files — and dropped ImageCompareView.svelte out of the
//     "has hex" set entirely (its only hex literals were --warn fallbacks). Was 90/466 (actual
//     pre-CPE-1810 count was 88/466 — the files count already had 2 free of slack before this
//     ticket). That pass brought it to 87/442.
//  2. A PR review round then found the first pass's own grep shape (`var(--token, <fallback>)`)
//     was blind to sites that hard-code the SAME amber literals (`#b5872b`/`#b8860b`) directly with
//     NO token reference at all — the exact shape the ticket's "Why it matters" named those two
//     hexes for. A follow-up sweep (AgentTimeline/CheckpointDialog/CompareDialog/ConflictDialog/
//     DataBrowser/ExplorerPane/FileList/IntegrityDialog/Sidebar/StatusBar/SyncDialog — 19 call
//     sites, 20 hex occurrences since one SyncDialog rule carried the literal twice) migrated those
//     onto `--warn`/`--warn-fill` too and dropped DataBrowser.svelte out of the "has hex" set
//     entirely (its only hex literal was this same amber). `src/lib/sessionChip.ts`'s `#b5872b` is
//     NOT part of this — it's a `.ts` file (this ratchet only walks `.svelte`), and even if it were
//     `.svelte` it would still be excluded: it's a fixed categorical session-identity palette,
//     deliberately theme-invariant by its own comment, not a caution/warning semantic. That pass
//     brought it to 86/422.
//  3. The same review round's Visual Critic pass found a semantic INVERSION this ticket's own
//     migration had frozen in place: SidecarManager.svelte's `.log-error` had been pointed at
//     `var(--warn)` (wrong — that's the caution token marking an actual error) while `.log-warn`
//     sat right below it on a still-bare `#c9a227` (2.42:1 on white — below AA, below even the 3:1
//     large-text floor, and in dark only 1.27:1 separated it from `.log-error` — indistinguishable
//     log levels). Fixed the pair: `.log-error` -> `var(--danger)`, `.log-warn` -> `var(--warn)`.
//     One more hex literal removed. Brought it to 86/421.
//  4. A further review round found step 3's fix was itself wrong: SidecarManager.svelte's `.logs`
//     pane (which `.log-error`/`.log-warn` render inside) sets `background: var(--bg-dim, <hex>)`,
//     and `--bg-dim` is undefined nowhere in app.css — so that pane's REAL background is the fixed
//     literal fallback in every theme, never the theme's own `--surface`/`--bg` that `--danger`/
//     `--warn` are calibrated against. Re-measured against the pane's actual fixed backdrop, light
//     and hc-light both fell under WCAG AA (hc-light's `.log-error` even under the 3:1 UI floor) —
//     strictly worse than the flat pre-ticket literals, which cleared ~6.8-7.9:1 there by accident
//     of having been picked for a dark backdrop. Reverted `.log-error`/`.log-warn` to those literal
//     values (not retuned — `--danger`/`--warn` are global tokens serving many surfaces and this
//     pane's background is broken independently of them) with a comment explaining why, pending
//     CPE-1821 (which now owns this whole log pane) making `--bg-dim` real. Two hex literals back.
//     Brought it to 86/423.
//  5. CPE-1821 defined the three tokens CPE-1810 explicitly deferred — `--text-muted` (undefined
//     everywhere, 20 call sites across AgentTimeline/ConsultedFiles/FileList all carrying a bare
//     `#9a9a9a` fallback), `--accent-2` (1 call site, BackupDashboard's `.mirror.auto`, `#209764`
//     fallback), and `--bg-dim` (1 call site, SidecarManager's `.logs` pane, the `#0f0f0f` fallback
//     step 4 above restored) — deleting one hex literal per fallback call site, 22 in total. Making
//     `--bg-dim` real also finally let `.log-error`/`.log-warn` retokenize onto `var(--danger)`/
//     `var(--warn)` per step 4's own note (measured safe THIS time — see SidecarManager.svelte's
//     updated comment for the numbers against the pane's real, now-resolved background), removing
//     2 more literals (`#d08b2b`, `#c9a227`). No file dropped out of the "has hex" set — every one
//     of the five touched files (AgentTimeline/ConsultedFiles/FileList/BackupDashboard/
//     SidecarManager) still carries unrelated hex literals out of this ticket's scope. Brought it to
//     86/399 (423 − 24).
//  6. CPE-1931: the matcher above (steps 1-5) ran HEX_LITERAL over the raw WHOLE-FILE text,
//     comments included, so a comment citing a ticket/PR number whose digits are all valid hex
//     (`PR #1044`, and every `#19NN` ticket from here on) counted as a hard-coded colour. This
//     stopped being a hypothetical the moment the repo's ticket numbers crossed into all-hex
//     territory: it sent PR #1044 red on two comments, not a colour, in the CPE-1900s. Replaced the
//     whole-file scan with `hexColourSites()` (above), which only looks where a colour can actually
//     land, stripping CSS comments from each first. Re-baselined from scratch per the ticket's
//     explicit instruction (recount, don't patch the old total forward by subtracting the two known
//     false positives — CPE-1922's failure mode). First cut: 85 files / 276 occurrences.
//  7. CPE-1931 Reviewer round (UAT on PR #1049): independently re-implemented step 6's matcher,
//     confirmed the 86/399 → 85/276 recount exactly, then categorised all 123 dropped occurrences —
//     and found step 6's own PR description claim, that the gap was "comments, doc examples, and
//     non-`style=` attributes the old regex was never entitled to count," was FACTUALLY WRONG for
//     three of them, which were real, live hard-coded colours the narrowing had silently dropped:
//       - FileList.svelte:340's `badge.style.cssText = "...color:#fff;..."` — a genuine CSS value
//         built as a JS string and assigned straight onto a DOM node's `.style`, invisible to a
//         matcher that only looked inside `<style>`/`style=`. Fixed by adding
//         `scriptStyleAssignmentValues()` (above), which now counts it — bringing the total up one,
//         to 277 (the files count stays 85: FileList.svelte was already in the "has hex" set for
//         its other 6 style-block/style-attr occurrences).
//       - ColorRulesDialog.svelte:27's `newColor = "#e2504b"` default swatch and :129's
//         `rule.color ?? "#888888"` picker fallback — genuinely data the user picks their own value
//         over, not app theming, so left OUT of the counted set — but now an EXPLICIT, reasoned
//         exemption (see the top-of-file exemption list above and the comments at both sites)
//         instead of a silent, accidental one.
//     TerminalPanel.svelte's 4 xterm `theme: {...}` values (background/foreground/cursor/
//     selectionBackground) also dropped, consistent with this file's own pre-existing "icons and
//     terminal-theme mirrors are intentional and out of scope" note — but honestly: that moves them
//     from COUNTED-BUT-TOLERATED to STRUCTURALLY INVISIBLE to this ratchet, losing growth-visibility
//     for that one category even though growth was never the thing being watched there. New
//     baseline after both fixes: 85 files / 277 occurrences.
const BASELINE_FILES_WITH_HEX = 85;
const BASELINE_TOTAL_HEX_OCCURRENCES = 277;

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
    const perFile: { file: string; count: number }[] = [];
    for (const f of files) {
      const hits = hexColourSites(readFileSync(f, "utf8"));
      if (hits.length) {
        filesWithHex++;
        totalOccurrences += hits.length;
        perFile.push({ file: relative(SRC, f).split("\\").join("/"), count: hits.length });
      }
    }
    // CPE-1931 UAT round: a bare "N > baseline" number tells a developer the ratchet moved but not
    // WHERE or WHAT TO DO — they'd have to go diff their own change against this list by hand. Name
    // the files (sorted so the biggest offenders read first) and the remedy in the failure message
    // itself, so fixing the real colour is the path of least resistance, not raising the baseline.
    perFile.sort((a, b) => b.count - a.count || a.file.localeCompare(b.file));
    const offenders = perFile.map(({ file, count }) => `  ${file} (${count})`).join("\n");
    const remedy =
      "Move the new hex literal(s) onto a semantic theme token instead (defined in BOTH the bare " +
      ':root and :root[data-theme="light"]/[data-theme="dark"] blocks in app.css) — never leave a ' +
      "new hard-coded hex in a <style> block or style= attribute. Files currently carrying " +
      `hard-coded hex in a style position, most first:\n${offenders}`;
    expect(
      filesWithHex,
      `files containing a hard-coded hex literal in a style position: ${filesWithHex} > baseline ${BASELINE_FILES_WITH_HEX}. ${remedy}`,
    ).toBeLessThanOrEqual(BASELINE_FILES_WITH_HEX);
    expect(
      totalOccurrences,
      `total hard-coded hex literal occurrences in style positions: ${totalOccurrences} > baseline ${BASELINE_TOTAL_HEX_OCCURRENCES}. ${remedy}`,
    ).toBeLessThanOrEqual(BASELINE_TOTAL_HEX_OCCURRENCES);
  });
});

// CPE-1931: lock in hexColourSites()'s two directions directly, on synthetic input, rather than
// relying on the whole-repo count above to happen to exercise both. A regression in either
// direction is dangerous in a different way: missing a real `<style>` hex silently lets new
// hard-coded colour ship (the growth guard goes blind); counting a comment's ticket/PR reference
// re-breaks CI on every future ticket number, which is the exact defect this ticket exists to fix.
describe("hexColourSites() matches only CSS value positions (CPE-1931)", () => {
  it("still catches a hard-coded hex literal inside a <style> block", () => {
    const svelte = `<div class="x" />\n<style>\n  .x { color: #ff00aa; }\n</style>\n`;
    expect(hexColourSites(svelte)).toEqual(["#ff00aa"]);
  });

  it("still catches a hard-coded hex literal inside an inline style= attribute", () => {
    const svelte = `<div style="background: #123456;" />\n`;
    expect(hexColourSites(svelte)).toEqual(["#123456"]);
  });

  it("does not count a PR/ticket reference in a // comment as a colour", () => {
    const svelte = `<script>\n  // PR #1044 review round 2\n</script>\n<div />\n`;
    expect(hexColourSites(svelte)).toEqual([]);
  });

  it("does not count a PR/ticket reference in an <!-- --> comment as a colour", () => {
    const svelte = `<!-- see #1892 for context -->\n<div />\n`;
    expect(hexColourSites(svelte)).toEqual([]);
  });

  it("does not count a /* */ comment inside a <style> block as a colour", () => {
    const svelte = `<style>\n  /* was #1044, migrated off */\n  .x { color: var(--text); }\n</style>\n`;
    expect(hexColourSites(svelte)).toEqual([]);
  });

  it("does not count an SVG fill=/stroke= attribute (out of this ratchet's scope, same as Icon.svelte)", () => {
    const svelte = `<svg><path fill="#ffd166" stroke="#e0a800" /></svg>\n`;
    expect(hexColourSites(svelte)).toEqual([]);
  });

  // CPE-1931 Reviewer round (UAT on PR #1049): the coverage added below in response to the three
  // real drops it found — FileList.svelte's `.style.cssText` assignment, and confirmation that the
  // two ColorRulesDialog.svelte data-not-theming sites stay deliberately quiet.
  it("catches a hex literal in a simple .style.<prop> = \"...\" JS assignment", () => {
    const svelte = `<script>\n  el.style.color = "#123456";\n</script>\n`;
    expect(hexColourSites(svelte)).toEqual(["#123456"]);
  });

  it("catches a hex literal in a .style.cssText assignment built from concatenated strings whose CSS declarations carry internal semicolons before the real statement end (the exact FileList.svelte:340 shape)", () => {
    const svelte = `<script>
  function setDragBadge() {
    badge.style.cssText =
      "position:absolute; top:-1000px; left:-1000px; padding:4px 10px; border-radius:6px;" +
      "background:var(--accent); color:#fff; font:600 12px system-ui,sans-serif; white-space:nowrap;";
  }
</script>
`;
    expect(hexColourSites(svelte)).toEqual(["#fff"]);
  });

  it("does not truncate a .style.cssText assignment at a backslash-escaped quote inside the string", () => {
    const svelte = `<script>\n  el.style.cssText = "content: \\"x\\"; color:#fff;";\n</script>\n`;
    expect(hexColourSites(svelte)).toEqual(["#fff"]);
  });

  it("catches a hex literal in a literal style:prop=\"...\" directive attribute", () => {
    const svelte = `<div style:color="#654321" />\n`;
    expect(hexColourSites(svelte)).toEqual(["#654321"]);
  });

  it("does not count a plain (non-.style, non-style=) variable holding a hex default — data the user picks over, not app theming, same shape as ColorRulesDialog.svelte's newColor", () => {
    const svelte = `<script>\n  let newColor = "#e2504b";\n</script>\n<div />\n`;
    expect(hexColourSites(svelte)).toEqual([]);
  });

  it("does not count a hex fallback in a plain attribute binding — same shape as ColorRulesDialog.svelte's rule.color ?? \"#888888\" colour-picker fallback", () => {
    const svelte = `<input type="color" value={rule.color ?? "#888888"} />\n`;
    expect(hexColourSites(svelte)).toEqual([]);
  });
});

// ---------------------------------------------------------------------------------------------
// CPE-1631 PR review round 2: a real bug shipped in that ticket's first revision and went
// undetected by every guard test above (all of which regex-parse app.css's TEXT, so they're blind
// to how a browser actually TOKENIZES it) — a doc comment referenced two `--pal-*` names back to
// back with only a bare "/" between them (no space), e.g. "--pal-hljs-*/--pal-dark-hljs-*", which
// spells out the two-character CSS comment-close sequence (asterisk immediately followed by
// slash) mid-sentence. That silently truncated the surrounding `/* ... */` comment right there,
// and everything parsed after it — hundreds of real rules, including every `.hljs-*` colour rule
// this same ticket added — became unparseable garbage to a real CSS engine. Verified in Chrome:
// `document.styleSheets[0].cssRules.length` read 8 instead of the expected 161. No amount of
// regex-based text checking catches this (the text is perfectly innocent-looking prose); the only
// generic, cheap guard is exactly what this test does: count `/*` vs `*/` occurrences file-wide.
// This can't tell you WHERE a stray one is, but a real mismatch means *something* in the file will
// silently corrupt however a browser parses everything after it — that alone is worth failing CI
// over, rather than relying on a human eyeballing a screenshot that happens to still look plausible
// (exactly what let this ship the first time).
describe("app.css comment-marker balance (CPE-1631)", () => {
  it("every /* has a matching */ (an unbalanced count silently truncates a comment and corrupts everything parsed after it)", () => {
    const opens = (css.match(/\/\*/g) ?? []).length;
    const closes = (css.match(/\*\//g) ?? []).length;
    expect(
      closes,
      `app.css has ${opens} "/*" but ${closes} "*/" — a mismatch means some comment closes early ` +
        `(likely a bare "*/" accidentally spelled out mid-sentence, e.g. two --pal-* names joined by ` +
        `a bare "/" with no space) or never closes, silently corrupting how a real browser parses ` +
        `everything after that point. Search for a non-whitespace character immediately followed by ` +
        `"*/" to find the culprit.`,
    ).toBe(opens);
  });
});
