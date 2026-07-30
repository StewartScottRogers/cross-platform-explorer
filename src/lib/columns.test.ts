import { describe, it, expect } from "vitest";
import {
  columnsTemplate,
  resizeColumnTo,
  boundaryOffsets,
  COLUMN_MINS,
  NAME_COL_MIN,
  MID_MIN,
} from "./columns";

describe("columnsTemplate (CPE-350)", () => {
  it("renders px widths plus a trailing 1fr spacer", () => {
    expect(columnsTemplate([320, 150, 120, 90])).toBe("320px 150px 120px 90px 1fr");
  });
  it("rounds fractional widths", () => {
    expect(columnsTemplate([100.4, 99.6])).toBe("100px 100px 1fr");
  });
});

describe("resizeColumnTo (CPE-350)", () => {
  const w = [320, 150, 120, 90];

  it("sets a column width and does not mutate the input", () => {
    const out = resizeColumnTo(w, 0, 400);
    expect(out[0]).toBe(400);
    expect(w[0]).toBe(320);
  });

  it("clamps below the per-column minimum (never collapses to zero)", () => {
    expect(resizeColumnTo(w, 3, 5)[3]).toBe(COLUMN_MINS[3]); // Size min
    expect(resizeColumnTo(w, 0, -100)[0]).toBe(COLUMN_MINS[0]);
  });

  it("clamps above the maximum", () => {
    expect(resizeColumnTo(w, 0, 99999)[0]).toBe(1200);
  });

  it("ignores an out-of-range index", () => {
    expect(resizeColumnTo(w, 9, 500)).toEqual(w);
    expect(resizeColumnTo(w, -1, 500)).toEqual(w);
  });
});

describe("boundaryOffsets (CPE-350)", () => {
  it("accumulates from the left padding to each column's right edge", () => {
    expect(boundaryOffsets([320, 150, 120, 90], 10)).toEqual([330, 480, 600, 690]);
  });
});

// CPE-1140: the middle (file-list) pane's derived minimum must never be smaller than the
// leftmost Name column's own minimum — the middle pane can never be narrower than that column.
describe("MID_MIN (CPE-1140)", () => {
  it("equals NAME_COL_MIN", () => {
    expect(NAME_COL_MIN).toBe(COLUMN_MINS[0]);
  });
  it("is at least the Name column's own minimum", () => {
    expect(MID_MIN).toBeGreaterThanOrEqual(NAME_COL_MIN);
  });
  it("is at least the sum of every visible column's minimum width", () => {
    const sum = COLUMN_MINS.reduce((a, b) => a + b, 0);
    expect(MID_MIN).toBeGreaterThanOrEqual(sum);
  });
});
