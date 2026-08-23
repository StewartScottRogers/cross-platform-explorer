/**
 * CPE-1859 — `.disk` (the free-space readout) had no right-anchor of its own: only `margin-left: 12px`.
 * It sat at the right edge purely because `.git` precedes it carrying the row's one `margin-left: auto`,
 * so in any folder WITHOUT a git chip the free-space text rendered next to the item count instead.
 *
 * ## What this file is, and what it is NOT
 *
 * **It pins the RULE'S PRESENCE, not the rule's EFFECT.** This project's vitest config runs jsdom,
 * which has no layout engine at all: `getBoundingClientRect` returns zeros and `getComputedStyle`
 * reports nothing from a Svelte component's scoped `<style>` block. No test in this file — and no test
 * that could be written under this config — can observe WHERE in the row an element lands. That blind
 * spot is exactly why the defect survived from CPE-403 (2026-07) until CPE-1854 went looking.
 *
 * The actual verification is a real browser render: `scripts/dev-harness/statusbar-notice` mounts the
 * real StatusBar with the real `src/app.css`, driven by headless Chrome, and reports the measured
 * `getBoundingClientRect` of `.item-count` / `.git` / `.disk` against the bar's right padding edge.
 * At w=900 with the chip absent, `.disk` measured `right=216.0` against a content edge of `886.0`
 * (670.0px adrift) before the fix and `right=886.0` (0.0px) after it. See the ticket's work log.
 *
 * So the value of the checks below is narrow but real: they are the only thing in CI that fails if a
 * future edit deletes either half of a two-rule mechanism whose halves look individually redundant.
 * Treat a green run here as "the declarations are still written down", never as "the bar still lays
 * out correctly".
 *
 * Follows the raw-CSS-source convention already used by `StatusBar.notice.test.ts`,
 * `CheckpointDialog.narrowWidth.test.ts` and `src/app.css.test.ts`.
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const SRC = join(process.cwd(), "src", "lib", "components", "StatusBar.svelte");
const source = readFileSync(SRC, "utf8");

/** The component's `<style>` block body, isolated from markup/script so selector matching below can't
 *  accidentally match text in the template or in a script comment. */
function styleBlock(): string {
  const m = source.match(/<style>([\s\S]*)<\/style>/);
  if (!m) throw new Error("StatusBar.svelte: no <style> block found");
  return m[1];
}

/** Every top-level rule in the block, as `{ selector, body }` with comments stripped first.
 *
 *  Deliberately NOT the looser `ruleBody(css, sel)` helper in `StatusBar.notice.test.ts`: that one
 *  allows any whitespace before the selector, so asking it for `.disk` would also match the tail of
 *  `.git ~ .disk`. Since this file's whole subject is the DIFFERENCE between those two rules, the
 *  selector must be compared whole. */
function rules(): Array<{ selector: string; body: string }> {
  const css = styleBlock().replace(/\/\*[\s\S]*?\*\//g, "");
  const out: Array<{ selector: string; body: string }> = [];
  const re = /([^{}]+)\{([^{}]*)\}/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(css)) !== null) {
    out.push({ selector: m[1].trim().replace(/\s+/g, " "), body: m[2] });
  }
  return out;
}

function bodyOf(selector: string): string {
  const hit = rules().find((r) => r.selector === selector);
  if (!hit) throw new Error(`StatusBar.svelte <style>: no rule with selector \`${selector}\``);
  return hit.body;
}

describe("CPE-1859 — the status bar's right-hand cluster anchors itself", () => {
  it("`.disk` carries its own `margin-left: auto` (it must not depend on `.git` being rendered)", () => {
    // The whole defect in one assertion. `{#if diskLabel}` and `{#if git && git.is_repo}` are
    // independent gates, so `.disk` is routinely on screen with no `.git` before it — that is the
    // steady state of every non-repo folder, not merely a sub-second race while the two fetches land.
    expect(bodyOf(".disk")).toMatch(/margin-left:\s*auto\s*;/);
  });

  it("`.git ~ .disk` restores the plain 12px separator when the chip IS present", () => {
    // Not cosmetic bookkeeping — load-bearing. Flexbox distributes positive free space EQUALLY among
    // all main-axis auto margins, so leaving `.disk` with an auto margin while `.git` also has one
    // stops the chip anchoring and parks it mid-row (measured in real Chrome: `.git` moved from
    // left=637.3 to left=361.1 at w=900). Deleting this rule is therefore a regression in the COMMON
    // case, which is precisely the kind of edit that reads as harmless cleanup.
    expect(bodyOf(".git ~ .disk")).toMatch(/margin-left:\s*12px\s*;/);
  });

  it("`.git` keeps the `margin-left: auto` that anchors the cluster when the chip is present", () => {
    expect(bodyOf(".git")).toMatch(/margin-left:\s*auto\s*;/);
  });

  it("no OTHER rule in the block introduces a third main-axis auto margin", () => {
    // A third `margin-left: auto` anywhere in this row would re-split the free space and un-anchor
    // both readouts again. `.git` and `.git ~ .disk`'s base rule `.disk` are the only two allowed.
    const owners = rules()
      .filter((r) => /margin-left:\s*auto/.test(r.body))
      .map((r) => r.selector)
      .sort();
    expect(owners).toEqual([".disk", ".git"]);
  });

  it("the markup gates `.disk` and `.git` independently — the reason the anchor cannot be shared", () => {
    // Pins the PREMISE rather than the fix: if either readout ever became unconditional, or the two
    // were merged behind one gate, the two-rule mechanism above would be over-engineering and should
    // be revisited rather than left in place uncomprehended.
    expect(source).toMatch(/\{#if git && git\.is_repo\}/);
    expect(source).toMatch(/\{#if diskLabel\}/);
  });
});
