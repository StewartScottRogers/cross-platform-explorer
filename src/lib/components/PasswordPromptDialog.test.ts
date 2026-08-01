/**
 * PasswordPromptDialog (CPE-1179, epic CPE-705). Shared masked-input modal — the owner of "type a
 * password"; CPE-1182 will wire it up for encrypted-archive extract/create. Pure props-in/events-out
 * (no backend call of its own), so these are straightforward component-render tests, no Tauri mock
 * needed.
 */
import { describe, it, expect } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import PasswordPromptDialog from "./PasswordPromptDialog.svelte";

describe("PasswordPromptDialog (CPE-1179)", () => {
  it("renders a masked password field with Cancel/OK actions", () => {
    render(PasswordPromptDialog);
    const field = screen.getByTestId("password-field") as HTMLInputElement;
    expect(field.type).toBe("password");
    expect(screen.getByTestId("cancel-btn")).toBeTruthy();
    expect(screen.getByTestId("ok-btn")).toBeTruthy();
    expect(screen.queryByTestId("password-error")).toBeNull();
  });

  it("typing then clicking OK dispatches submit with the typed value", async () => {
    const { component } = render(PasswordPromptDialog);
    const submitted: string[] = [];
    component.$on("submit", (e: CustomEvent<string>) => submitted.push(e.detail));

    await fireEvent.input(screen.getByTestId("password-field"), { target: { value: "hunter2" } });
    await fireEvent.click(screen.getByTestId("ok-btn"));

    expect(submitted).toEqual(["hunter2"]);
  });

  it("pressing Enter in the field submits the typed value", async () => {
    const { component } = render(PasswordPromptDialog);
    const submitted: string[] = [];
    component.$on("submit", (e: CustomEvent<string>) => submitted.push(e.detail));

    await fireEvent.input(screen.getByTestId("password-field"), { target: { value: "swordfish" } });
    await fireEvent.keyDown(screen.getByTestId("password-field"), { key: "Enter" });

    expect(submitted).toEqual(["swordfish"]);
  });

  it("clicking Cancel dispatches cancel", async () => {
    const { component } = render(PasswordPromptDialog);
    let cancelled = 0;
    component.$on("cancel", () => cancelled++);

    await fireEvent.click(screen.getByTestId("cancel-btn"));

    expect(cancelled).toBe(1);
  });

  it("pressing Escape dispatches cancel", async () => {
    const { component } = render(PasswordPromptDialog);
    let cancelled = 0;
    component.$on("cancel", () => cancelled++);

    await fireEvent.keyDown(window, { key: "Escape" });

    expect(cancelled).toBe(1);
  });

  it("pressing Escape in the field dispatches cancel exactly once (no double-dispatch)", async () => {
    // Regression (CPE-1179 review): a keydown in the field bubbles to window, so if both the field's
    // onKeydown AND the svelte:window listener handled Escape, cancel would fire twice. Only window
    // should handle it.
    const { component } = render(PasswordPromptDialog);
    let cancelled = 0;
    component.$on("cancel", () => cancelled++);

    await fireEvent.keyDown(screen.getByTestId("password-field"), { key: "Escape" });

    expect(cancelled).toBe(1);
  });

  it("renders the optional error line when the error prop is set", () => {
    render(PasswordPromptDialog, { error: "Wrong password — try again" });
    expect(screen.getByTestId("password-error").textContent).toContain("Wrong password — try again");
  });
});
