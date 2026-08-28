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

/**
 * CPE-1979 — `commit`'s "nothing would change, don't re-navigate" short-circuit, pinned in BOTH
 * directions. It compares the typed value against `currentPath`, which is only the thing on screen while
 * no view is layered over that path's own listing; `pathOverlaidByView` is how the parent says otherwise
 * (see the prop's comment and `src/App.archiveNav.test.ts` for the end-to-end case).
 *
 * Both directions are asserted deliberately. Only ever testing the new `true` case would leave the
 * short-circuit itself — the reason the guard exists at all, and a real behaviour (Enter on an unchanged
 * address bar must not re-list the folder) — resting on nothing, so a future "just delete the
 * comparison" would sail through green.
 *
 * Red-proofed by hand before commit, both ways, results recorded here rather than only in the PR body:
 *   - `&& !false` (the pre-CPE-1979 behaviour, `pathOverlaidByView` ignored) — 2 red of 11 across this
 *     file and `src/App.archiveNav.test.ts`: the "dispatches ... when a view is layered over it" case
 *     here, and the end-to-end archive exit there. CPE-1366's Back test stayed GREEN, so the two exits
 *     are covered independently and neither shadows the other.
 *   - `&& !true` (the equality comparison made unreachable) — 1 red of 9 in this file: the "does not
 *     dispatch ... already describes what is on screen" case.
 */
describe("NavToolbar address commit vs. a layered view (CPE-1979)", () => {
  /** Opens the address bar (which seeds the input with `currentPath`) and presses Enter without typing —
   *  exactly the gesture that submits a value equal to `currentPath`. */
  async function pressEnterOnUnchangedAddress(props: Record<string, unknown>) {
    const { component } = render(NavToolbar, {
      props: { crumbs: [{ name: "photos", path: "C:\\d\\photos" }], editingPath: true, ...props },
    });
    const navigate = vi.fn();
    component.$on("navigate", navigate);
    const input = (await screen.findByLabelText("Address")) as HTMLInputElement;
    expect(input.value).toBe(props.currentPath);
    await fireEvent.keyDown(input, { key: "Enter" });
    return navigate;
  }

  it("does not dispatch navigate when the typed path already describes what is on screen", async () => {
    const navigate = await pressEnterOnUnchangedAddress({
      currentPath: "C:\\d\\photos",
      pathOverlaidByView: false,
    });
    expect(navigate).not.toHaveBeenCalled();
  });

  it("dispatches navigate for the same path when a view is layered over it", async () => {
    const navigate = await pressEnterOnUnchangedAddress({
      currentPath: "C:\\d\\photos",
      pathOverlaidByView: true,
    });
    expect(navigate).toHaveBeenCalledTimes(1);
    expect(navigate.mock.calls[0][0].detail).toBe("C:\\d\\photos");
  });

  it("still refuses an empty address, layered view or not", async () => {
    const { component } = render(NavToolbar, {
      props: {
        crumbs: [{ name: "photos", path: "C:\\d\\photos" }],
        currentPath: "C:\\d\\photos",
        editingPath: true,
        pathOverlaidByView: true,
      },
    });
    const navigate = vi.fn();
    component.$on("navigate", navigate);
    const input = (await screen.findByLabelText("Address")) as HTMLInputElement;
    await fireEvent.input(input, { target: { value: "   " } });
    await fireEvent.keyDown(input, { key: "Enter" });
    expect(navigate).not.toHaveBeenCalled();
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
