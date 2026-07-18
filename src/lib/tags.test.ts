import { describe, it, expect } from "vitest";
import { entryFor, hasTag, allTags, labelColor, LABEL_COLORS, type TagStore } from "./tags";

const store: TagStore = {
  "/a/one": { tags: ["work", "urgent"], label: "red" },
  "/a/two": { tags: ["home", "work"], label: "" },
};

describe("tags helpers (CPE-636)", () => {
  it("entryFor returns the present entry and an empty entry for a missing path", () => {
    expect(entryFor(store, "/a/one")).toEqual({ tags: ["work", "urgent"], label: "red" });
    expect(entryFor(store, "/a/missing")).toEqual({ tags: [], label: "" });
  });

  it("hasTag reports membership per path", () => {
    expect(hasTag(store, "/a/one", "urgent")).toBe(true);
    expect(hasTag(store, "/a/one", "home")).toBe(false);
    expect(hasTag(store, "/a/missing", "work")).toBe(false);
  });

  it("allTags returns the distinct tags across the store, sorted", () => {
    expect(allTags(store)).toEqual(["home", "urgent", "work"]);
    expect(allTags({})).toEqual([]);
  });

  it("labelColor resolves a known label to its hex and an unknown/empty label to ''", () => {
    expect(labelColor("red")).toBe(LABEL_COLORS.red);
    expect(labelColor("green")).toBe("#4ca65a");
    expect(labelColor("")).toBe("");
    expect(labelColor("chartreuse")).toBe("");
  });
});
