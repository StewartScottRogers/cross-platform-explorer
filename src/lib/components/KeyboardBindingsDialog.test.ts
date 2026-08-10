/**
 * Component test for the read-only keyboard shortcuts viewer (CPE-1548, epic CPE-1484). Renders
 * every `ACTIONS` entry grouped by category with its currently effective chord (via `chordFor` +
 * `formatChord`), including the "Unbound" case, and asserts the filter narrows the visible rows by
 * both description and group. Close-on-Escape and close-on-backdrop-click mirror
 * `ShortcutsDialog.svelte`'s existing pattern.
 */
import { describe, it, expect } from "vitest";
import { render, screen, fireEvent, within } from "@testing-library/svelte";
import KeyboardBindingsDialog from "./KeyboardBindingsDialog.svelte";
import { ACTIONS, defaultKeymap, setChord, type Keymap } from "../keymap";

describe("KeyboardBindingsDialog renders every action + its chord (CPE-1548)", () => {
  it("lists every ACTIONS entry, grouped by category", async () => {
    const keymap = defaultKeymap();
    render(KeyboardBindingsDialog, { keymap });

    const groups = screen.getByTestId("keyboard-bindings-groups");
    for (const action of ACTIONS) {
      expect(within(groups).getByText(action.description)).toBeTruthy();
    }
    // Spot-check a couple of group headings appear.
    expect(within(groups).getByText("Navigation")).toBeTruthy();
    expect(within(groups).getByText("Tabs")).toBeTruthy();
  });

  it("shows the effective (overridden) chord, not the built-in default, when the keymap has an override", async () => {
    const keymap = setChord(defaultKeymap(), "newTab", "Ctrl+Alt+N");
    render(KeyboardBindingsDialog, { keymap });
    const row = screen.getByText("New tab").closest(".row") as HTMLElement;
    expect(within(row).getByText("Ctrl+Alt+N")).toBeTruthy();
  });

  it("renders a friendly display form for a substituted key (arrow glyph)", async () => {
    const keymap = defaultKeymap();
    render(KeyboardBindingsDialog, { keymap });
    const row = screen.getByText("Back").closest(".row") as HTMLElement;
    expect(within(row).getByText("Alt+←")).toBeTruthy();
  });

  it("shows 'Unbound' for an action whose chord is empty", async () => {
    const keymap: Keymap = { ...defaultKeymap(), refresh: "" };
    render(KeyboardBindingsDialog, { keymap });
    const row = screen.getByText("Refresh").closest(".row") as HTMLElement;
    expect(within(row).getByText("Unbound")).toBeTruthy();
  });
});

describe("KeyboardBindingsDialog filter (CPE-1548)", () => {
  it("narrows the visible rows by action description, case-insensitively", async () => {
    render(KeyboardBindingsDialog, { keymap: defaultKeymap() });
    await fireEvent.input(screen.getByTestId("keyboard-bindings-filter"), { target: { value: "rename" } });

    expect(screen.getByText("Rename")).toBeTruthy();
    expect(screen.queryByText("Copy")).toBeNull();
    expect(screen.queryByText("New tab")).toBeNull();
  });

  it("narrows the visible rows by group name", async () => {
    render(KeyboardBindingsDialog, { keymap: defaultKeymap() });
    await fireEvent.input(screen.getByTestId("keyboard-bindings-filter"), { target: { value: "tabs" } });

    expect(screen.getByText("New tab")).toBeTruthy();
    expect(screen.getByText("Close tab")).toBeTruthy();
    expect(screen.queryByText("Rename")).toBeNull();
  });

  it("shows an empty state when nothing matches", async () => {
    render(KeyboardBindingsDialog, { keymap: defaultKeymap() });
    await fireEvent.input(screen.getByTestId("keyboard-bindings-filter"), { target: { value: "zzzznomatch" } });

    expect(screen.getByText(/No shortcuts match/)).toBeTruthy();
  });
});

describe("KeyboardBindingsDialog close behavior (CPE-1548)", () => {
  it("dispatches close on Escape", async () => {
    const { component } = render(KeyboardBindingsDialog, { keymap: defaultKeymap() });
    let closed = false;
    component.$on("close", () => (closed = true));

    await fireEvent.keyDown(window, { key: "Escape" });
    expect(closed).toBe(true);
  });

  it("dispatches close on backdrop click but not on a click inside the dialog", async () => {
    const { component, container } = render(KeyboardBindingsDialog, { keymap: defaultKeymap() });
    let closed = false;
    component.$on("close", () => (closed = true));

    const dialog = container.querySelector(".dialog") as HTMLElement;
    await fireEvent.click(dialog);
    expect(closed).toBe(false);

    const backdrop = container.querySelector(".backdrop") as HTMLElement;
    await fireEvent.click(backdrop);
    expect(closed).toBe(true);
  });
});
