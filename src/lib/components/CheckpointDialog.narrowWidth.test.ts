/**
 * CPE-1635 — structural CSS guard for CheckpointDialog's header row at narrow widths.
 *
 * jsdom cannot lay out CSS, so it cannot see whether `.head-row`/`.dialog` genuinely overflow a real
 * viewport — that's exactly why this defect could exist unnoticed (see the ticket's work log for the
 * real-Chrome verification that actually confirms/refutes layout). What jsdom-based vitest CAN pin is
 * the *mechanism*: that the specific properties this fix relies on are present in the component's own
 * `<style>` block, so a future edit can't silently drop them without failing a fast, headless test —
 * the same "parse the raw CSS source" convention `src/app.css.test.ts` already uses for its own guards.
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const SRC = join(process.cwd(), "src", "lib", "components", "CheckpointDialog.svelte");
const source = readFileSync(SRC, "utf8");

/** The component's `<style>` block body, isolated from markup/script so selector-text matching below
 *  can't accidentally match something in a comment or the template. */
function styleBlock(): string {
  const m = source.match(/<style>([\s\S]*)<\/style>/);
  if (!m) throw new Error("CheckpointDialog.svelte: no <style> block found");
  return m[1];
}

/** Extracts one top-level CSS rule's declaration body by selector, e.g. `ruleBody(css, "h2")`. Simple
 *  brace matching — sufficient for this file's flat (non-nested) rule set. */
function ruleBody(css: string, selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const re = new RegExp(`(?:^|\\s|\\})${escaped}\\s*\\{([^}]*)\\}`);
  const m = css.match(re);
  if (!m) throw new Error(`CheckpointDialog.svelte <style>: no rule found for selector "${selector}"`);
  return m[1];
}

describe("CPE-1635 — CheckpointDialog header-row narrow-width CSS", () => {
  it("h2 (the .head-row title) opts OUT of the flex default min-width:auto floor", () => {
    // Without `min-width: 0`, a flex item's automatic minimum size floors it at its own min-content
    // width — the exact "usual culprit" CPE-1635 named for a header row that won't shrink/wrap sanely.
    const h2 = ruleBody(styleBlock(), "h2");
    expect(h2).toMatch(/min-width:\s*0\b/);
  });

  it("h2 truncates with an ellipsis instead of wrapping to multiple lines or overflowing", () => {
    const h2 = ruleBody(styleBlock(), "h2");
    expect(h2).toMatch(/white-space:\s*nowrap/);
    expect(h2).toMatch(/overflow:\s*hidden/);
    expect(h2).toMatch(/text-overflow:\s*ellipsis/);
  });

  it("the .docs (help/book) icon button stays fixed-size — flex-shrink:0 — while h2 gives way", () => {
    // The inverse guarantee: h2 is the sibling that should shrink; this button must not also shrink and
    // distort its 26x26 SVG icon once the row is squeezed enough for the ellipsis above to engage.
    const docs = ruleBody(styleBlock(), ".docs");
    expect(docs).toMatch(/flex-shrink:\s*0\b/);
  });

  it(".head-row itself stays a non-wrapping flex row (space-between) — the title shrinks, not the row layout", () => {
    const headRow = ruleBody(styleBlock(), ".head-row");
    expect(headRow).toMatch(/display:\s*flex/);
    expect(headRow).toMatch(/justify-content:\s*space-between/);
  });
});
