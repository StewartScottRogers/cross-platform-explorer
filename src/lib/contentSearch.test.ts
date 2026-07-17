import { describe, it, expect } from "vitest";
import { groupMatches, baseName, parentDir, highlightSegments, type ContentMatch } from "./contentSearch";

const m = (path: string, line_number: number, line = "x"): ContentMatch => ({ path, line_number, line });

describe("highlightSegments (CPE-557)", () => {
  const texts = (line: string, q: string, cs = false) => highlightSegments(line, q, cs).map((s) => (s.match ? `[${s.text}]` : s.text)).join("");

  it("wraps the matched substring, leaving the rest plain", () => {
    expect(texts("the parser owns it", "parser")).toBe("the [parser] owns it");
  });

  it("highlights every occurrence, case-insensitive by default", () => {
    expect(texts("Foo foo FOO", "foo")).toBe("[Foo] [foo] [FOO]");
  });

  it("respects case-sensitive matching", () => {
    expect(texts("Foo foo", "foo", true)).toBe("Foo [foo]");
  });

  it("a blank query leaves the whole line unmatched", () => {
    const segs = highlightSegments("hello", "");
    expect(segs).toEqual([{ text: "hello", match: false }]);
  });

  it("no match → the whole line as one plain segment", () => {
    expect(highlightSegments("abc", "zzz")).toEqual([{ text: "abc", match: false }]);
  });

  it("handles a match at the very start and end", () => {
    expect(texts("abcabc", "abc")).toBe("[abc][abc]");
  });
});

describe("content search helpers (CPE-417)", () => {
  it("groups matches by file, first-seen order preserved", () => {
    const groups = groupMatches([m("/a.txt", 1), m("/b.txt", 2), m("/a.txt", 5)]);
    expect(groups.map((g) => g.path)).toEqual(["/a.txt", "/b.txt"]);
    expect(groups[0].matches.map((x) => x.line_number)).toEqual([1, 5]);
  });

  it("baseName + parentDir handle both separators", () => {
    expect(baseName("Z:\\repos\\app\\main.ts")).toBe("main.ts");
    expect(baseName("/home/u/x.rs")).toBe("x.rs");
    expect(parentDir("Z:/repos/app/main.ts")).toBe("Z:/repos/app");
    expect(parentDir("Z:\\repos\\app\\main.ts")).toBe("Z:\\repos\\app");
    expect(parentDir("x")).toBe("");
  });
});
