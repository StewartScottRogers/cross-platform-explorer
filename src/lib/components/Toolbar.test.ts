/**
 * Toolbar component (CPE-226): the first button is a Settings gear that toggles
 * a scoped settings popover.
 */
import { describe, it, expect } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import Toolbar from "./Toolbar.svelte";

describe("Toolbar", () => {
  it("renders a labelled Settings gear as its first button", () => {
    render(Toolbar, { props: { label: "Preview" } });
    const gear = screen.getByRole("button", { name: "Preview settings" });
    expect(gear).toBeTruthy();
    expect(gear.getAttribute("aria-haspopup")).toBe("dialog");
    expect(gear.getAttribute("aria-expanded")).toBe("false");
  });

  it("opens and closes the scoped popover when the gear is clicked", async () => {
    render(Toolbar, { props: { label: "Navigation" } });
    const gear = screen.getByRole("button", { name: "Navigation settings" });

    expect(screen.queryByRole("dialog")).toBeNull();
    await fireEvent.click(gear);
    const dialog = screen.getByRole("dialog", { name: "Navigation settings" });
    expect(dialog).toBeTruthy();
    expect(gear.getAttribute("aria-expanded")).toBe("true");

    await fireEvent.click(gear);
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});
