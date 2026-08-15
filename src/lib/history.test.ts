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

  describe("CPE-1737 round 2: trailing-slash spelling", () => {
    it("re-visiting the SAME folder via a differently-slashed spelling is still a no-op", () => {
      // A remote directory row's path now legitimately carries a trailing '/' (CPE-1737 round 1);
      // Up/breadcrumb/typed-address never produce that shape. Before comparing canonically, arriving
      // back at "the folder I'm already on" via the OTHER spelling pushed a spurious duplicate entry —
      // so Back would then land right back on the same folder instead of the one visited before it.
      let h = visit(createHistory("sftp://h/srv"), "sftp://h/srv/sub");
      const before = h;
      h = visit(h, "sftp://h/srv/sub/");
      expect(h).toBe(before);
      expect(h.entries).toEqual(["sftp://h/srv", "sftp://h/srv/sub"]);
    });

    it("never rewrites a LOCAL Windows path's separators — the stored value is exactly what was passed", () => {
      // Regression pin: an earlier round of this fix stored `canonicalPath(path)`, which also
      // normalises '\' to '/' — corrupting a local Windows path's separators the moment it entered
      // history, breaking `current(h)` for every caller that feeds it straight to a backend command.
      const h = createHistory("C:\\d");
      expect(current(h)).toBe("C:\\d");
    });
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
