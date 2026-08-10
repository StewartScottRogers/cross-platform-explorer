import { describe, it, expect, beforeEach } from "vitest";
import { get } from "svelte/store";
import {
  addEntries, removeEntry, type DropStackEntry,
  dropStackEntries, initDropStack, addToDropStack, removeFromDropStack, clearDropStack,
} from "./dropStack";
import { loadDropStack, saveDropStack } from "./settings";

const e = (path: string, addedFrom: string, addedAt: number): DropStackEntry => ({ path, addedFrom, addedAt });

describe("addEntries (CPE-1530)", () => {
  it("appends new entries to the end, in the order given", () => {
    let l: DropStackEntry[] = [];
    l = addEntries(l, ["/a.txt"], "/src", 100);
    l = addEntries(l, ["/b.txt", "/c.txt"], "/other", 200);
    expect(l.map((x) => x.path)).toEqual(["/a.txt", "/b.txt", "/c.txt"]);
    expect(l[0]).toEqual({ path: "/a.txt", addedFrom: "/src", addedAt: 100 });
    expect(l[2]).toEqual({ path: "/c.txt", addedFrom: "/other", addedAt: 200 });
  });

  it("de-dupes by path: re-adding an already-shelved path refreshes it and moves it to the end", () => {
    let l: DropStackEntry[] = [e("/a.txt", "/src", 100), e("/b.txt", "/src", 100)];
    l = addEntries(l, ["/a.txt"], "/elsewhere", 300);
    expect(l).toHaveLength(2);
    expect(l.map((x) => x.path)).toEqual(["/b.txt", "/a.txt"]); // bumped to the end
    expect(l[1]).toEqual({ path: "/a.txt", addedFrom: "/elsewhere", addedAt: 300 });
  });

  it("collapses duplicate paths WITHIN a single call to one entry (last occurrence wins)", () => {
    const l = addEntries([], ["/a.txt", "/a.txt"], "/src", 100);
    expect(l).toHaveLength(1);
    expect(l[0].path).toBe("/a.txt");
  });

  it("is a no-op for an empty paths array and does not mutate the input", () => {
    const l = [e("/a.txt", "/src", 100)];
    expect(addEntries(l, [], "/src")).toBe(l);
    expect(l).toHaveLength(1);
  });

  it("does not mutate the input list on a normal add", () => {
    const l = [e("/a.txt", "/src", 100)];
    addEntries(l, ["/b.txt"], "/src", 200);
    expect(l).toHaveLength(1);
  });

  it("defaults `now` to the current time when omitted", () => {
    const before = Date.now();
    const l = addEntries([], ["/a.txt"], "/src");
    const after = Date.now();
    expect(l[0].addedAt).toBeGreaterThanOrEqual(before);
    expect(l[0].addedAt).toBeLessThanOrEqual(after);
  });
});

describe("removeEntry (CPE-1530)", () => {
  it("drops only the matching path, keeping the rest in order", () => {
    const l = [e("/a.txt", "/src", 1), e("/b.txt", "/src", 2), e("/c.txt", "/src", 3)];
    expect(removeEntry(l, "/b.txt").map((x) => x.path)).toEqual(["/a.txt", "/c.txt"]);
  });

  it("is a no-op when the path is absent, and does not mutate the input", () => {
    const l = [e("/a.txt", "/src", 1)];
    expect(removeEntry(l, "/z.txt")).toEqual(l);
    expect(l).toHaveLength(1);
  });

  it("removing the only entry empties the list", () => {
    expect(removeEntry([e("/a.txt", "/src", 1)], "/a.txt")).toEqual([]);
  });
});

describe("empty state", () => {
  it("a fresh reducer chain starts empty", () => {
    expect(addEntries([], [], "/src")).toEqual([]);
    expect(removeEntry([], "/anything")).toEqual([]);
  });
});

describe("settings.ts persistence (dropStack field, CPE-1530)", () => {
  it("defaults to an empty stack when nothing is saved", () => {
    expect(loadDropStack()).toEqual([]);
  });

  it("round-trips a saved stack through save/load", () => {
    const stack = [e("/a.txt", "/src", 100), e("/b.txt", "/other", 200)];
    saveDropStack(stack);
    expect(loadDropStack()).toEqual(stack);
    saveDropStack([]); // reset for other tests
  });

  it("degrades a corrupt/hand-edited entry to empty rather than crashing", () => {
    saveDropStack([{ bogus: 1 } as unknown as DropStackEntry]);
    expect(loadDropStack()).toEqual([]);
    saveDropStack([]); // reset
  });

  it("rejects an entry with an empty path", () => {
    saveDropStack([{ path: "", addedFrom: "/src", addedAt: 1 }]);
    expect(loadDropStack()).toEqual([]);
    saveDropStack([]); // reset
  });
});

describe("reactive store (CPE-1530)", () => {
  beforeEach(() => {
    saveDropStack([]);
    clearDropStack();
  });

  it("starts empty before init", () => {
    expect(get(dropStackEntries)).toEqual([]);
  });

  it("initDropStack() loads whatever was persisted (first call in the suite — the guard is idempotent)", () => {
    saveDropStack([e("/a.txt", "/src", 100)]);
    initDropStack();
    expect(get(dropStackEntries).map((x) => x.path)).toEqual(["/a.txt"]);
    clearDropStack();
    saveDropStack([]);
  });

  it("addToDropStack adds a single path and persists it", () => {
    addToDropStack("/a.txt", "/src");
    expect(get(dropStackEntries).map((x) => x.path)).toEqual(["/a.txt"]);
    expect(loadDropStack().map((x) => x.path)).toEqual(["/a.txt"]);
  });

  it("addToDropStack accepts multiple paths from the same source folder", () => {
    addToDropStack(["/a.txt", "/b.txt"], "/src");
    expect(get(dropStackEntries).map((x) => x.path)).toEqual(["/a.txt", "/b.txt"]);
    expect(get(dropStackEntries).every((x) => x.addedFrom === "/src")).toBe(true);
  });

  it("re-adding an already-shelved path bumps it to the end and persists the refresh", () => {
    addToDropStack("/a.txt", "/src");
    addToDropStack("/b.txt", "/src");
    addToDropStack("/a.txt", "/elsewhere");
    const list = get(dropStackEntries);
    expect(list.map((x) => x.path)).toEqual(["/b.txt", "/a.txt"]);
    expect(list[1].addedFrom).toBe("/elsewhere");
    expect(loadDropStack().map((x) => x.path)).toEqual(["/b.txt", "/a.txt"]);
  });

  it("removeFromDropStack drops one path and persists it", () => {
    addToDropStack(["/a.txt", "/b.txt"], "/src");
    removeFromDropStack("/a.txt");
    expect(get(dropStackEntries).map((x) => x.path)).toEqual(["/b.txt"]);
    expect(loadDropStack().map((x) => x.path)).toEqual(["/b.txt"]);
  });

  it("clearDropStack empties both the reactive store and the persisted value", () => {
    addToDropStack(["/a.txt", "/b.txt"], "/src");
    clearDropStack();
    expect(get(dropStackEntries)).toEqual([]);
    expect(loadDropStack()).toEqual([]);
  });
});
