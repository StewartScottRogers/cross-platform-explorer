import { describe, it, expect } from "vitest";
import { windowRange, totalHeight } from "./virtualize";

describe("virtualize windowing (CPE-690)", () => {
  it("returns an empty window for empty or degenerate input", () => {
    expect(windowRange(0, 500, 20, 0)).toEqual({ start: 0, end: 0, padTop: 0, padBottom: 0 });
    expect(windowRange(0, 500, 0, 100)).toEqual({ start: 0, end: 0, padTop: 0, padBottom: 0 });
    expect(windowRange(0, 0, 20, 100)).toEqual({ start: 0, end: 0, padTop: 0, padBottom: 0 });
  });

  it("renders the top window + overscan at scrollTop 0 (list)", () => {
    // 20px rows, 200px viewport → 10 visible rows; +3 overscan below, 0 above at top.
    const w = windowRange(0, 200, 20, 1000, 1, 3);
    expect(w.start).toBe(0);
    expect(w.end).toBe(13); // 10 visible + 3 overscan
    expect(w.padTop).toBe(0);
    expect(w.padBottom).toBe((1000 - 13) * 20);
  });

  it("windows around the scroll position with overscan above and below", () => {
    // scrolled to row 100 (2000px). firstRow=100, visible=10, overscan=3.
    const w = windowRange(2000, 200, 20, 1000, 1, 3);
    expect(w.start).toBe(97); // 100 - 3
    expect(w.end).toBe(113); // 100 + 10 + 3
    expect(w.padTop).toBe(97 * 20);
    expect(w.padBottom).toBe((1000 - 113) * 20);
  });

  it("clamps at the end of the list (no negative padBottom)", () => {
    const w = windowRange(1_000_000, 200, 20, 1000, 1, 3);
    expect(w.end).toBe(1000);
    expect(w.padBottom).toBe(0);
    expect(w.start).toBeLessThan(w.end);
  });

  it("is row-aligned for grid views (columns = N)", () => {
    // 4 columns, 120px tiles, 240px viewport → 2 visible rows, +1 overscan.
    const w = windowRange(0, 240, 120, 100, 4, 1);
    expect(w.start).toBe(0);
    // firstRow=0, visibleRows=2, +1 overscan → endRow=3 → 3*4 = 12 items
    expect(w.end).toBe(12);
    expect(w.start % 4).toBe(0); // window starts on a row boundary
  });

  it("padTop + rendered-rows height + padBottom == total scroll height (scrollbar honest)", () => {
    const rowH = 22, count = 777, cols = 3;
    const w = windowRange(3000, 300, rowH, count, cols, 2);
    const renderedRows = Math.ceil(w.end / cols) - w.start / cols;
    const rendered = renderedRows * rowH;
    expect(w.padTop + rendered + w.padBottom).toBe(totalHeight(rowH, count, cols));
  });

  it("totalHeight rounds up partial final rows", () => {
    expect(totalHeight(20, 100, 1)).toBe(2000);
    expect(totalHeight(120, 10, 4)).toBe(360); // 10 items / 4 cols = 3 rows
    expect(totalHeight(20, 0, 1)).toBe(0);
  });
});
