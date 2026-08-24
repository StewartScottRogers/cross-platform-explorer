/**
 * CPE-1833 — the status bar's two advisory notes (`.filtered-hidden`, `.unreadable`) were never
 * announced to a screen reader, and their full text was reachable only by hovering the `title`
 * attribute.
 *
 * Two distinct defects, two distinct fixes here:
 *
 * 1. **Never announced.** Each note is `{#if count > 0}`-conditionally mounted, i.e. it appears as a
 *    BRAND NEW element already holding its final text. That shape is exactly what Chromium+Windows AT
 *    (WebView2 with NVDA/Narrator — this app) routinely fails to announce, even with `role="status"` on
 *    the span itself — the same lesson CPE-1816 recorded the same day: a live region must already exist
 *    in the accessibility tree BEFORE its content changes. The fix is a SEPARATE, always-mounted
 *    announcer (`.advisory-live`) whose text content updates in place; it is never conditionally
 *    rendered. `aria-atomic="true"` makes a simultaneous change to both notes read as one coherent
 *    sentence rather than two competing ones.
 *
 * 2. **Full text unreachable without a mouse.** `title` is hover-only. `tabindex="0"` on each visible
 *    pill makes it a real Tab stop; a `:focus-visible` CSS rule (pinned by source below, same as
 *    `StatusBar.notice.test.ts`'s convention — jsdom cannot lay out CSS, so the rule's PRESENCE is what
 *    a fast test can pin) reveals the untruncated text on focus for a sighted keyboard user with no
 *    mouse to hover with. The element's OWN text content was always the full sentence — CSS
 *    `text-overflow: ellipsis` only clips what is PAINTED, never the DOM text — so a screen reader's
 *    virtual cursor already read it; `tabindex="0"` is what makes it reachable by literal Tab key.
 *
 * The critical test below (`describe("the announcer persists...")`) is the one that actually PROVES the
 * mechanism, not just the markup: it captures the live-region DOM node BEFORE a note appears, then
 * proves the SAME node (not a new one) carries the text afterward. Reverting to the naive
 * `{#if count > 0}<span role="status">...</span>{/if}` shape — which looks correct by every other
 * measure — fails this test immediately, because the node would not exist at initial mount at all.
 *
 * Real screen-reader verification (NVDA/Narrator against the installed build) is out of scope for this
 * environment (no `tauri-driver`/AT harness available here) — flagged in the ticket's own Notes as a
 * candidate for the QA Architect's manual-test burndown.
 */
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { render, screen } from "@testing-library/svelte";
import { tick } from "svelte";
import StatusBar from "./StatusBar.svelte";

const SRC = join(process.cwd(), "src", "lib", "components", "StatusBar.svelte");
const source = readFileSync(SRC, "utf8");

function styleBlock(): string {
  const m = source.match(/<style>([\s\S]*)<\/style>/);
  if (!m) throw new Error("StatusBar.svelte: no <style> block found");
  return m[1];
}

describe("CPE-1833 — the announcer persists across the change (the actual mechanism, not just markup)", () => {
  it("the live region exists BEFORE any note is present — not conditionally mounted", () => {
    render(StatusBar, { itemCount: 5, totalCount: 5, filteredHidden: 0, unreadableCount: 0 });
    // getByRole throws if absent, which is exactly the failure mode this proves against: a
    // `{#if}`-gated live region (the "naive fix" the ticket calls out) would not exist here at all.
    const region = screen.getByRole("status");
    expect(region).toBeTruthy();
    expect(region.getAttribute("aria-live")).toBe("polite");
    expect(region.textContent?.trim()).toBe("");
  });

  it("RED-PROOF: the SAME node updates its text in place when a note appears — proves it is not removed and re-inserted", async () => {
    const { component } = render(StatusBar, {
      itemCount: 5,
      totalCount: 5,
      filteredHidden: 0,
      unreadableCount: 0,
    });
    const before = screen.getByRole("status");
    expect(before.textContent?.trim()).toBe("");

    await component.$set({ filteredHidden: 3 });
    await tick();

    const after = screen.getByRole("status");
    // Node IDENTITY, not just content — this is what a persistent live region means. If the
    // implementation regressed to `{#if filteredHidden > 0}<div role="status">...</div>{/if}`, this
    // assertion fails because `after` would be a DIFFERENT element than `before` (a new one, already
    // holding its final text) — exactly the shape screen readers routinely fail to announce.
    expect(after).toBe(before);
    expect(after.textContent).toContain(
      "3 entries were hidden because their names could not be shown safely",
    );
  });

  it("clears back to empty text (never removed) once both notes clear", async () => {
    const { component } = render(StatusBar, {
      itemCount: 5,
      totalCount: 5,
      filteredHidden: 2,
      unreadableCount: 1,
    });
    const region = screen.getByRole("status");
    expect(region.textContent?.trim().length).toBeGreaterThan(0);

    await component.$set({ filteredHidden: 0, unreadableCount: 0 });
    await tick();

    // Still the SAME node, present, just empty — not torn down.
    expect(screen.getByRole("status")).toBe(region);
    expect(screen.getByRole("status").textContent?.trim()).toBe("");
  });
});

