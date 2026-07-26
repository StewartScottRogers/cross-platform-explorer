import { describe, it, expect } from "vitest";
import {
  FALLBACK_LINE_HEIGHT,
  resolveLineHeight,
  lineToScrollTop,
  scrollTopToLine,
  enclosingSymbol,
} from "./outline";
import type { Symbol as CodeSymbol } from "../bindings.gen";

const sym = (name: string, line: number, kind: CodeSymbol["kind"] = "function"): CodeSymbol => ({
  name,
  kind,
  line,
});

describe("resolveLineHeight", () => {
  it("uses the parsed CSS line-height when it is a valid positive number", () => {
    expect(resolveLineHeight("18px", 12)).toBe(18);
  });

  it("falls back to fontSize * 1.4 when line-height is the 'normal' keyword", () => {
    expect(resolveLineHeight("normal", 12)).toBeCloseTo(16.8);
  });

  it("falls back to fontSize * 1.4 when line-height is empty (no layout yet)", () => {
    expect(resolveLineHeight("", 10)).toBeCloseTo(14);
  });

  it("falls back to the constant when both line-height and fontSize are unusable (0/NaN)", () => {
    expect(resolveLineHeight("0px", 0)).toBe(FALLBACK_LINE_HEIGHT);
    expect(resolveLineHeight("normal", NaN)).toBe(FALLBACK_LINE_HEIGHT);
  });

  it("never returns 0, negative, or NaN", () => {
    for (const [lh, fs] of [["0px", 0], ["-5px", -3], ["normal", -1], ["NaNpx", NaN]] as const) {
      const got = resolveLineHeight(lh, fs as number);
      expect(Number.isFinite(got)).toBe(true);
      expect(got).toBeGreaterThan(0);
    }
  });
});

describe("lineToScrollTop", () => {
  it("computes (line-1)*lineHeight", () => {
    expect(lineToScrollTop(1, 18)).toBe(0);
    expect(lineToScrollTop(10, 18)).toBe(9 * 18);
  });

  it("clamps line below 1 up to 1 (scrollTop 0)", () => {
    expect(lineToScrollTop(0, 18)).toBe(0);
    expect(lineToScrollTop(-5, 18)).toBe(0);
  });

  it("is division/NaN-safe: a zero or NaN lineHeight falls back instead of producing NaN", () => {
    expect(Number.isNaN(lineToScrollTop(10, 0))).toBe(false);
    expect(lineToScrollTop(10, 0)).toBe(9 * FALLBACK_LINE_HEIGHT);
    expect(Number.isNaN(lineToScrollTop(10, NaN))).toBe(false);
    expect(lineToScrollTop(10, NaN)).toBe(9 * FALLBACK_LINE_HEIGHT);
  });

  it("is NaN-safe for a NaN line too", () => {
    expect(Number.isNaN(lineToScrollTop(NaN, 18))).toBe(false);
    expect(lineToScrollTop(NaN, 18)).toBe(0);
  });
});

describe("scrollTopToLine", () => {
  it("is the inverse of lineToScrollTop for on-grid values", () => {
    expect(scrollTopToLine(0, 18)).toBe(1);
    expect(scrollTopToLine(9 * 18, 18)).toBe(10);
  });

  it("rounds to the nearest line for an in-between scrollTop", () => {
    expect(scrollTopToLine(18 * 4.6, 18)).toBe(6); // round(4.6)+1 = 5+1
  });

  it("is division/NaN-safe for a zero or NaN lineHeight", () => {
    expect(Number.isNaN(scrollTopToLine(100, 0))).toBe(false);
    expect(Number.isNaN(scrollTopToLine(100, NaN))).toBe(false);
  });

  it("treats a negative/NaN scrollTop as 0", () => {
    expect(scrollTopToLine(-50, 18)).toBe(1);
    expect(scrollTopToLine(NaN, 18)).toBe(1);
  });
});

describe("enclosingSymbol", () => {
  it("returns null for an empty outline", () => {
    expect(enclosingSymbol([], 50)).toBeNull();
  });

  it("returns null when topLine is above the first symbol's line", () => {
    const outline = [sym("run", 5), sym("helper", 12)];
    expect(enclosingSymbol(outline, 1)).toBeNull();
    expect(enclosingSymbol(outline, 4)).toBeNull();
  });

  it("returns the last symbol whose line <= topLine (boundary: exactly on a symbol's line)", () => {
    const outline = [sym("run", 5), sym("helper", 12), sym("Widget", 20)];
    expect(enclosingSymbol(outline, 5)).toEqual(sym("run", 5));
    expect(enclosingSymbol(outline, 11)).toEqual(sym("run", 5));
    expect(enclosingSymbol(outline, 12)).toEqual(sym("helper", 12));
  });

  it("returns the last symbol when topLine is past every symbol's line", () => {
    const outline = [sym("run", 5), sym("helper", 12), sym("Widget", 20)];
    expect(enclosingSymbol(outline, 999)).toEqual(sym("Widget", 20));
  });

  it("is defensive about an out-of-order outline (scans for the max line <= topLine)", () => {
    const outline = [sym("b", 20), sym("a", 5)];
    expect(enclosingSymbol(outline, 10)).toEqual(sym("a", 5));
    expect(enclosingSymbol(outline, 25)).toEqual(sym("b", 20));
  });
});
