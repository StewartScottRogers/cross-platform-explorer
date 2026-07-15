import { describe, it, expect } from "vitest";
import { groupMatches, baseName, parentDir, type ContentMatch } from "./contentSearch";

const m = (path: string, line_number: number, line = "x"): ContentMatch => ({ path, line_number, line });

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
