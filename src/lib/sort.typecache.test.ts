/**
 * CPE-694 (epic CPE-688): a type sort must compute each entry's type name O(n) times — once per entry,
 * cached — not O(n log n) times (twice per comparison). This lives in its own file because it mocks
 * `./filetypes` to count calls, which would perturb the behaviour-based assertions in sort.test.ts.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import type { DirEntry } from "./types";

// vi.mock is hoisted above imports, so the spy it references must be created via vi.hoisted or the
// factory closes over an uninitialised binding.
const { typeNameSpy } = vi.hoisted(() => ({
  typeNameSpy: vi.fn(
    (e: { is_dir: boolean; extension: string }) =>
      e.is_dir ? "File folder" : e.extension ? `${e.extension.toUpperCase()} File` : "File",
  ),
}));
vi.mock("./filetypes", () => ({ typeName: typeNameSpy }));

import { sortEntries } from "./sort";

const entry = (over: Partial<DirEntry>): DirEntry => ({
  name: "x",
  path: "/x",
  is_dir: false,
  size: 0,
  modified: 0,
  extension: "",
  hidden: false,
  ...over,
});

// Block body: an arrow that *returns* mockClear()'s value (the spy itself) would be registered by
// vitest as a per-test teardown and invoked with no args — calling typeName(undefined).
beforeEach(() => {
  typeNameSpy.mockClear();
});

describe("type sort caches typeName (CPE-694)", () => {
  it("computes typeName exactly once per entry (O(n), not O(n log n))", () => {
    const exts = ["txt", "md", "png", "txt", "js", "md", "png", "txt"];
    const es = exts.map((extension, i) => entry({ name: `f${i}`, extension }));
    sortEntries(es, "type", "asc");
    // A comparison-time recompute would be ~2·n·log2(n) ≫ n calls for n = 8.
    expect(typeNameSpy).toHaveBeenCalledTimes(es.length);
  });

  it("never calls typeName for non-type sort keys", () => {
    const es = Array.from({ length: 6 }, (_, i) => entry({ name: `f${i}`, extension: "txt" }));
    sortEntries(es, "name", "asc");
    sortEntries(es, "size", "asc");
    sortEntries(es, "modified", "asc");
    expect(typeNameSpy).not.toHaveBeenCalled();
  });
});
