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
