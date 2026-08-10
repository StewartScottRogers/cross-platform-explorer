import { describe, it, expect } from "vitest";
import { render, screen, fireEvent } from "@testing-library/svelte";
import ImageCompareView from "./ImageCompareView.svelte";
import type { ImageDiff } from "../bindings.gen";

// PR #746 code review (CPE-1508) caught two real bugs in the zoom/pan wiring that the original test
// suite never exercised — both regression-tested here directly against the component (rather than
// through the whole CompareDialog flow) since neither depends on the backend call, just the already-
// fetched ImageDiff.

function makeDiff(overrides: Partial<ImageDiff> = {}): ImageDiff {
  return {
    width: 100,
    height: 100,
    changedPixels: 10,
    totalPixels: 10000,
    percentDifferent: 0.1,
    bbox: { x: 40, y: 40, width: 20, height: 20 },
    sizeMismatch: false,
    maskPng: [137, 80, 78, 71],
    ...overrides,
  };
}

describe("ImageCompareView zoom/pan (CPE-1508)", () => {
  it("heatmap 'Zoom to changed region' actually changes the zoom state, not a no-op", async () => {
    render(ImageCompareView, { left: "/a.png", right: "/b.png", diff: makeDiff() });

    await fireEvent.click(screen.getByTestId("ic-subview-heatmap"));

    // jsdom never lays out real geometry (clientWidth/clientHeight are always 0), so give the bound
    // viewport a concrete size — the real bug was that `viewportEl` was never bound in the heatmap
    // branch's markup at all (bind:this only appeared on the side-by-side/onion-skin containers), so
    // `zoomToChangedRegion`'s `!viewportEl` guard returned immediately every single time regardless of
    // layout; this stub isolates that from an unrelated "jsdom has no layout" false negative.
    const wrap = screen.getByTestId("ic-heatmap-img").closest(".ic-heat-img-wrap") as HTMLElement;
    Object.defineProperty(wrap, "clientWidth", { value: 400, configurable: true });
    Object.defineProperty(wrap, "clientHeight", { value: 400, configurable: true });

    expect(screen.getByTestId("ic-zoom-pct").textContent).toBe("100%");
    await fireEvent.click(screen.getByTestId("ic-zoom-to-bbox"));
    expect(screen.getByTestId("ic-zoom-pct").textContent).not.toBe("100%");

    // The image + its bbox overlay must move TOGETHER — they share one transformed `.ic-heat-canvas`
    // (previously the `<img>` had no transform at all, so a "zoom" wouldn't have touched it anyway).
    const canvas = screen.getByTestId("ic-heat-canvas");
    expect(canvas.getAttribute("style")).toMatch(/transform: scale\(([2-9]|[1-9]\d+)/); // zoomed in, not 1x
  });

  it("the zoom toolbar (incl. Reset) is reachable from the heatmap sub-view, so a zoom-to-region jump can be undone", async () => {
    render(ImageCompareView, { left: "/a.png", right: "/b.png", diff: makeDiff() });
    await fireEvent.click(screen.getByTestId("ic-subview-heatmap"));
    expect(screen.getByTestId("ic-zoom-pct")).toBeTruthy();

    const wrap = screen.getByTestId("ic-heatmap-img").closest(".ic-heat-img-wrap") as HTMLElement;
    Object.defineProperty(wrap, "clientWidth", { value: 400, configurable: true });
    Object.defineProperty(wrap, "clientHeight", { value: 400, configurable: true });
    await fireEvent.click(screen.getByTestId("ic-zoom-to-bbox"));
    expect(screen.getByTestId("ic-zoom-pct").textContent).not.toBe("100%");

    await fireEvent.click(screen.getByTitle("Reset zoom"));
    expect(screen.getByTestId("ic-zoom-pct").textContent).toBe("100%");
  });

  it("wheel-zoom anchors on the pane actually under the cursor, not a shared ref bound to the left pane", async () => {
    render(ImageCompareView, { left: "/a.png", right: "/b.png", diff: makeDiff() });
    const leftPane = screen.getByTestId("ic-left-img").closest(".ic-pane") as HTMLElement;
    const rightPane = screen.getByTestId("ic-right-img").closest(".ic-pane") as HTMLElement;
    const rectAt = (left: number) => () => ({
      left, top: 0, right: left + 400, bottom: 400, width: 400, height: 400, x: left, y: 0, toJSON() {},
    });
    // The left pane's rect starts at x=0; the right pane's at x=500 (as if laid out side by side) — the
    // bug anchored every wheel-zoom (even over the right pane) against whichever rect `viewportEl`
    // happened to be bound to (only ever the left pane), offsetting the anchor by ~one pane width.
    Object.defineProperty(leftPane, "getBoundingClientRect", { value: rectAt(0), configurable: true });
    Object.defineProperty(rightPane, "getBoundingClientRect", { value: rectAt(500), configurable: true });

    // Wheel-zoom over the RIGHT pane, at screen x=700 (200px into that 400px-wide pane).
    await fireEvent.wheel(rightPane, { deltaY: -100, clientX: 700, clientY: 200 });

    const style = screen.getByTestId("ic-right-img").getAttribute("style") ?? "";
    const m = style.match(/translate\(([-\d.]+)px, ([-\d.]+)px\)/);
    expect(m).toBeTruthy();
    const panX = Number(m![1]);
    // Correct anchor (200px in) keeps the resulting pan small (~-33px); the bug's anchor (700px into a
    // pane whose rect starts at x=0) would produce a pan roughly 3.5x larger in magnitude (~-117px).
    expect(Math.abs(panX)).toBeLessThan(60);
  });
});
