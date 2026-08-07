import { describe, it, expect } from "vitest";
import { arrowDelta, pageDelta } from "./gridnav";

describe("arrowDelta (CPE-769)", () => {
  describe("single column (list / details)", () => {
    it("Down/Up move by ±1", () => {
      expect(arrowDelta("ArrowDown", 1)).toBe(1);
      expect(arrowDelta("ArrowUp", 1)).toBe(-1);
    });
    it("Left/Right are no-ops (no horizontal move in a 1-column list)", () => {
      expect(arrowDelta("ArrowLeft", 1)).toBe(0);
      expect(arrowDelta("ArrowRight", 1)).toBe(0);
    });
  });

  describe("grid (cols > 1)", () => {
    it("Down/Up move by a whole row (±cols)", () => {
      expect(arrowDelta("ArrowDown", 4)).toBe(4);
      expect(arrowDelta("ArrowUp", 4)).toBe(-4);
      expect(arrowDelta("ArrowDown", 7)).toBe(7);
    });
    it("Right/Left move by one tile (±1), which moveLead wraps across rows", () => {
      expect(arrowDelta("ArrowRight", 4)).toBe(1);
      expect(arrowDelta("ArrowLeft", 4)).toBe(-1);
    });
  });

  describe("guards", () => {
    it("treats cols < 1 / NaN as a single column", () => {
      expect(arrowDelta("ArrowDown", 0)).toBe(1);
      expect(arrowDelta("ArrowDown", -3)).toBe(1);
      expect(arrowDelta("ArrowRight", Number.NaN)).toBe(0);
      expect(arrowDelta("ArrowDown", 2.9)).toBe(2); // floored
    });
    it("non-arrow keys yield 0", () => {
      expect(arrowDelta("Enter", 4)).toBe(0);
      expect(arrowDelta("a", 4)).toBe(0);
      expect(arrowDelta("Home", 4)).toBe(0);
    });
  });
});

describe("pageDelta (CPE-1374)", () => {
  describe("single column (list / details)", () => {
    it("PageDown/PageUp move by ±rowsPerPage", () => {
      expect(pageDelta("PageDown", 1, 12)).toBe(12);
      expect(pageDelta("PageUp", 1, 12)).toBe(-12);
    });
  });

  describe("grid (cols > 1)", () => {
    it("scales by the column count, same as one row in arrowDelta", () => {
      // 3 visible rows of 4 columns = a page of 12 tiles, matching arrowDelta("ArrowDown", 4) * 3.
      expect(pageDelta("PageDown", 4, 3)).toBe(12);
      expect(pageDelta("PageUp", 4, 3)).toBe(-12);
      expect(pageDelta("PageDown", 4, 1)).toBe(arrowDelta("ArrowDown", 4));
    });
  });

  describe("guards", () => {
    it("floors + clamps rowsPerPage and cols to at least 1", () => {
      expect(pageDelta("PageDown", 1, 0)).toBe(1); // a tiny viewport still moves at least one row
      expect(pageDelta("PageDown", 1, -5)).toBe(1);
      expect(pageDelta("PageDown", 0, 5)).toBe(5);
      expect(pageDelta("PageDown", 1, 3.9)).toBe(3); // floored
    });
    it("non-page keys yield 0", () => {
      expect(pageDelta("ArrowDown", 4, 5)).toBe(0);
      expect(pageDelta("Enter", 4, 5)).toBe(0);
    });
  });
});
