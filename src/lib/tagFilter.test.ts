import { describe, it, expect } from "vitest";
import { filterEntriesByTag, anyEntryHasTag } from "./tagFilter";
import type { TagStore } from "./tags";

const store: TagStore = {
  "/a": { tags: ["work", "urgent"], label: "" },
  "/b": { tags: ["work"], label: "red" },
  "/c": { tags: [], label: "blue" },
};
const entries = [{ path: "/a" }, { path: "/b" }, { path: "/c" }, { path: "/untracked" }];

describe("tagFilter (CPE-639)", () => {
  it("keeps only entries carrying the tag", () => {
    expect(filterEntriesByTag(entries, store, "work").map((e) => e.path)).toEqual(["/a", "/b"]);
    expect(filterEntriesByTag(entries, store, "urgent").map((e) => e.path)).toEqual(["/a"]);
  });
  it("a blank tag returns everything; an unknown tag returns nothing", () => {
    expect(filterEntriesByTag(entries, store, "")).toHaveLength(4);
    expect(filterEntriesByTag(entries, store, "nope")).toHaveLength(0);
  });
  it("anyEntryHasTag reflects presence", () => {
    expect(anyEntryHasTag(entries, store, "work")).toBe(true);
    expect(anyEntryHasTag(entries, store, "nope")).toBe(false);
  });
});
