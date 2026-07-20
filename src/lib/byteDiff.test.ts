import { describe, it, expect } from "vitest";
import { byteDiff } from "./byteDiff";

const b = (...n: number[]) => Uint8Array.from(n);

describe("byteDiff (CPE-778)", () => {
  it("equal buffers → equal, no ranges, no firstDiff", () => {
    const r = byteDiff(b(1, 2, 3), b(1, 2, 3));
    expect(r).toEqual({ equal: true, firstDiff: null, ranges: [], lengthDiffers: false });
  });

  it("a single differing byte → one 1-long range at firstDiff", () => {
    const r = byteDiff(b(1, 2, 3, 4), b(1, 9, 3, 4));
    expect(r.equal).toBe(false);
    expect(r.firstDiff).toBe(1);
    expect(r.ranges).toEqual([{ start: 1, len: 1 }]);
    expect(r.lengthDiffers).toBe(false);
  });

  it("coalesces adjacent differences and separates by matches", () => {
    // positions 1,2 differ; 4 differs; rest match
    const r = byteDiff(b(0, 1, 1, 0, 1, 0), b(0, 9, 9, 0, 9, 0));
    expect(r.ranges).toEqual([{ start: 1, len: 2 }, { start: 4, len: 1 }]);
    expect(r.firstDiff).toBe(1);
  });

  it("flags a length difference with a trailing range when the common part matches", () => {
    const r = byteDiff(b(1, 2, 3), b(1, 2, 3, 4, 5));
    expect(r.lengthDiffers).toBe(true);
    expect(r.equal).toBe(false);
    expect(r.ranges).toEqual([{ start: 3, len: 2 }]); // the extra tail bytes
    expect(r.firstDiff).toBe(3);
  });

  it("merges the trailing tail with a diff run that reaches the common end", () => {
    // last common byte differs (index 2), and b has extra bytes → one merged range [2, 3]
    const r = byteDiff(b(1, 2, 3), b(1, 2, 9, 4, 5));
    expect(r.ranges).toEqual([{ start: 2, len: 3 }]);
    expect(r.lengthDiffers).toBe(true);
  });

  it("handles empty buffers", () => {
    expect(byteDiff(b(), b())).toEqual({ equal: true, firstDiff: null, ranges: [], lengthDiffers: false });
    const r = byteDiff(b(), b(1, 2));
    expect(r).toEqual({ equal: false, firstDiff: 0, ranges: [{ start: 0, len: 2 }], lengthDiffers: true });
  });
});
