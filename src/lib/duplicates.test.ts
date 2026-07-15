import { describe, it, expect } from "vitest";
import { redundantPaths, keepsOnePerGroup, pruneGroups } from "./duplicates";

const g = (...paths: string[]) => ({ paths });

describe("duplicate cleanup guards (CPE-428)", () => {
  const groups = [g("/a1", "/a2", "/a3"), g("/b1", "/b2")];

  it("redundantPaths selects every copy except the first of each group", () => {
    expect(redundantPaths(groups)).toEqual(["/a2", "/a3", "/b2"]);
  });

  it("keepsOnePerGroup blocks removing an entire group", () => {
    // Safe default keeps one per group.
    expect(keepsOnePerGroup(groups, new Set(["/a2", "/a3", "/b2"]))).toBe(true);
    // Selecting ALL of group A would wipe it — not allowed.
    expect(keepsOnePerGroup(groups, new Set(["/a1", "/a2", "/a3"]))).toBe(false);
    // Nothing selected is trivially safe.
    expect(keepsOnePerGroup(groups, new Set())).toBe(true);
  });

  it("pruneGroups removes deleted paths and drops groups that are no longer duplicates", () => {
    const after = pruneGroups(groups, new Set(["/a2", "/a3", "/b2"]));
    // Group A drops to 1 (/a1) → removed; group B drops to 1 (/b1) → removed.
    expect(after).toEqual([]);
    const partial = pruneGroups(groups, new Set(["/a3"]));
    expect(partial).toEqual([g("/a1", "/a2"), g("/b1", "/b2")]);
  });
});
