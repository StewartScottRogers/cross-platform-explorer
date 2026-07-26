import { describe, it, expect } from "vitest";
import {
  FALLBACK_LINE_HEIGHT,
  resolveLineHeight,
  lineToScrollTop,
  scrollTopToLine,
  enclosingSymbol,
  countHiddenAbove,
  foldToExpand,
  lineAtVisibleRow,
  rowScrollTarget,
} from "./outline";
import type { Symbol as CodeSymbol, FoldRange } from "../bindings.gen";

const fold = (start: number, end: number): FoldRange => ({ start_line: start, end_line: end, kind: "block" });

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

// ---- fold-aware jump/breadcrumb corrections (CPE-1095) ----

describe("countHiddenAbove", () => {
  it("is 0 when nothing is hidden (no-fold case)", () => {
    expect(countHiddenAbove(50, new Set())).toBe(0);
  });

  it("counts only hidden lines strictly above the target", () => {
    // A collapsed fold spanning lines 10-19 hides 11..19 (start line itself stays rendered).
    const hidden = new Set([11, 12, 13, 14, 15, 16, 17, 18, 19]);
    expect(countHiddenAbove(9, hidden)).toBe(0); // above the fold entirely
    expect(countHiddenAbove(25, hidden)).toBe(9); // below the fold: all 9 hidden lines count
  });

  it("does not count a hidden line that is >= the target itself", () => {
    const hidden = new Set([5, 6, 7]);
    expect(countHiddenAbove(6, hidden)).toBe(1); // only line 5 is strictly above 6
  });
});

describe("foldToExpand", () => {
  const foldByStart = new Map([
    [10, fold(10, 19)],
    [30, fold(30, 32)],
  ]);

  it("returns null when no fold is collapsed (no-fold case)", () => {
    expect(foldToExpand(15, new Set(), foldByStart)).toBeNull();
  });

  it("returns null when the line isn't inside any collapsed fold's range", () => {
    expect(foldToExpand(25, new Set([10]), foldByStart)).toBeNull();
    // The fold's own start line is rendered (not hidden), so it doesn't need expanding either.
    expect(foldToExpand(10, new Set([10]), foldByStart)).toBeNull();
  });

  it("returns the start line of the collapsed fold hiding the target (fold-collapsed-above case)", () => {
    expect(foldToExpand(15, new Set([10]), foldByStart)).toBe(10);
    expect(foldToExpand(31, new Set([10, 30]), foldByStart)).toBe(30);
  });

  it("picks the correct fold when multiple are collapsed", () => {
    expect(foldToExpand(19, new Set([10, 30]), foldByStart)).toBe(10);
  });
});

describe("lineAtVisibleRow", () => {
  it("is the identity when nothing is hidden (no-fold case)", () => {
    expect(lineAtVisibleRow(1, 50, new Set())).toBe(1);
    expect(lineAtVisibleRow(20, 50, new Set())).toBe(20);
  });

  it("shifts past hidden lines to find the real source line", () => {
    // Lines 11-19 are hidden (a collapsed 10-19 fold): the 10th rendered row (line 10) is followed
    // immediately by the 11th rendered row, which is real source line 20.
    const hidden = new Set([11, 12, 13, 14, 15, 16, 17, 18, 19]);
    expect(lineAtVisibleRow(10, 50, hidden)).toBe(10);
    expect(lineAtVisibleRow(11, 50, hidden)).toBe(20);
    expect(lineAtVisibleRow(12, 50, hidden)).toBe(21);
  });

  it("clamps a visibleRow past the last rendered row to the last real line", () => {
    expect(lineAtVisibleRow(999, 50, new Set())).toBe(50);
  });

  it("is division/index-safe for a non-positive totalLines", () => {
    expect(lineAtVisibleRow(5, 0, new Set())).toBe(1);
    expect(lineAtVisibleRow(5, -1, new Set())).toBe(1);
  });
});

describe("rowScrollTarget", () => {
  it("uses the fallback when the row isn't rendered (row-not-rendered fallback case)", () => {
    expect(rowScrollTarget(null, { top: 100 }, 40, 123)).toBe(123);
  });

  it("computes the row's position relative to the container when the row IS rendered", () => {
    // container top at 100px, scrolled 40px down, row's own top (post-scroll) at 130px.
    expect(rowScrollTarget({ top: 130 }, { top: 100 }, 40, 999)).toBe(70); // 130 - 100 + 40
  });

  it("ignores the fallback entirely once a row rect is available", () => {
    expect(rowScrollTarget({ top: 0 }, { top: 0 }, 0, 99999)).toBe(0);
  });
});
