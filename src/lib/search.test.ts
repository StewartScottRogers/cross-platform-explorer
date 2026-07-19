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

  it("expands a {a,b} brace group to any of its alternatives (CPE-697)", () => {
    expect(matchesQuery("photo.jpg", "*.{jpg,png}")).toBe(true);
    expect(matchesQuery("photo.png", "*.{jpg,png}")).toBe(true);
    expect(matchesQuery("photo.gif", "*.{jpg,png}")).toBe(false);
    expect(matchesQuery("photo.gif", "*.{jpg,png,gif}")).toBe(true);
  });

  it("supports * and ? inside a brace group", () => {
    expect(matchesQuery("archive.tar.gz", "*.{tar.*,zip}")).toBe(true);
    expect(matchesQuery("data.zip", "*.{tar.*,zip}")).toBe(true);
    expect(matchesQuery("report1.md", "report{?,10}.md")).toBe(true);
    expect(matchesQuery("report10.md", "report{?,10}.md")).toBe(true);
    expect(matchesQuery("reportXX.md", "report{?,10}.md")).toBe(false);
  });

  it("expands nested and multiple brace groups", () => {
    expect(matchesQuery("img.jpg", "{img,pic}.{jpg,png}")).toBe(true);
    expect(matchesQuery("pic.png", "{img,pic}.{jpg,png}")).toBe(true);
    expect(matchesQuery("doc.jpg", "{img,pic}.{jpg,png}")).toBe(false);
    expect(matchesQuery("a1.txt", "{a{1,2},b}.txt")).toBe(true);
    expect(matchesQuery("b.txt", "{a{1,2},b}.txt")).toBe(true);
    expect(matchesQuery("a3.txt", "{a{1,2},b}.txt")).toBe(false);
  });

  it("treats an unmatched brace or a comma-less group literally", () => {
    // No top-level comma → the braces are literal characters in the name.
    expect(matchesQuery("{x}.txt", "{x}.txt")).toBe(true);
    expect(matchesQuery("x.txt", "{x}.txt")).toBe(false);
    // Unmatched opening brace → literal (and, lacking * ? or a group, a plain substring query).
    expect(matchesQuery("a{b.txt", "a{b")).toBe(true);
    expect(matchesQuery("axb.txt", "a{b")).toBe(false);
    // A comma outside any group is a literal comma.
    expect(matchesQuery("a,b.txt", "a,b*")).toBe(true);
    expect(matchesQuery("ab.txt", "a,b*")).toBe(false);
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
