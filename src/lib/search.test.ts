import { describe, it, expect, vi } from "vitest";
import { matchesQuery, makeMatcher } from "./search";

describe("matchesQuery", () => {
  it("matches everything for an empty query", () => {
    expect(matchesQuery("anything.txt", "")).toBe(true);
    expect(matchesQuery("anything.txt", "   ")).toBe(true);
  });

  it("does a case-insensitive substring match for plain queries", () => {
    expect(matchesQuery("Report-2026.pdf", "report")).toBe(true);
    expect(matchesQuery("Report-2026.pdf", "2026")).toBe(true);
    expect(matchesQuery("Report-2026.pdf", "budget")).toBe(false);
  });

  it("treats * as any run of characters (anchored full match)", () => {
    expect(matchesQuery("notes.txt", "*.txt")).toBe(true);
    expect(matchesQuery("a.txtx", "*.txt")).toBe(false); // anchored, not substring
    expect(matchesQuery("archive.tar.gz", "*.gz")).toBe(true);
    expect(matchesQuery("img_2026.jpg", "img_*")).toBe(true);
  });

  it("treats ? as exactly one character", () => {
    expect(matchesQuery("report1.md", "report?.md")).toBe(true);
    expect(matchesQuery("reportA.md", "report?.md")).toBe(true);
    expect(matchesQuery("report10.md", "report?.md")).toBe(false);
    expect(matchesQuery("report.md", "report?.md")).toBe(false);
  });

  it("is case-insensitive for wildcard queries", () => {
    expect(matchesQuery("PHOTO.JPG", "*.jpg")).toBe(true);
  });

  it("treats regex metacharacters in the query literally", () => {
    expect(matchesQuery("a+b.txt", "a+b.*")).toBe(true);
    expect(matchesQuery("axb.txt", "a+b.*")).toBe(false); // '+' is literal, not "one or more"
    expect(matchesQuery("file(1).txt", "file(1)*")).toBe(true);
  });
});

describe("makeMatcher (compile once, CPE-695)", () => {
  it("matches identically to matchesQuery for plain, glob, and empty queries", () => {
    const cases: [string, string][] = [
      ["Report-2026.pdf", "report"],
      ["notes.txt", "*.txt"],
      ["a.txtx", "*.txt"],
      ["report1.md", "report?.md"],
      ["PHOTO.JPG", "*.jpg"],
      ["a+b.txt", "a+b.*"],
      ["anything", ""],
    ];
    for (const [name, query] of cases) {
      expect(makeMatcher(query)(name)).toBe(matchesQuery(name, query));
    }
  });

  it("compiles a glob's RegExp once regardless of how many names it tests", () => {
    const spy = vi.spyOn(globalThis, "RegExp");
    const match = makeMatcher("*.txt");
    ["a.txt", "b.txt", "c.md", "d.txt", "e.png"].forEach(match);
    // globToRegExp's `.replace(/…/g)` calls use regex *literals* (not the constructor); only the single
    // `new RegExp(pattern)` inside makeMatcher counts — proving no per-entry recompile.
    expect(spy).toHaveBeenCalledTimes(1);
    spy.mockRestore();
  });

  it("does not construct a RegExp for a plain (non-glob) query", () => {
    const spy = vi.spyOn(globalThis, "RegExp");
    const match = makeMatcher("report");
    ["report.md", "budget.md"].forEach(match);
    expect(spy).not.toHaveBeenCalled();
    spy.mockRestore();
  });
});
