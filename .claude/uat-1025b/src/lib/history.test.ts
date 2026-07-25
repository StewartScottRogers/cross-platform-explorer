import { describe, it, expect } from "vitest";
import {
  createHistory, visit, back, forward, canGoBack, canGoForward, current, recentPaths,
} from "./history";

describe("history", () => {
  it("starts empty with no current entry", () => {
    const h = createHistory();
    expect(current(h)).toBeNull();
    expect(canGoBack(h)).toBe(false);
    expect(canGoForward(h)).toBe(false);
  });

  it("tracks visits and exposes the current path", () => {
    let h = createHistory("/a");
    h = visit(h, "/b");
    expect(current(h)).toBe("/b");
    expect(canGoBack(h)).toBe(true);
    expect(canGoForward(h)).toBe(false);
  });

  it("goes back and forward", () => {
    let h = visit(visit(createHistory("/a"), "/b"), "/c");
    h = back(h);
    expect(current(h)).toBe("/b");
    expect(canGoForward(h)).toBe(true);
    h = forward(h);
    expect(current(h)).toBe("/c");
  });

  it("truncates forward history when navigating somewhere new after going back", () => {
    let h = visit(visit(createHistory("/a"), "/b"), "/c");
    h = back(h); // at /b, forward = /c
    h = visit(h, "/d"); // new branch — /c must be discarded
    expect(current(h)).toBe("/d");
    expect(canGoForward(h)).toBe(false);
    expect(h.entries).toEqual(["/a", "/b", "/d"]);
  });

  it("treats re-visiting the current path as a no-op (refresh must not pile up)", () => {
    let h = visit(createHistory("/a"), "/b");
    const before = h;
    h = visit(h, "/b");
    expect(h).toBe(before);
    expect(h.entries).toEqual(["/a", "/b"]);
  });

  it("clamps at the ends rather than going out of bounds", () => {
    let h = createHistory("/a");
    expect(back(h)).toBe(h);
    expect(forward(h)).toBe(h);
  });

  describe("recentPaths (CPE-604)", () => {
    it("lists distinct prior paths, most recent first, excluding the current", () => {
      let h = createHistory("/a");
      h = visit(h, "/b");
      h = visit(h, "/c"); // current = /c
      expect(recentPaths(h)).toEqual(["/b", "/a"]);
    });
    it("collapses duplicates and honours the cap", () => {
      let h = createHistory("/a");
      h = visit(h, "/b");
      h = visit(h, "/a"); // re-visited /a truncates forward but /a is now current
      h = visit(h, "/c"); // current = /c; entries: /a,/b,/a,/c
      expect(recentPaths(h)).toEqual(["/a", "/b"]); // /a once, /c (current) excluded
      expect(recentPaths(h, 1)).toEqual(["/a"]);
    });
    it("returns nothing for an empty or single-entry history", () => {
      expect(recentPaths(createHistory())).toEqual([]);
      expect(recentPaths(createHistory("/only"))).toEqual([]);
    });
  });
});
