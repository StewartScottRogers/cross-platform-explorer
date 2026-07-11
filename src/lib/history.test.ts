import { describe, it, expect } from "vitest";
import {
  createHistory, visit, back, forward, canGoBack, canGoForward, current,
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
});
