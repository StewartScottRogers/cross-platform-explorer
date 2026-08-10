/**
 * TabBar render tests — compact chrome density (CPE-1528, epic CPE-1488).
 */
import { describe, it, expect } from "vitest";
import { render } from "@testing-library/svelte";
import TabBar from "./TabBar.svelte";

const tabs = [{ id: 1, title: "Home" }];

describe("TabBar density (CPE-1528)", () => {
  it("does not apply the compact class when density is comfortable (default)", () => {
    const { container } = render(TabBar, { tabs, activeId: 1 });
    expect(container.querySelector(".tabbar")?.classList.contains("compact")).toBe(false);
  });

  it("applies the compact class to the root .tabbar when density is compact", () => {
    const { container } = render(TabBar, { tabs, activeId: 1, density: "compact" });
    expect(container.querySelector(".tabbar")?.classList.contains("compact")).toBe(true);
  });
});
