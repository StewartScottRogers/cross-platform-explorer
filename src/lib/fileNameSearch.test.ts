import { describe, it, expect } from "vitest";
import { sortNameMatches, type NameMatch } from "./fileNameSearch";

const m = (name: string, is_dir = false, path = name): NameMatch => ({ name, is_dir, path });

describe("sortNameMatches (CPE-603)", () => {
  it("puts folders before files, then sorts by name case-insensitively", () => {
    const out = sortNameMatches([m("beta.txt"), m("Alpha", true), m("alpha.txt"), m("Zeta", true)]);
    expect(out.map((x) => x.name)).toEqual(["Alpha", "Zeta", "alpha.txt", "beta.txt"]);
  });
  it("breaks name ties by full path and does not mutate the input", () => {
    const input = [m("a.txt", false, "/z/a.txt"), m("a.txt", false, "/a/a.txt")];
    const out = sortNameMatches(input);
    expect(out.map((x) => x.path)).toEqual(["/a/a.txt", "/z/a.txt"]);
    expect(input.map((x) => x.path)).toEqual(["/z/a.txt", "/a/a.txt"]); // original untouched
  });
});
