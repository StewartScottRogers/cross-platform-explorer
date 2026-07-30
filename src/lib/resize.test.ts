import { describe, it, expect } from "vitest";
import { clampWidth, maxSidePaneWidth, PANE_DIVIDER_W } from "./resize";
import { MID_MIN, NAME_COL_MIN } from "./columns";

describe("clampWidth", () => {
  it("returns the value when within range", () => {
    expect(clampWidth(300, 160, 480)).toBe(300);
  });
  it("clamps to the minimum (safe floor)", () => {
    expect(clampWidth(50, 160, 480)).toBe(160);
  });
  it("clamps to the maximum", () => {
    expect(clampWidth(9999, 160, 480)).toBe(480);
  });
});

// CPE-1140: the side panes no longer have a fixed maximum — `maxSidePaneWidth` computes a dynamic
// one instead, so a side pane can grow unbounded EXCEPT where doing so would squeeze the middle
// (file-list) pane below its own minimum.
describe("maxSidePaneWidth (CPE-1140)", () => {
  it("leaves exactly enough room for the other pane, the dividers, and the middle's minimum", () => {
    const max = maxSidePaneWidth(1200, 300, PANE_DIVIDER_W, 2, 372, 160);
    expect(max).toBe(1200 - 300 - 2 * PANE_DIVIDER_W - 372); // 516
  });

  it("never drops below the pane's own minimum, even when the window can't fit everything", () => {
    // window far too narrow to also satisfy the middle's minimum — the side pane still gets its floor;
    // the middle pane (not this side pane) is what overflows, per the ticket's narrow-window rule.
    const max = maxSidePaneWidth(500, 300, PANE_DIVIDER_W, 2, 372, 160);
    expect(max).toBe(160);
  });
});

// (a) widening a side pane stops so the middle pane stays >= MID_MIN.
describe("side-pane drag clamp respects the middle pane's minimum (CPE-1140)", () => {
  it("dragging the sidebar to an extreme width still leaves the middle pane >= MID_MIN", () => {
    const windowWidth = 1400;
    const rightWidth = 300; // right pane holding its default width
    const sidebarMin = 160;
    const max = maxSidePaneWidth(windowWidth, rightWidth, PANE_DIVIDER_W, 2, MID_MIN, sidebarMin);
    const dragged = clampWidth(9999, sidebarMin, max); // attempt to drag the sidebar to 9999px
    expect(dragged).toBeLessThan(9999);
    const middleWidth = windowWidth - dragged - rightWidth - 2 * PANE_DIVIDER_W;
    expect(middleWidth).toBeGreaterThanOrEqual(MID_MIN);
  });

  it("dragging the right pane to an extreme width still leaves the middle pane >= MID_MIN", () => {
    const windowWidth = 1400;
    const sidebarWidth = 220;
    const rightMin = 220;
    const max = maxSidePaneWidth(windowWidth, sidebarWidth, PANE_DIVIDER_W, 2, MID_MIN, rightMin);
    const dragged = clampWidth(9999, rightMin, max);
    expect(dragged).toBeLessThan(9999);
    const middleWidth = windowWidth - sidebarWidth - dragged - 2 * PANE_DIVIDER_W;
    expect(middleWidth).toBeGreaterThanOrEqual(MID_MIN);
  });
});

// (b) MID_MIN must never be smaller than the Name column's own minimum (requirement #3 of CPE-1140).
describe("MID_MIN floor (CPE-1140)", () => {
  it("MID_MIN is at least the Name column's own minimum width", () => {
    expect(MID_MIN).toBeGreaterThanOrEqual(NAME_COL_MIN);
  });
});

// (c) a persisted width saved under the old fixed SIDEBAR_MAX/RIGHT_MAX (now removed) must load
// re-clamped, never producing a layout where the middle pane is squeezed below MID_MIN.
describe("load-time re-clamp (CPE-1140)", () => {
  it("a stale over-old-max sidebar width loads re-clamped to what the window can support", () => {
    const persisted = 9999; // far over the old (now-removed) SIDEBAR_MAX=480
    const windowWidth = 1024;
    const defaultRightWidth = 300;
    const sidebarMin = 160;
    const max = maxSidePaneWidth(windowWidth, defaultRightWidth, PANE_DIVIDER_W, 2, MID_MIN, sidebarMin);
    const loaded = clampWidth(persisted, sidebarMin, max);

    expect(loaded).toBeLessThan(persisted);
    expect(loaded).toBe(max);
    const middleWidth = windowWidth - loaded - defaultRightWidth - 2 * PANE_DIVIDER_W;
    expect(middleWidth).toBeGreaterThanOrEqual(MID_MIN);
  });

  it("a small persisted width below the minimum still loads clamped up to the minimum", () => {
    const loaded = clampWidth(
      50,
      160,
      maxSidePaneWidth(1600, 300, PANE_DIVIDER_W, 2, MID_MIN, 160),
    );
    expect(loaded).toBe(160);
  });
});
