import { describe, it, expect } from "vitest";
import {
  evaluateSavedSearch,
  matchesSavedSearch,
  serializeSavedSearch,
  parseSavedSearch,
  flattenTree,
  type SavedSearch,
} from "./savedSearch";
import type { Condition } from "./colorRules";
import type { DirEntry } from "./types";
import type { TreeNode } from "./bindings.gen";

const NOW = 1_700_000_000_000;
const DAY_MS = 86_400_000;
const f = (name: string, over: Partial<DirEntry> = {}): DirEntry =>
  ({ name, path: "/x/" + name, is_dir: false, size: 10, modified: NOW, extension: "", ...(over as object) }) as DirEntry;

const search = (over: Partial<SavedSearch> = {}): SavedSearch => ({
  id: "s1",
  name: "My search",
  conditions: [],
  match: "all",
  ...over,
});

describe("evaluateSavedSearch (CPE-986)", () => {
  const png: Condition = { kind: "ext", exts: ["png"] };
  const big: Condition = { kind: "size", min: 1000 };
  const entries = [
    f("a.png", { size: 10 }),
    f("b.txt", { size: 10 }),
    f("big.png", { size: 9999 }),
    f("big.bin", { size: 9999 }),
  ];

  it("match 'all' requires every condition (AND)", () => {
    const s = search({ conditions: [png, big], match: "all" });
    expect(evaluateSavedSearch(entries, s, NOW).map((e) => e.name)).toEqual(["big.png"]);
  });

  it("match 'any' requires at least one condition (OR)", () => {
    const s = search({ conditions: [png, big], match: "any" });
    expect(evaluateSavedSearch(entries, s, NOW).map((e) => e.name)).toEqual(["a.png", "big.png", "big.bin"]);
  });

  it("returns [] when a search matches none", () => {
    const s = search({ conditions: [{ kind: "ext", exts: ["gif"] }], match: "all" });
    expect(evaluateSavedSearch(entries, s, NOW)).toEqual([]);
  });

  it("empty conditions: 'all' matches everything, 'any' matches nothing", () => {
    expect(evaluateSavedSearch(entries, search({ conditions: [], match: "all" }), NOW)).toHaveLength(4);
    expect(evaluateSavedSearch(entries, search({ conditions: [], match: "any" }), NOW)).toEqual([]);
  });

  it("composes with a real date Condition through the reused matcher", () => {
    // Prove reuse of matchesCondition: an ext + a newerThan condition together.
    const recent = f("fresh.log", { name: "fresh.log", modified: NOW - DAY_MS }); // 1 day old
    const stale = f("old.log", { name: "old.log", modified: NOW - 40 * DAY_MS }); // 40 days old
    const s = search({
      conditions: [
        { kind: "ext", exts: ["log"] },
        { kind: "newerThan", days: 7 },
      ],
      match: "all",
    });
    expect(matchesSavedSearch(recent, s, NOW)).toBe(true);
    expect(matchesSavedSearch(stale, s, NOW)).toBe(false); // right ext, but too old
  });
});

describe("serializeSavedSearch / parseSavedSearch (CPE-986)", () => {
  it("round-trips through JSON", () => {
    const s = search({
      id: "abc",
      name: "Recent images",
      conditions: [{ kind: "ext", exts: ["png", "jpg"] }],
      match: "any",
    });
    const round = parseSavedSearch(serializeSavedSearch(s));
    expect(round).toEqual(s);
  });

  it("returns null (no throw) on malformed JSON", () => {
    expect(parseSavedSearch("{not json")).toBeNull();
    expect(parseSavedSearch("")).toBeNull();
  });

  it("returns null on a missing or blank name", () => {
    expect(parseSavedSearch(JSON.stringify(search({ name: "" })))).toBeNull();
    expect(parseSavedSearch(JSON.stringify(search({ name: "   " })))).toBeNull();
    const { name: _omit, ...noName } = search();
    expect(parseSavedSearch(JSON.stringify(noName))).toBeNull();
  });

  it("returns null on an invalid match or a corrupted condition", () => {
    expect(parseSavedSearch(JSON.stringify({ ...search(), match: "some" }))).toBeNull();
    // { kind: "ext" } with no `exts` is a landmine that would later throw in matchesCondition.
    expect(parseSavedSearch(JSON.stringify({ ...search(), conditions: [{ kind: "ext" }] }))).toBeNull();
  });

  it("round-trips a captured `root` (CPE-1229) and rejects a non-string root", () => {
    const withRoot = search({ root: "Z:\\repos\\project" });
    expect(parseSavedSearch(serializeSavedSearch(withRoot))).toEqual(withRoot);
    // Older-shaped data with no root at all still parses (root stays optional/omittable).
    expect(parseSavedSearch(serializeSavedSearch(search()))?.root).toBeUndefined();
    expect(parseSavedSearch(JSON.stringify({ ...search(), root: 42 }))).toBeNull();
  });
});

// CPE-1229 open-evaluator: there's no whole-computer index, so opening a structured saved search scans
// recursively from its captured `root` via the existing `commands.scanTree` and flattens the result into
// a `DirEntry[]` that runs through the SAME `evaluateSavedSearch`/`matchesCondition` as any other listing
// — no parallel matcher. `flattenTree` is the pure, DOM-free piece of that wiring (App.svelte owns the
// `scanTree` IPC call itself); these tests prove it produces entries the real matcher can actually use.
describe("flattenTree (CPE-1229)", () => {
  const file = (name: string, size: number, modified: number | null = NOW): TreeNode => ({
    name,
    isDir: false,
    size,
    modified,
  });
  const dir = (name: string, children: TreeNode[]): TreeNode => ({ name, isDir: true, children });

  it("joins root + name into a real path, recursing into subfolders", () => {
    const tree: TreeNode[] = [file("a.txt", 5), dir("sub", [file("b.txt", 2)])];
    const flat = flattenTree(tree, "Z:\\repos\\proj");
    expect(flat.map((e) => e.path)).toEqual([
      "Z:\\repos\\proj\\a.txt",
      "Z:\\repos\\proj\\sub",
      "Z:\\repos\\proj\\sub\\b.txt",
    ]);
    // A forward-slash root stays forward-slash (matches whichever separator the root already uses).
    expect(flattenTree(tree, "/home/me")[0].path).toBe("/home/me/a.txt");
  });

  it("maps size/modified/is_dir straight through and derives a lowercase extension", () => {
    const tree: TreeNode[] = [file("Photo.PNG", 9999, NOW), dir("Empty", [])];
    const flat = flattenTree(tree, "/root");
    const photo = flat.find((e) => e.name === "Photo.PNG")!;
    expect(photo).toMatchObject({ is_dir: false, size: 9999, modified: NOW, extension: "png" });
    const empty = flat.find((e) => e.name === "Empty")!;
    expect(empty).toMatchObject({ is_dir: true, size: 0, modified: null, extension: "" });
  });

  it("flattened entries evaluate through the real matcher (the open-evaluator's actual wiring)", () => {
    const tree: TreeNode[] = [
      file("keep.png", 9999),
      file("skip.txt", 9999),
      dir("nested", [file("also-keep.png", 1)]),
    ];
    const s = search({ conditions: [{ kind: "ext", exts: ["png"] }], match: "all" });
    const matches = evaluateSavedSearch(flattenTree(tree, "/root"), s, NOW);
    expect(matches.map((e) => e.path)).toEqual(["/root/keep.png", "/root/nested/also-keep.png"]);
  });
});
