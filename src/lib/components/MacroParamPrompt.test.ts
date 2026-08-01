/**
 * MacroParamPrompt (CPE-1190 UI half). Pure props-in/events-out — no backend call of its own — so
 * these are straightforward component-render tests, mirroring PasswordPromptDialog.test.ts.
 */
import { describe, it, expect } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import MacroParamPrompt from "./MacroParamPrompt.svelte";

describe("MacroParamPrompt (CPE-1190 UI half)", () => {
  it("renders one labelled field per requested label", () => {
    render(MacroParamPrompt, { labels: ["prefix", "folder"] });
    expect(screen.getByTestId("param-field-prefix")).toBeTruthy();
    expect(screen.getByTestId("param-field-folder")).toBeTruthy();
    expect(screen.queryByTestId("no-params")).toBeNull();
  });

  it("renders a fallback message when no labels are requested", () => {
    render(MacroParamPrompt, { labels: [] });
    expect(screen.getByTestId("no-params")).toBeTruthy();
  });

  it("clicking Continue dispatches submit with the full values map, including an untouched field as \"\"", async () => {
    const { component } = render(MacroParamPrompt, { labels: ["prefix", "folder"] });
    const submitted: Array<Record<string, string>> = [];
    component.$on("submit", (e: CustomEvent<Record<string, string>>) => submitted.push(e.detail));

    await fireEvent.input(screen.getByTestId("param-field-prefix"), { target: { value: "vacation" } });
    await fireEvent.click(screen.getByTestId("ok-btn"));

    expect(submitted).toEqual([{ prefix: "vacation", folder: "" }]);
  });

  it("pressing Enter in a field submits the current values map", async () => {
    const { component } = render(MacroParamPrompt, { labels: ["label"] });
    const submitted: Array<Record<string, string>> = [];
    component.$on("submit", (e: CustomEvent<Record<string, string>>) => submitted.push(e.detail));

    await fireEvent.input(screen.getByTestId("param-field-label"), { target: { value: "reviewed" } });
    await fireEvent.keyDown(screen.getByTestId("param-field-label"), { key: "Enter" });

    expect(submitted).toEqual([{ label: "reviewed" }]);
  });

  it("clicking Cancel dispatches cancel", async () => {
    const { component } = render(MacroParamPrompt, { labels: ["x"] });
    let cancelled = 0;
    component.$on("cancel", () => cancelled++);

    await fireEvent.click(screen.getByTestId("cancel-btn"));

    expect(cancelled).toBe(1);
  });

  it("pressing Escape dispatches cancel", async () => {
    const { component } = render(MacroParamPrompt, { labels: ["x"] });
    let cancelled = 0;
    component.$on("cancel", () => cancelled++);

    await fireEvent.keyDown(window, { key: "Escape" });

    expect(cancelled).toBe(1);
  });
});