describe("CPE-1833 — both notes changing at once announce as ONE coherent sentence, not two competing ones", () => {
  it("combines both notes' text in the single live region, marked aria-atomic", async () => {
    const { component } = render(StatusBar, {
      itemCount: 5,
      totalCount: 5,
      filteredHidden: 0,
      unreadableCount: 0,
    });
    const region = screen.getByRole("status");
    expect(region.getAttribute("aria-atomic")).toBe("true");

    await component.$set({ filteredHidden: 2, unreadableCount: 3 });
    await tick();

    const after = screen.getByRole("status");
    expect(after).toBe(region); // still the same persistent node
    expect(after.textContent).toContain(
      "2 entries were hidden because their names could not be shown safely",
    );
    expect(after.textContent).toContain("Couldn't read 3 entries");
  });

  it("only filteredHidden's sentence is present when unreadableCount is 0", () => {
    render(StatusBar, { itemCount: 5, totalCount: 5, filteredHidden: 4, unreadableCount: 0 });
    const region = screen.getByRole("status");
    expect(region.textContent).toContain("4 entries were hidden");
    expect(region.textContent).not.toContain("Couldn't read");
  });
});

describe("CPE-1833 — the visible pills are reachable without a mouse", () => {
  it("filteredHidden pill is a real Tab stop (tabindex=0), full sentence still in its own text content", () => {
    render(StatusBar, { itemCount: 5, totalCount: 5, filteredHidden: 3 });
    // Scoped to the VISIBLE pill: the same sentence is also present, verbatim, in the persistent
    // `.sr-only` announcer proven elsewhere in this file, so an unscoped query is ambiguous.
    const pill = screen.getByText("3 entries were hidden because their names could not be shown safely", {
      selector: ".filtered-hidden",
    });
    expect(pill.getAttribute("tabindex")).toBe("0");
    // The DOM text is already the FULL sentence — CSS ellipsis only clips what's painted, never the
    // underlying text — so no separate aria-label is required for the accessible name.
    expect(pill.textContent?.trim()).toBe(
      "3 entries were hidden because their names could not be shown safely",
    );
  });

  it("unreadable pill is a real Tab stop (tabindex=0)", () => {
    render(StatusBar, { itemCount: 5, totalCount: 5, unreadableCount: 2 });
    const pill = screen.getByText("Couldn't read 2 entries", { selector: ".unreadable" });
    expect(pill.getAttribute("tabindex")).toBe("0");
  });
});

describe("CPE-1833 — CSS mechanism pinned by source (jsdom cannot lay out CSS; see StatusBar.notice.test.ts's convention)", () => {
  it("a :focus-visible rule reveals the full text on focus for `.filtered-hidden` / `.unreadable`", () => {
    const css = styleBlock();
    expect(css).toMatch(/\.filtered-hidden:focus-visible[\s\S]{0,400}?white-space:\s*normal/);
    expect(css).toMatch(/overflow:\s*visible/);
  });

  it("the live-region announcer uses the clip (not display:none) visually-hidden technique, which stays in the accessibility tree", () => {
    const css = styleBlock();
    expect(css).toMatch(/\.sr-only\s*\{[^}]*clip:\s*rect\(0,\s*0,\s*0,\s*0\)/);
    // display:none / visibility:hidden would remove the node from the accessibility tree in most
    // browsers, defeating the entire fix — assert neither is present in the sr-only rule body.
    const m = css.match(/\.sr-only\s*\{([^}]*)\}/);
    expect(m).toBeTruthy();
    expect(m![1]).not.toMatch(/display:\s*none/);
    expect(m![1]).not.toMatch(/visibility:\s*hidden/);
  });
});
