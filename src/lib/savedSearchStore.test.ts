import { describe, it, expect, beforeEach, vi } from "vitest";
import { get } from "svelte/store";
import {
  parseSavedSearches,
  serializeSavedSearches,
  addSavedSearchTo,
  renameSavedSearchIn,
  removeSavedSearchFrom,
} from "./savedSearchStore";
import type { SavedSearch } from "./savedSearch";
import type { Condition } from "./colorRules";

const png: Condition = { kind: "ext", exts: ["png"] };

const search = (over: Partial<SavedSearch> = {}): SavedSearch => ({
  id: "s1",
  name: "My search",
  conditions: [png],
  match: "all",
  ...over,
});

describe("savedSearchStore CRUD (CPE-1228)", () => {
  it("addSavedSearchTo appends a search with a generated id, trimmed name, immutably", () => {
    const before: SavedSearch[] = [];
    const after = addSavedSearchTo(before, "  Big PNGs  ", [png], "all");
    expect(before).toEqual([]); // original untouched
    expect(after).toHaveLength(1);
    expect(after[0].id).toMatch(/^ss_/);
    expect(after[0].name).toBe("Big PNGs");
    expect(after[0].conditions).toEqual([png]);
    expect(after[0].match).toBe("all");
  });

  it("addSavedSearchTo is a no-op on a blank name", () => {
    expect(addSavedSearchTo([], "", [png], "all")).toEqual([]);
    expect(addSavedSearchTo([], "   ", [png], "any")).toEqual([]);
  });

  it("addSavedSearchTo captures an optional root (CPE-1229 'Save search…')", () => {
    const after = addSavedSearchTo([], "Big PNGs", [png], "all", "Z:\\repos\\project");
    expect(after[0].root).toBe("Z:\\repos\\project");
  });

  it("addSavedSearchTo omits root entirely when not given or blank (back-compat shape)", () => {
    expect(addSavedSearchTo([], "No root", [png], "all")).toHaveLength(1);
    expect(addSavedSearchTo([], "No root", [png], "all")[0].root).toBeUndefined();
    expect(addSavedSearchTo([], "Blank root", [png], "all", "   ")[0].root).toBeUndefined();
  });

  it("renameSavedSearchIn renames by id, ignoring blank/unknown", () => {
    const list = [search({ id: "a", name: "Old" })];
    expect(renameSavedSearchIn(list, "a", "New")[0].name).toBe("New");
    expect(renameSavedSearchIn(list, "a", "  ")[0].name).toBe("Old"); // blank ignored
    expect(renameSavedSearchIn(list, "zzz", "New")[0].name).toBe("Old"); // unknown id
  });

  it("removeSavedSearchFrom drops by id", () => {
    const list = [search({ id: "a", name: "A" }), search({ id: "b", name: "B" })];
    expect(removeSavedSearchFrom(list, "a")).toEqual([list[1]]);
    expect(removeSavedSearchFrom(list, "zzz")).toHaveLength(2);
  });
});

describe("savedSearchStore serialize/parse (CPE-1228)", () => {
  it("round-trips a list through serialize/parse", () => {
    const list = [search({ id: "a" }), search({ id: "b", name: "Other", match: "any" })];
    expect(parseSavedSearches(serializeSavedSearches(list))).toEqual(list);
  });

  it("tolerates null/garbage input, returning []", () => {
    expect(parseSavedSearches(null)).toEqual([]);
    expect(parseSavedSearches("not json")).toEqual([]);
    expect(parseSavedSearches(JSON.stringify({ not: "an array" }))).toEqual([]);
    expect(parseSavedSearches("{bad json")).toEqual([]);
  });

  it("drops malformed entries while keeping valid ones", () => {
    const good = search({ id: "good" });
    const mixed = JSON.stringify([
      good,
      { id: "no-name" }, // missing name
      { id: "bad-match", name: "x", conditions: [], match: "sometimes" }, // invalid match
      { id: "bad-cond", name: "x", conditions: [{ kind: "ext" }], match: "all" }, // ext with no exts
      42,
      null,
    ]);
    expect(parseSavedSearches(mixed)).toEqual([good]);
  });
});

// The module-level `savedSearches` writable store: init-from-storage, persist-on-change, and a
// re-init round-trip (write → fresh module load → same data), reusing the vitest-provided jsdom
// localStorage. Each test resets modules so the singleton store is freshly constructed from whatever
// is in storage at that point, mirroring how the app boots.
describe("savedSearches writable store (CPE-1228)", () => {
  const KEY = "cpe.savedSearches";

  // The store is a module-level singleton, constructed once at import time from whatever is in
  // localStorage. `vi.resetModules()` forces the NEXT dynamic import to re-evaluate the module from
  // scratch (simulating an app restart), so each test/re-init sees a fresh construction against the
  // storage state at that moment.
  beforeEach(() => {
    localStorage.clear();
    vi.resetModules();
  });

  it("initializes empty with no persisted data", async () => {
    const { savedSearches } = await import("./savedSearchStore");
    expect(get(savedSearches)).toEqual([]);
  });

  it("initializes from a valid persisted list", async () => {
    localStorage.setItem(KEY, JSON.stringify([search({ id: "p1", name: "Persisted" })]));
    const { savedSearches } = await import("./savedSearchStore");
    expect(get(savedSearches)).toEqual([search({ id: "p1", name: "Persisted" })]);
  });

  it("tolerates malformed persisted data at init, never throwing", async () => {
    localStorage.setItem(KEY, "{not valid json");
    const { savedSearches } = await import("./savedSearchStore");
    expect(get(savedSearches)).toEqual([]);
  });

  it("addSavedSearch (module helper) threads the captured root through to persistence", async () => {
    const mod = await import("./savedSearchStore");
    mod.addSavedSearch("Rooted", [png], "all", "Z:\\repos\\project");
    const persisted = parseSavedSearches(localStorage.getItem(KEY));
    expect(persisted[0].root).toBe("Z:\\repos\\project");
  });

  it("add/rename/remove helpers persist every change to localStorage", async () => {
    const mod = await import("./savedSearchStore");
    mod.addSavedSearch("First", [png], "all");
    let persisted = parseSavedSearches(localStorage.getItem(KEY));
    expect(persisted).toHaveLength(1);
    expect(persisted[0].name).toBe("First");
    const id = persisted[0].id;

    mod.renameSavedSearch(id, "Renamed");
    persisted = parseSavedSearches(localStorage.getItem(KEY));
    expect(persisted[0].name).toBe("Renamed");

    mod.removeSavedSearch(id);
    persisted = parseSavedSearches(localStorage.getItem(KEY));
    expect(persisted).toEqual([]);
  });

  it("persistence round-trip: write via the store, then a fresh module load sees the same data", async () => {
    const mod = await import("./savedSearchStore");
    mod.addSavedSearch("Round trip", [png], "any");
    const before = get(mod.savedSearches);

    // Simulate an app restart: reset the module registry and re-import so the module-level store
    // re-initializes straight from whatever is now in localStorage.
    vi.resetModules();
    const { savedSearches: reloaded } = await import("./savedSearchStore");
    expect(get(reloaded)).toEqual(before);
  });
});
