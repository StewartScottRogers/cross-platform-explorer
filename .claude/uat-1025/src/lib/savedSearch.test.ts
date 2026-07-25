import { describe, it, expect } from "vitest";
import {
  evaluateSavedSearch,
  matchesSavedSearch,
  serializeSavedSearch,
  parseSavedSearch,
  type SavedSearch,
} from "./savedSearch";
import type { Condition } from "./colorRules";
import type { DirEntry } from "./types";

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
});
