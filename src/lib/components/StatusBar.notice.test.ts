/**
 * CPE-1660 — the status-bar notice `<span>` had no overflow strategy at all: no `max-width`, no
 * `white-space: nowrap`, no ellipsis. A notice longer than the window could hold simply wrapped to a
 * second line, and `.statusbar`'s fixed 26px height visibly grew for the 5s the notice is shown.
 *
 * jsdom cannot lay out CSS (no real text metrics, no reflow), so it cannot see whether the notice
 * genuinely wraps at a given pixel width — that's exactly the class of defect this ticket's own
 * evidence table was gathered against, via a real headless-Chrome iframe harness (see the ticket's
 * work log; note its harness gotcha: `--window-size` under `--headless=new` does NOT set the CSS
 * viewport — it clamps to ~500px and rescales, so a real check must mount in an iframe and confirm the
 * width from inside it, not trust the outer window size). What jsdom-based vitest CAN pin, matching the
 * "parse the raw CSS source" convention `CheckpointDialog.narrowWidth.test.ts` / `src/app.css.test.ts`
 * already use for this exact class of guard, is the *mechanism*: that the specific properties the fix
 * relies on are present in the component's own `<style>` block, so a future edit can't silently drop
 * them without failing a fast, headless test. Combined with jsdom-renderable structural checks (the
 * `title` tooltip, the shared `.notice` class for both variants) below.
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { render, screen } from "@testing-library/svelte";
import StatusBar from "./StatusBar.svelte";

const SRC = join(process.cwd(), "src", "lib", "components", "StatusBar.svelte");
const source = readFileSync(SRC, "utf8");

/** The component's `<style>` block body, isolated from markup/script so selector-text matching below
 *  can't accidentally match something in a comment or the template. */
function styleBlock(): string {
  const m = source.match(/<style>([\s\S]*)<\/style>/);
  if (!m) throw new Error("StatusBar.svelte: no <style> block found");
  return m[1];
}

/** Extracts one top-level CSS rule's declaration body by selector, e.g. `ruleBody(css, ".notice")`.
 *  Simple brace matching — sufficient for this file's flat (non-nested) rule set. */
function ruleBody(css: string, selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const re = new RegExp(`(?:^|\\s|\\})${escaped}\\s*\\{([^}]*)\\}`);
  const m = css.match(re);
  if (!m) throw new Error(`StatusBar.svelte <style>: no rule found for selector "${selector}"`);
  return m[1];
}

describe("CPE-1660 — StatusBar notice overflow CSS", () => {
  it(".notice truncates with an ellipsis instead of wrapping to a second line", () => {
    const notice = ruleBody(styleBlock(), ".notice");
    expect(notice).toMatch(/white-space:\s*nowrap/);
    expect(notice).toMatch(/overflow:\s*hidden/);
    expect(notice).toMatch(/text-overflow:\s*ellipsis/);
  });

  it(".notice opts OUT of the flex default min-width:auto floor so it can actually shrink", () => {
    // Without `min-width: 0`, a flex item's automatic minimum size floors it at its own min-content
    // width, which is exactly what would let a long notice force the row wider / crush its neighbours
    // (git status, free space) instead of truncating.
    const notice = ruleBody(styleBlock(), ".notice");
    expect(notice).toMatch(/min-width:\s*0\b/);
  });

  it(".notice caps how much of the row a single notice may claim, so it cannot squeeze siblings out", () => {
    const notice = ruleBody(styleBlock(), ".notice");
    expect(notice).toMatch(/max-width:/);
  });
});

describe("CPE-1660 — StatusBar notice DOM (jsdom-checkable structure)", () => {
  it("renders the notice with the overflow-strategy class and the full text still reachable via title", () => {
    const longNotice = "A".repeat(200);
    render(StatusBar, { itemCount: 1, totalCount: 1, notice: longNotice });
    const el = screen.getByText(longNotice);
    expect(el.className).toContain("notice");
    expect(el.getAttribute("title")).toBe(longNotice);
  });

  it("the error-styled notice gets the same overflow-strategy class as the normal one", () => {
    const longNotice = "B".repeat(200);
    render(StatusBar, { itemCount: 1, totalCount: 1, notice: longNotice, noticeIsError: true });
    const el = screen.getByText(longNotice);
    expect(el.className).toContain("notice");
    expect(el.className).toContain("error");
    expect(el.getAttribute("title")).toBe(longNotice);
  });
});
