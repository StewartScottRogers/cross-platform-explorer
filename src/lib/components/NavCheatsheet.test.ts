import { describe, it, expect } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import NavCheatsheet from "./NavCheatsheet.svelte";

describe("NavCheatsheet (CPE-1555)", () => {
  it("renders nothing when open=false", () => {
    render(NavCheatsheet, { open: false });
    expect(screen.queryByTestId("nav-cheatsheet-backdrop")).toBeNull();
    expect(screen.queryByTestId("nav-cheatsheet-rows")).toBeNull();
  });

  it("renders all seven binding rows when open=true", () => {
    render(NavCheatsheet, { open: true });
    const rows = screen.getByTestId("nav-cheatsheet-rows");
    expect(rows.querySelectorAll(".row").length).toBe(7);
    for (const keys of ["h j k l", "gg / G", "v", "d / y / p", "/", ":", "Esc"]) {
      expect(rows.textContent).toContain(keys);
    }
  });

  it("fires close when the close button is clicked", async () => {
    const { component } = render(NavCheatsheet, { open: true });
    let closed = false;
    component.$on("close", () => (closed = true));

    await fireEvent.click(screen.getByTestId("nav-cheatsheet-close"));
    expect(closed).toBe(true);
  });

  it("fires close on Escape", async () => {
    const { component } = render(NavCheatsheet, { open: true });
    let closed = false;
    component.$on("close", () => (closed = true));

    await fireEvent.keyDown(window, { key: "Escape" });
    expect(closed).toBe(true);
  });

  it("fires close on backdrop click but not on a click inside the dialog", async () => {
    const { component } = render(NavCheatsheet, { open: true });
    let closed = false;
    component.$on("close", () => (closed = true));

    await fireEvent.click(screen.getByTestId("nav-cheatsheet-rows"));
    expect(closed).toBe(false);

    await fireEvent.click(screen.getByTestId("nav-cheatsheet-backdrop"));
    expect(closed).toBe(true);
  });
});
