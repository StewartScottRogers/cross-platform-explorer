/**
 * CPE-1712 review round 2 — coverage regression guard.
 *
 * Round 1 missed the tab strip entirely: a spoofed folder name opened in a tab rendered raw, both in
 * the visible label AND the hover `title` attribute. Every assertion here is on `container`/`screen`
 * text or a DOM attribute — what the user's eyes (or a screen reader) actually see.
 */
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/svelte";
import TabBar from "./TabBar.svelte";

// Built from a decimal code point, not a literal character — see filename.ts's own doc comment for why.
const RLO = String.fromCharCode(0x202e);

describe("TabBar — tab label AND title attribute (CPE-1712 round 2 blocker)", () => {
  it("escapes both the visible label and the title attribute for a spoofed tab", () => {
    const tabs = [{ id: 1, title: `${RLO}gnp.txt` }];
    const { container } = render(TabBar, { tabs, activeId: 1 });

    expect(screen.getByText("[RLO]gnp.txt")).toBeTruthy();
    const tabButton = container.querySelector(".tab");
    expect(tabButton?.getAttribute("title")).toBe("[RLO]gnp.txt");
    expect(tabButton?.getAttribute("title")).not.toContain(RLO);
    expect(container.textContent).not.toContain("txt.png");
  });

  it("still shows an ordinary tab title, and a real Arabic one, untouched", () => {
    const tabs = [{ id: 1, title: "Downloads" }, { id: 2, title: "مستندات" }];
    const { container } = render(TabBar, { tabs, activeId: 1 });
    expect(container.textContent).toContain("Downloads");
    expect(container.textContent).toContain("مستندات");
  });
});
