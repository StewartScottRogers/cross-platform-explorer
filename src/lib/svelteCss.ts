/**
 * Reading declarations back out of a Svelte component's `<style>` block (CPE-1968).
 *
 * WHY THIS EXISTS. CPE-1968 fixed a swallowed click by making a dialog's body a CONTENT-INDEPENDENT
 * height, and that fix lives entirely in CSS — so the guard for it has to read CSS. Two component
 * tests (`OrganizeDialog.test.ts`, `MacrosDialog.test.ts`) need the same three things, and CLAUDE.md's
 * CPE-1950 note is explicit that where duplication is removable the right move is to remove it rather
 * than pin two copies to one oracle. Hence one module, imported by both.
 *
 * CPE-1933 rule 2 — ANCHOR ON CODE, NEVER ON PROSE. `styleBlock` strips CSS comments before it
 * matches. It also refuses to guess: a class with zero or two matching blocks throws rather than
 * picking one.
 *
 * WHAT THE STRIPPER BUYS TODAY: NOTHING, MEASURED (CLAUDE.md — "do not name a backstop without
 * checking it can fire", and re-measure such a note in the commit that ships it). An earlier draft of
 * this paragraph said the stripper was needed *because* the fix it guards ships with a long comment
 * quoting the old broken declarations. That was over-claimed on two counts, both measured on this
 * commit's own tree:
 *
 *   1. With `stripCssComments` returning its input unchanged, `OrganizeDialog.test.ts` and
 *      `MacrosDialog.test.ts` run 31/31 GREEN. The component's comment does quote
 *      `min-height: 120px; max-height: 45vh` — but never in a form matching `.preview {`, which is
 *      what the regex anchors on. Nothing in the tree reaches the stripper today.
 *   2. The failure mode was stated BACKWARDS. Without the stripper, a commented-out WHOLE rule gives
 *      two matches and `styleBlock` THROWS. Measured by pasting a CSS-commented copy of the old
 *      `.preview` rule into the component with the stripper disabled: 3 of 15 red, every one saying
 *      "expected exactly one `.preview { … }` block in the component's <style>, found 2". That is a
 *      loud red, not the silent pass the old sentence implied. Restoring the stripper with the same
 *      decoy still in place: 15/15 green — which is the stripper actually doing its job.
 *
 * So it is kept as CORRECT AND DEFENSIVE, not as a load-bearing backstop: a silent wrong answer needs
 * the live rule DELETED and a commented copy left behind, which no current file does. If a future
 * component ever comments out a rule it also declares, this is what stops the comment answering.
 *
 * WHAT THE COMMENT STRIPPER DOES NOT HANDLE, stated rather than left to be discovered: a `/*`
 * sequence inside a CSS string (`content: "/*"`). No component in this repo has one, and the
 * consequence would be a throw from `styleBlock` (the block would not parse as unique), not a silent
 * wrong answer — it fails toward reporting rather than toward passing.
 */

