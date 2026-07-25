/**
 * TestModeOverlay render test (CPE-1046) — the automation "test-mode" corner badge shown when the app
 * is launched with `--test-mode` (App.svelte reads `window.__CPE_TEST_MODE__` and conditionally renders
 * this component). Verifies the badge text renders, it's a small upper-left badge (not a full-viewport
 * frame — the user explicitly required "only as big as needed, no bigger"), and the whole thing is
 * inert (`pointer-events: none`, not focusable) so it can never block automation clicks, steal keyboard
 * focus, or block the user closing the window.
 */
import { render, screen } from "@testing-library/svelte";
import { describe, it, expect } from "vitest";
import TestModeOverlay from "./TestModeOverlay.svelte";

describe("TestModeOverlay", () => {
  it("renders the automated-test warning badge text", () => {
    render(TestModeOverlay);
    expect(screen.getByText(/automated test/i)).toBeTruthy();
    expect(screen.getByText(/don't touch/i)).toBeTruthy();
    expect(screen.getByText("🤖")).toBeTruthy();
  });

  it("is a small badge pinned to the upper-left corner, not a full-viewport frame", () => {
    const { container } = render(TestModeOverlay);
    const badge = container.querySelector(".test-mode-badge") as HTMLElement;
    expect(badge).toBeTruthy();
    expect(badge.style.position).toBe("fixed");
    expect(badge.style.top).toBe("8px");
    expect(badge.style.left).toBe("8px");
    // Sized to its own content — explicitly NOT `inset: 0` / full viewport.
    expect(badge.style.width).toBe("max-content");
    expect(badge.style.inset).toBe("");
  });

  it("never intercepts clicks or steals keyboard focus", () => {
    const { container } = render(TestModeOverlay);
    const badge = container.querySelector(".test-mode-badge") as HTMLElement;
    expect(badge.style.pointerEvents).toBe("none");
    expect(badge.getAttribute("tabindex")).toBe("-1");
    // No focusable/interactive elements anywhere inside (buttons, inputs, links, positive tabindex).
    expect(badge.querySelectorAll("button, input, a, [tabindex]:not([tabindex='-1'])").length).toBe(0);
  });

  it("is marked aria-hidden — purely decorative, not part of the accessible content tree", () => {
    const { container } = render(TestModeOverlay);
    expect(container.querySelector(".test-mode-badge")?.getAttribute("aria-hidden")).toBe("true");
  });
});
