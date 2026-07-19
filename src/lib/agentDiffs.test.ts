import { describe, it, expect } from "vitest";
import {
  foldDiffs,
  normalizeFsDiff,
  diffFor,
  diffSegs,
  emptyDiffState,
  ingestDiff,
  currentDiffs,
  clearDiffs,
  type FsDiff,
} from "./agentDiffs";

const d = (path: string, before: string, after: string): FsDiff => ({ path, before, after });

describe("foldDiffs", () => {
  it("stores the latest diff per path, newest to the front", () => {
    let s = emptyDiffState();
    s = foldDiffs(s, [d("/a", "", "1"), d("/b", "x", "y")]);
    expect(s.order).toEqual(["/b", "/a"]);
    // A second write to /a replaces its record and moves it to the front.
    s = foldDiffs(s, [d("/a", "1", "2")]);
    expect(s.order).toEqual(["/a", "/b"]);
    expect(diffFor(s, "/a")).toEqual(d("/a", "1", "2"));
  });

  it("keeps the char total accurate across replace", () => {
    let s = emptyDiffState();
    s = foldDiffs(s, [d("/a", "aa", "bbb")]); // 2 + 3 = 5
    expect(s.chars).toBe(5);
    s = foldDiffs(s, [d("/a", "bbb", "c")]); // replace: 3 + 1 = 4
    expect(s.chars).toBe(4);
  });

  it("evicts the oldest entries over the count cap", () => {
    let s = emptyDiffState();
    s = foldDiffs(s, [d("/a", "", "a"), d("/b", "", "b"), d("/c", "", "c")], 2);
    expect(s.order).toEqual(["/c", "/b"]); // /a (oldest) evicted
    expect(diffFor(s, "/a")).toBeNull();
    expect(s.byPath["/a"]).toBeUndefined();
  });

  it("evicts the oldest entries over the char cap", () => {
    let s = emptyDiffState();
    // charCap 5; each entry costs 3 -> only the newest fits.
    s = foldDiffs(s, [d("/a", "aa", "a"), d("/b", "bb", "b")], 100, 5);
    expect(s.order).toEqual(["/b"]);
    expect(s.chars).toBeLessThanOrEqual(5);
  });
});

describe("normalizeFsDiff", () => {
  it("keeps well-formed records and drops malformed ones", () => {
    const items = normalizeFsDiff([
      { path: "/a", before: "x", after: "y" },
      { path: "", before: "x", after: "y" }, // empty path
      { path: "/b", before: 1, after: "y" }, // non-string before
      { path: "/c", after: "y" }, // missing before
      "nope",
    ]);
    expect(items).toEqual([{ path: "/a", before: "x", after: "y" }]);
  });

  it("returns [] for non-array payloads", () => {
    expect(normalizeFsDiff(null)).toEqual([]);
    expect(normalizeFsDiff({})).toEqual([]);
  });
});

describe("diffSegs", () => {
  it("returns intra-content segments for a stored path, null otherwise", () => {
    let s = emptyDiffState();
    s = foldDiffs(s, [d("/a", "hello world", "hello there")]);
    const segs = diffSegs(s, "/a");
    expect(segs).not.toBeNull();
    // Reconstructing each side from its segments yields the original strings.
    expect(segs!.old.map((x) => x.text).join("")).toBe("hello world");
    expect(segs!.new.map((x) => x.text).join("")).toBe("hello there");
    expect(diffSegs(s, "/missing")).toBeNull();
  });
});

describe("store lifecycle", () => {
  it("ingests a payload and clears back to empty", () => {
    clearDiffs();
    ingestDiff([{ path: "/a", before: "", after: "hi" }]);
    expect(diffFor(currentDiffs(), "/a")).toEqual({ path: "/a", before: "", after: "hi" });
    clearDiffs();
    expect(currentDiffs()).toEqual(emptyDiffState());
  });

  it("ignores an empty/malformed payload without touching the store", () => {
    clearDiffs();
    ingestDiff("garbage");
    ingestDiff([]);
    expect(currentDiffs()).toEqual(emptyDiffState());
  });
});
