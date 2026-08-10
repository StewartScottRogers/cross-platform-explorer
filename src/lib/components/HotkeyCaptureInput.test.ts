/**
 * Component test for the press-to-set hotkey capture control (CPE-1549, epic CPE-1484). Verifies
 * the capture state machine in isolation (no keymap/conflict knowledge here — that's
 * `KeyboardBindingsDialog.test.ts`'s job): clicking arms capture, a qualifying keydown commits and
 * emits `set` with the `hotkeyFromEvent`-normalized chord, Escape cancels without emitting
 * anything, a bare modifier press is ignored (stays armed), and a combo `hotkeyFromEvent` rejects
 * (no Ctrl/Alt) stays armed with no `set` either.
 */
import { describe, it, expect } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import HotkeyCaptureInput from "./HotkeyCaptureInput.svelte";

describe("HotkeyCaptureInput idle display (CPE-1549)", () => {
  it("shows the display prop when at rest", () => {
    render(HotkeyCaptureInput, { display: "Ctrl+Alt+N" });
    expect(screen.getByText("Ctrl+Alt+N")).toBeTruthy();
  });

  it("shows a 'Click to set…' placeholder when display is empty", () => {
    render(HotkeyCaptureInput, { display: "" });
    expect(screen.getByText("Click to set…")).toBeTruthy();
  });
});

describe("HotkeyCaptureInput arming + capture (CPE-1549)", () => {
  it("arms capture mode on click and shows the 'press a key' prompt", async () => {
    render(HotkeyCaptureInput, { display: "" });
    await fireEvent.click(screen.getByRole("button"));
    expect(screen.getByText("Press a key…")).toBeTruthy();
  });

  it("commits and dispatches `set` with the normalized chord on a qualifying keydown", async () => {
    const { component } = render(HotkeyCaptureInput, { display: "" });
    let captured: string | undefined;
    component.$on("set", (e) => (captured = e.detail));

    await fireEvent.click(screen.getByRole("button"));
    await fireEvent.keyDown(window, { key: "n", ctrlKey: true, altKey: true });

    expect(captured).toBe("Ctrl+Alt+N");
    // Capture disarmed after commit — back to idle prompt, not still "Press a key…".
    expect(screen.queryByText("Press a key…")).toBeNull();
  });

  it("normalizes a bare function/navigation key the same way hotkeyFromEvent would reject it, unless qualified", async () => {
    const { component } = render(HotkeyCaptureInput, { display: "" });
    let captured: string | undefined;
    component.$on("set", (e) => (captured = e.detail));

    await fireEvent.click(screen.getByRole("button"));
    await fireEvent.keyDown(window, { key: "k", metaKey: true });

    expect(captured).toBe("Ctrl+K"); // metaKey (Cmd) counts as Ctrl, same as hotkeyFromEvent elsewhere.
  });
});

describe("HotkeyCaptureInput Escape cancels (CPE-1549)", () => {
  it("Escape cancels capture without emitting `set`", async () => {
    const { component } = render(HotkeyCaptureInput, { display: "" });
    let setCount = 0;
    component.$on("set", () => setCount++);

    await fireEvent.click(screen.getByRole("button"));
    expect(screen.getByText("Press a key…")).toBeTruthy();

    await fireEvent.keyDown(window, { key: "Escape" });

    expect(setCount).toBe(0);
    expect(screen.queryByText("Press a key…")).toBeNull();
    expect(screen.getByText("Click to set…")).toBeTruthy();
  });
});

describe("HotkeyCaptureInput modifier-only + rejected combos (CPE-1549)", () => {
  it("ignores a bare modifier keydown and keeps waiting", async () => {
    const { component } = render(HotkeyCaptureInput, { display: "" });
    let setCount = 0;
    component.$on("set", () => setCount++);

    await fireEvent.click(screen.getByRole("button"));
    await fireEvent.keyDown(window, { key: "Control", ctrlKey: true });

    expect(setCount).toBe(0);
    expect(screen.getByText("Press a key…")).toBeTruthy();
  });

  it("stays armed with no `set` when hotkeyFromEvent rejects the combo (bare letter, no Ctrl/Alt)", async () => {
    const { component } = render(HotkeyCaptureInput, { display: "" });
    let setCount = 0;
    component.$on("set", () => setCount++);

    await fireEvent.click(screen.getByRole("button"));
    await fireEvent.keyDown(window, { key: "n" });

    expect(setCount).toBe(0);
    expect(screen.getByText("Needs Ctrl or Alt…")).toBeTruthy();

    // Still armed — a follow-up qualifying keydown now commits normally.
    let captured: string | undefined;
    component.$on("set", (e) => (captured = e.detail));
    await fireEvent.keyDown(window, { key: "n", ctrlKey: true });
    expect(captured).toBe("Ctrl+N");
  });
});

describe("HotkeyCaptureInput disabled (CPE-1549)", () => {
  it("does not arm when disabled", async () => {
    render(HotkeyCaptureInput, { display: "Ctrl+K", disabled: true });
    await fireEvent.click(screen.getByRole("button"));
    expect(screen.queryByText("Press a key…")).toBeNull();
    expect(screen.getByText("Ctrl+K")).toBeTruthy();
  });
});
