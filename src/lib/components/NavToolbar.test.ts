/**
 * NavToolbar render test — the address-bar recent-folder autocomplete (CPE-361).
 */
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import NavToolbar from "./NavToolbar.svelte";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("NavToolbar folder picker (CPE-366)", () => {
  it("has a Browse-for-a-folder button that dispatches browse", async () => {
    const { component } = render(NavToolbar, {
      props: { crumbs: [{ name: "C:", path: "C:\\" }], currentPath: "C:\\" },
    });
    const browse = vi.fn();
    component.$on("browse", browse);
    await fireEvent.click(screen.getByRole("button", { name: /browse for a folder/i }));
    expect(browse).toHaveBeenCalled();
  });
});

describe("NavToolbar address autocomplete (CPE-361)", () => {
  it("renders recent folder paths as datalist options in edit mode", () => {
    const { container } = render(NavToolbar, {
      props: {
        crumbs: [{ name: "C:", path: "C:\\" }],
        currentPath: "C:\\",
        editingPath: true,
        recentPaths: ["C:\\repos\\app", "C:\\Users\\me\\Downloads"],
      },
    });
    const options = container.querySelectorAll("#recent-paths option");
    expect(options).toHaveLength(2);
    expect((options[0] as HTMLOptionElement).value).toBe("C:\\repos\\app");
  });
});

describe("NavToolbar density (CPE-1528)", () => {
  it("does not apply the compact class when density is comfortable (default)", () => {
    const { container } = render(NavToolbar, {
      props: { crumbs: [{ name: "C:", path: "C:\\" }], currentPath: "C:\\" },
    });
    expect(container.querySelector(".navbar")?.classList.contains("compact")).toBe(false);
  });

  it("applies the compact class to the root .navbar when density is compact", () => {
    const { container } = render(NavToolbar, {
      props: { crumbs: [{ name: "C:", path: "C:\\" }], currentPath: "C:\\", density: "compact" },
    });
    expect(container.querySelector(".navbar")?.classList.contains("compact")).toBe(true);
  });
});

describe("NavToolbar density toggle (CPE-1529)", () => {
  it("is not pressed when density is comfortable (default) and dispatches 'compact' on click", async () => {
    const { component } = render(NavToolbar, {
      props: { crumbs: [{ name: "C:", path: "C:\\" }], currentPath: "C:\\" },
    });
    const toggle = screen.getByRole("button", { name: /switch to compact density/i });
    expect(toggle.getAttribute("aria-pressed")).toBe("false");

    const density = vi.fn();
    component.$on("density", density);
    await fireEvent.click(toggle);

    expect(density).toHaveBeenCalledTimes(1);
    expect(density.mock.calls[0][0].detail).toBe("compact");
  });

  it("is pressed when density is compact and dispatches 'comfortable' on click", async () => {
    const { component } = render(NavToolbar, {
      props: { crumbs: [{ name: "C:", path: "C:\\" }], currentPath: "C:\\", density: "compact" },
    });
    const toggle = screen.getByRole("button", { name: /switch to comfortable density/i });
    expect(toggle.getAttribute("aria-pressed")).toBe("true");

    const density = vi.fn();
    component.$on("density", density);
    await fireEvent.click(toggle);

    expect(density).toHaveBeenCalledTimes(1);
    expect(density.mock.calls[0][0].detail).toBe("comfortable");
  });
});
