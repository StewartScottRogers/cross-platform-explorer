/**
 * TestModeOverlay render test (CPE-1046) — the automation "test-mode" halo overlay shown when the app
 * is launched with `--test-mode` (App.svelte reads `window.__CPE_TEST_MODE__` and conditionally renders
 * this component). Verifies the banner text + a halo element are present, and that the whole thing is
 * inert (`pointer-events: none`) so it can never block automation clicks or the user closing the window.
 */
import { render, screen } from "@testing-library/svelte";
import { describe, it, expect } from "vitest";
import TestModeOverlay from "./TestModeOverlay.svelte";

describe("TestModeOverlay", () => {
  it("renders the automated-test warning banner", () => {
    render(TestModeOverlay);
    expect(screen.getByText(/automated test in progress/i)).toBeTruthy();
    expect(screen.getByText(/please don't touch this window/i)).toBeTruthy();
    expect(screen.getByText("🤖")).toBeTruthy();
  });

  it("renders a fixed, full-viewport halo frame above everything and never intercepts clicks", () => {
    const { container } = render(TestModeOverlay);
    const halo = container.querySelector(".test-mode-halo") as HTMLElement;
    expect(halo).toBeTruthy();
    expect(halo.style.pointerEvents).toBe("none");
    expect(halo.style.position).toBe("fixed");
    expect(halo.style.zIndex).toBe("2147483647");

    // Everything under the halo (incl. the banner) must also be non-interactive.
    const banner = container.querySelector(".test-mode-banner") as HTMLElement;
    expect(banner).toBeTruthy();
    expect(banner.style.pointerEvents).toBe("none");
  });

  it("is marked aria-hidden — purely decorative, not part of the accessible content tree", () => {
    const { container } = render(TestModeOverlay);
    expect(container.querySelector(".test-mode-halo")?.getAttribute("aria-hidden")).toBe("true");
  });
});