/** Strip `/* … *\/` comments. See the caveat in this file's header comment. */
export function stripCssComments(css: string): string {
  return css.replace(/\/\*[\s\S]*?\*\//g, "");
}

/** The `<style>` body of a Svelte component, comments stripped. Throws if there is not exactly one. */
export function svelteStyle(componentSource: string): string {
  const blocks = [...componentSource.matchAll(/<style[^>]*>([\s\S]*?)<\/style>/g)];
  if (blocks.length !== 1) {
    throw new Error(`expected exactly one <style> block in the component, found ${blocks.length}`);
  }
  return stripCssComments(blocks[0][1]);
}

/**
 * The one CSS declaration block for `.className` in a Svelte component's `<style>`.
 * Throws rather than guessing when the selector is absent or declared more than once.
 */
export function styleBlock(componentSource: string, className: string): string {
  const css = svelteStyle(componentSource);
  const matches = [...css.matchAll(new RegExp(`(?:^|\\n)\\s*\\.${className}\\s*\\{([^}]*)\\}`, "g"))];
  if (matches.length !== 1) {
    throw new Error(
      `expected exactly one \`.${className} { … }\` block in the component's <style>, found ${matches.length}`,
    );
  }
  return matches[0][1];
}

/**
 * The value of `prop` in a declaration block, or `undefined`.
 *
 * Anchored on a declaration boundary so that asking for `height` does NOT return `min-height`'s or
 * `max-height`'s value — the exact confusion this module's callers are guarding against.
 */
export function declaration(block: string, prop: string): string | undefined {
  const m = new RegExp(`(?:^|;|\\{)\\s*${prop}\\s*:\\s*([^;]+)`, "i").exec(block);
  return m ? m[1].trim() : undefined;
}

/**
 * Resolve a CSS length to px against a viewport height. Handles `Npx`, `Nvh`, and `clamp()` / `min()`
 * / `max()` over those. THROWS on anything else — an unresolvable length must be a loud failure, not
 * a guessed number, because every caller uses the result to decide whether a box moves.
 */
export function lengthToPx(value: string, viewportHeightPx: number): number {
  const v = value.trim();

  const fn = /^(clamp|min|max)\(([\s\S]*)\)$/i.exec(v);
  if (fn) {
    const args = splitTopLevel(fn[2]).map((a) => lengthToPx(a, viewportHeightPx));
    const name = fn[1].toLowerCase();
    if (name === "min") return Math.min(...args);
    if (name === "max") return Math.max(...args);
    if (args.length !== 3) throw new Error(`clamp() needs 3 arguments, got ${args.length} in "${value}"`);
    return Math.min(Math.max(args[0], args[1]), args[2]);
  }

  const px = /^(-?[\d.]+)px$/i.exec(v);
  if (px) return parseFloat(px[1]);

  const vh = /^(-?[\d.]+)vh$/i.exec(v);
  if (vh) return (parseFloat(vh[1]) / 100) * viewportHeightPx;

  throw new Error(`cannot resolve CSS length "${value}" to px (supported: px, vh, clamp/min/max of those)`);
}

/** Split `a, b, c` on top-level commas only, so nested `min(1px, 2px)` survives. */
function splitTopLevel(args: string): string[] {
  const out: string[] = [];
  let depth = 0;
  let cur = "";
  for (const ch of args) {
    if (ch === "(") depth++;
    else if (ch === ")") depth--;
    if (ch === "," && depth === 0) {
      out.push(cur);
      cur = "";
    } else cur += ch;
  }
  if (cur.trim()) out.push(cur);
  return out;
}

/**
 * Does this block give its box a height that CANNOT depend on its content? Returns `null` when it
 * does, or a human-readable reason when it does not.
 *
 * This is CPE-1968's invariant in one place. Depending on the VIEWPORT is fine — the viewport does
 * not change while an async load is in flight — so `vh` and `clamp()` over `vh` pass. What fails is
 * anything content-driven: no definite `height` at all (the box sizes to its contents), a `%` height
 * (its parent here is auto-height, so a percentage resolves against content), and a
 * `min-height`/`max-height` PAIR that disagree, which is precisely the shape that let
 * `OrganizeDialog`'s `.preview` grow ~195px under the pointer.
 */
export function contentIndependentHeightReason(block: string, viewportHeightPx: number): string | null {
  const height = declaration(block, "height");
  const min = declaration(block, "min-height");
  const max = declaration(block, "max-height");

  if (!height) {
    return (
      "declares no `height`, so the box sizes to its content" +
      (min || max ? ` (a min-height/max-height pair is content-driven by definition: min=${min}, max=${max})` : "")
    );
  }
  if (/auto/i.test(height)) return `declares \`height: ${height}\`, which is content-driven`;
  if (/%/.test(height)) {
    return `declares \`height: ${height}\` — a percentage resolves against an auto-height parent here, so it is content-driven`;
  }

  const resolved = lengthToPx(height, viewportHeightPx);
  for (const [name, raw] of [
    ["min-height", min],
    ["max-height", max],
  ] as const) {
    if (raw !== undefined && lengthToPx(raw, viewportHeightPx) !== resolved) {
      return `declares \`height: ${height}\` (${resolved}px) but also \`${name}: ${raw}\`, which overrides it and reintroduces a content-driven height`;
    }
  }
  return null;
}
