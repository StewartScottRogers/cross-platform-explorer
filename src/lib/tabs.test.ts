import { describe, it, expect } from "vitest";
import { pushClosedTab, keepOnly, keepThroughRight } from "./tabs";

describe("keepOnly / keepThroughRight (CPE-357)", () => {
  it("keepOnly keeps just the target tab", () => {
    expect(keepOnly([1, 2, 3], 2)).toEqual([2]);
  });
  it("keepThroughRight keeps the target and everything to its left", () => {
    expect(keepThroughRight([1, 2, 3, 4], 2)).toEqual([1, 2]);
    expect(keepThroughRight([1, 2, 3], 3)).toEqual([1, 2, 3]); // rightmost = no-op
    expect(keepThroughRight([1, 2, 3], 1)).toEqual([1]);
  });
  it("keepThroughRight keeps all for an unknown id", () => {
    expect(keepThroughRight([1, 2, 3], 9)).toEqual([1, 2, 3]);
  });
});

describe("pushClosedTab (CPE-356)", () => {
  it("appends the newest closed path last", () => {
    expect(pushClosedTab(["/a"], "/b")).toEqual(["/a", "/b"]);
  });

  it("does not mutate the input", () => {
    const s = ["/a"];
    pushClosedTab(s, "/b");
    expect(s).toEqual(["/a"]);
  });

  it("caps the stack, dropping the oldest", () => {
    let s: string[] = [];
    for (let i = 0; i < 15; i++) s = pushClosedTab(s, `/f${i}`, 10);
    expect(s).toHaveLength(10);
    expect(s[0]).toBe("/f5"); // oldest kept
    expect(s[s.length - 1]).toBe("/f14"); // newest
  });
});
