import { describe, it, expect } from "vitest";
import { groupMatches, baseName, parentDir, highlightSegments, pushRecentSearch, relativeToRoot, scorePercent, type ContentMatch } from "./contentSearch";

const m = (path: string, line_number: number, line = "x"): ContentMatch => ({ path, line_number, line });

describe("pushRecentSearch (CPE-558)", () => {
  it("prepends newest-first and ignores a blank query", () => {
    expect(pushRecentSearch([], "foo")).toEqual(["foo"]);
    expect(pushRecentSearch(["foo"], "bar")).toEqual(["bar", "foo"]);
    expect(pushRecentSearch(["foo"], "   ")).toEqual(["foo"]); // blank ignored
  });

  it("de-duplicates, moving a repeat to the front", () => {
    expect(pushRecentSearch(["a", "b", "c"], "c")).toEqual(["c", "a", "b"]);
  });

  it("caps the list at max", () => {
    expect(pushRecentSearch(["b", "c", "d"], "a", 3)).toEqual(["a", "b", "c"]);
  });

  it("trims the stored query", () => {
    expect(pushRecentSearch([], "  hi  ")).toEqual(["hi"]);
  });
});

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

  // CPE-1235: a POSIX file directly at the filesystem root has its only separator at index 0, which
  // failed the general `cut > 0` slice and returned "" — silently un-watchable by smartFolderLiveRefresh
  // (a bare "" is filtered out of watchPathsForScope). The parent of a root-level file is the root itself.
  it("parentDir returns the root for a POSIX file at the filesystem root (CPE-1235)", () => {
    expect(parentDir("/foo.txt")).toBe("/");
  });

  it("parentDir regression set: nested POSIX, Windows drive-root, and no-separator are unaffected", () => {
    expect(parentDir("/a/b/c.txt")).toBe("/a/b"); // nested POSIX path
    expect(parentDir("C:\\foo.txt")).toBe("C:"); // Windows drive-root file
    expect(parentDir("foo.txt")).toBe(""); // no separator at all
    expect(parentDir("/")).toBe(""); // the root itself has no parent (unchanged, pre-existing behavior)
  });
  it("baseName preserves case (CPE-618 — must not lowercase display names)", () => {
    expect(baseName("C:/Users/Stewart/Documents")).toBe("Documents");
    expect(baseName("Z:\\repos\\App.svelte")).toBe("App.svelte");
  });
});

describe("relativeToRoot (CPE-1263)", () => {
  it("strips the root prefix, both separator styles", () => {
    expect(relativeToRoot("Z:\\repos\\app\\src\\main.ts", "Z:\\repos\\app")).toBe("src/main.ts");
    expect(relativeToRoot("/home/u/proj/src/main.ts", "/home/u/proj")).toBe("src/main.ts");
  });

  it("is case-insensitive about the root prefix (Windows drives)", () => {
    expect(relativeToRoot("Z:\\Repos\\App\\src\\main.ts", "z:\\repos\\app")).toBe("src/main.ts");
  });

  it("tolerates a trailing slash on root", () => {
    expect(relativeToRoot("Z:\\repos\\app\\src\\main.ts", "Z:\\repos\\app\\")).toBe("src/main.ts");
  });

  it("falls back to the normalised full path when it isn't under root", () => {
    expect(relativeToRoot("Z:\\other\\file.txt", "Z:\\repos\\app")).toBe("Z:/other/file.txt");
  });
});

describe("scorePercent (CPE-1263)", () => {
  it("scales a 0..1 cosine score to a rounded 0..100 percentage", () => {
    expect(scorePercent(1)).toBe(100);
    expect(scorePercent(0.5)).toBe(50);
    expect(scorePercent(0.873)).toBe(87);
    expect(scorePercent(0)).toBe(0);
  });

  it("clamps defensively outside 0..1", () => {
    expect(scorePercent(1.4)).toBe(100);
    expect(scorePercent(-0.2)).toBe(0);
  });
});
