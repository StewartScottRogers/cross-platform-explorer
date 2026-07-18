import { describe, it, expect } from "vitest";
import { compareNames, compareEntries, sortEntries } from "./sort";
import type { DirEntry } from "./types";

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

const names = (es: DirEntry[]) => es.map((e) => e.name);

describe("compareNames (natural order)", () => {
  it("orders embedded numbers by value, not lexically", () => {
    expect(compareNames("file2", "file10")).toBeLessThan(0);
    expect(compareNames("file10", "file2")).toBeGreaterThan(0);
  });

  it("is case-insensitive", () => {
    expect(compareNames("Apple", "apple")).toBe(0);
  });
});

describe("sortEntries", () => {
  it("sorts names in natural numeric order", () => {
    const es = [
      entry({ name: "file10.txt" }),
      entry({ name: "file2.txt" }),
      entry({ name: "file1.txt" }),
    ];
    expect(names(sortEntries(es, "name", "asc"))).toEqual([
      "file1.txt",
      "file2.txt",
      "file10.txt",
    ]);
  });

  it("keeps directories before files regardless of name", () => {
    const es = [
      entry({ name: "zeta.txt", is_dir: false }),
      entry({ name: "alpha", is_dir: true }),
    ];
    expect(names(sortEntries(es, "name", "asc"))).toEqual(["alpha", "zeta.txt"]);
  });

  it("does NOT flip the directories-first rule when descending", () => {
    const es = [
      entry({ name: "afile.txt", is_dir: false }),
      entry({ name: "zdir", is_dir: true }),
    ];
    // Descending by name, but the folder still leads.
    expect(names(sortEntries(es, "name", "desc"))).toEqual(["zdir", "afile.txt"]);
  });

  it("sorts by size with a natural-name tiebreaker", () => {
    const es = [
      entry({ name: "b", size: 100 }),
      entry({ name: "a", size: 100 }),
      entry({ name: "c", size: 50 }),
    ];
    expect(names(sortEntries(es, "size", "asc"))).toEqual(["c", "a", "b"]);
  });

  it("sorts by modified time, treating null as 0", () => {
    const es = [
      entry({ name: "new", modified: 2000 }),
      entry({ name: "old", modified: 1000 }),
      entry({ name: "unknown", modified: null }),
    ];
    expect(names(sortEntries(es, "modified", "asc"))).toEqual([
      "unknown",
      "old",
      "new",
    ]);
  });

  it("breaks a modified-time tie with natural name order (CPE-612)", () => {
    const es = [
      entry({ name: "file10", modified: 5000 }),
      entry({ name: "file2", modified: 5000 }),
      entry({ name: "file1", modified: 5000 }),
    ];
    // Same timestamp → deterministic natural-name order (file1 < file2 < file10), not backend order.
    expect(names(sortEntries(es, "modified", "asc"))).toEqual(["file1", "file2", "file10"]);
  });

  it("reverses order for descending", () => {
    const es = [entry({ name: "a" }), entry({ name: "b" }), entry({ name: "c" })];
    expect(names(sortEntries(es, "name", "desc"))).toEqual(["c", "b", "a"]);
  });

  it("does not mutate the input array", () => {
    const es = [entry({ name: "b" }), entry({ name: "a" })];
    const before = names(es);
    sortEntries(es, "name", "asc");
    expect(names(es)).toEqual(before);
  });
});

describe("compareEntries", () => {
  it("returns 0 for equal names", () => {
    expect(compareEntries(entry({ name: "same" }), entry({ name: "same" }), "name", "asc")).toBe(0);
  });
});

describe("foldersFirst toggle (CPE-359)", () => {
  const items = [
    entry({ name: "banana.txt", is_dir: false }),
    entry({ name: "Apples", is_dir: true }),
    entry({ name: "cherry.md", is_dir: false }),
  ];

  it("floats folders above files by default", () => {
    expect(names(sortEntries(items, "name", "asc"))).toEqual(["Apples", "banana.txt", "cherry.md"]);
  });

  it("interleaves folders and files alphabetically when foldersFirst is off", () => {
    expect(names(sortEntries(items, "name", "asc", false))).toEqual(["Apples", "banana.txt", "cherry.md"]);
    // A folder that sorts later stays later when mixed:
    const mixed = [
      entry({ name: "zeta", is_dir: true }),
      entry({ name: "alpha.txt", is_dir: false }),
    ];
    expect(names(sortEntries(mixed, "name", "asc", false))).toEqual(["alpha.txt", "zeta"]);
    // …but with foldersFirst on, the folder leads regardless of name:
    expect(names(sortEntries(mixed, "name", "asc", true))).toEqual(["zeta", "alpha.txt"]);
  });
});
