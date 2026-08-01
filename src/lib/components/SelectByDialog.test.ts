/**
 * CPE-1229 (epic CPE-978): "Select by…" is the only place this app builds a structured `Condition`, so
 * it doubles as the "Save search…" affordance — capture the same condition as a named `SavedSearch`
 * instead of (or as well as) applying it to the current selection. These are component-render tests, no
 * Tauri mock needed (the dialog dispatches events; the caller owns the store write + the actual select).
 */
import { describe, it, expect } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import SelectByDialog from "./SelectByDialog.svelte";
import type { Condition } from "../colorRules";

describe("SelectByDialog — existing Select flow still works (CPE-782 regression)", () => {
  it("dispatches submit with the built condition", async () => {
    const { component } = render(SelectByDialog);
    const submitted: Condition[] = [];
    component.$on("submit", (e) => submitted.push(e.detail));

    await fireEvent.input(screen.getByLabelText("Extensions"), { target: { value: "png, jpg" } });
    await fireEvent.click(screen.getByTestId("select-btn"));

    expect(submitted).toEqual([{ kind: "ext", exts: ["png", "jpg"] }]);
  });
});

describe("SelectByDialog — Save search… (CPE-1229)", () => {
  it("first click reveals the name field instead of dispatching anything", async () => {
    const { component } = render(SelectByDialog);
    const saved: unknown[] = [];
    component.$on("save", (e) => saved.push(e.detail));

    await fireEvent.input(screen.getByLabelText("Extensions"), { target: { value: "png" } });
    await fireEvent.click(screen.getByTestId("save-search-reveal"));

    expect(screen.getByLabelText("Search name")).toBeTruthy();
    expect(saved).toHaveLength(0);
  });

  it("naming it and confirming dispatches save with the name + the built condition", async () => {
    const { component } = render(SelectByDialog);
    const saved: { name: string; condition: Condition }[] = [];
    component.$on("save", (e) => saved.push(e.detail));

    await fireEvent.input(screen.getByLabelText("Extensions"), { target: { value: "png" } });
    await fireEvent.click(screen.getByTestId("save-search-reveal"));
    await fireEvent.input(screen.getByLabelText("Search name"), { target: { value: "Big PNGs" } });
    await fireEvent.click(screen.getByTestId("save-search-confirm"));

    expect(saved).toEqual([{ name: "Big PNGs", condition: { kind: "ext", exts: ["png"] } }]);
  });

  it("Enter in the name field also confirms the save", async () => {
    const { component } = render(SelectByDialog);
    const saved: { name: string; condition: Condition }[] = [];
    component.$on("save", (e) => saved.push(e.detail));

    await fireEvent.input(screen.getByLabelText("Extensions"), { target: { value: "log" } });
    await fireEvent.click(screen.getByTestId("save-search-reveal"));
    const nameField = screen.getByLabelText("Search name");
    await fireEvent.input(nameField, { target: { value: "Logs" } });
    await fireEvent.keyDown(nameField, { key: "Enter" });

    expect(saved).toEqual([{ name: "Logs", condition: { kind: "ext", exts: ["log"] } }]);
  });

  it("is a no-op with a blank name or an incomplete condition", async () => {
    const { component } = render(SelectByDialog);
    const saved: unknown[] = [];
    component.$on("save", (e) => saved.push(e.detail));

    // No extensions typed yet ⇒ buildCondition() is null, even with a name.
    await fireEvent.click(screen.getByTestId("save-search-reveal"));
    await fireEvent.input(screen.getByLabelText("Search name"), { target: { value: "Nameless" } });
    await fireEvent.click(screen.getByTestId("save-search-confirm"));
    expect(saved).toHaveLength(0);

    // A condition but a blank name.
    await fireEvent.input(screen.getByLabelText("Extensions"), { target: { value: "png" } });
    await fireEvent.input(screen.getByLabelText("Search name"), { target: { value: "   " } });
    await fireEvent.click(screen.getByTestId("save-search-confirm"));
    expect(saved).toHaveLength(0);
  });

  it("autoReveal opens straight into the name field (command-palette 'Save search…' entry)", () => {
    render(SelectByDialog, { autoReveal: true });
    expect(screen.getByLabelText("Search name")).toBeTruthy();
    expect(screen.queryByTestId("save-search-reveal")).toBeNull();
  });
});
