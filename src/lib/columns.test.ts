import { describe, it, expect } from "vitest";
import {
  columnsTemplate,
  resizeColumnTo,
  boundaryOffsets,
  COLUMN_MINS,
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
