import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/svelte";
import NavModeIndicator from "./NavModeIndicator.svelte";

describe("NavModeIndicator (CPE-1555)", () => {
  it('shows "NORMAL" for mode="normal"', () => {
    render(NavModeIndicator, { mode: "normal" });
    expect(screen.getByTestId("nav-mode-label").textContent).toBe("NORMAL");
  });

  it('shows "VISUAL" for mode="visual"', () => {
    render(NavModeIndicator, { mode: "visual" });
    expect(screen.getByTestId("nav-mode-label").textContent).toBe("VISUAL");
  });

  it("shows no pending readout when nothing is buffered", () => {
    render(NavModeIndicator, { mode: "normal" });
    expect(screen.queryByTestId("nav-mode-pending")).toBeNull();
  });

  it("shows a buffered pending count", () => {
    render(NavModeIndicator, { mode: "normal", pendingCount: "3" });
    expect(screen.getByTestId("nav-mode-pending").textContent).toBe("3");
  });

  it("shows a buffered pending chord", () => {
    render(NavModeIndicator, { mode: "normal", pendingChord: "g" });
    expect(screen.getByTestId("nav-mode-pending").textContent).toBe("g");
  });

  it("shows a combined chord + count buffer (e.g. mid-typing '3g')", () => {
    render(NavModeIndicator, { mode: "normal", pendingChord: "g", pendingCount: "3" });
    expect(screen.getByTestId("nav-mode-pending").textContent).toBe("g3");
  });
});
