import { describe, it, expect } from "vitest";
import { firstMatchIndex } from "./typeahead";

const names = ["alpha", "beta", "gamma", "beans"];

describe("firstMatchIndex", () => {
  it("finds the first match from the start when nothing is selected", () => {
    expect(firstMatchIndex(names, "b", -1, true)).toBe(1); // beta
  });

  it("cycles to the next match when advancing past the current lead", () => {
    expect(firstMatchIndex(names, "b", 1, true)).toBe(3); // beta -> beans
  });

  it("wraps around to the first match", () => {
    expect(firstMatchIndex(names, "b", 3, true)).toBe(1); // beans -> beta
  });

  it("stays on the current item when refining with a longer prefix", () => {
    expect(firstMatchIndex(names, "be", 1, false)).toBe(1); // beta still matches "be"
  });

  it("matches case-insensitively", () => {
    expect(firstMatchIndex(["Alpha", "Beta"], "a", -1, true)).toBe(0);
  });

  it("returns -1 when nothing matches", () => {
    expect(firstMatchIndex(names, "z", -1, true)).toBe(-1);
  });

  it("returns -1 for an empty prefix or empty list", () => {
    expect(firstMatchIndex(names, "", 0, true)).toBe(-1);
    expect(firstMatchIndex([], "a", -1, true)).toBe(-1);
  });
});
