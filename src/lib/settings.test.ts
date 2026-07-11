import { describe, it, expect } from "vitest";
import { addRecent, togglePin } from "./settings";
import type { RecentFile } from "./types";

const r = (path: string, opened: number): RecentFile => ({
  path,
  name: path.split("/").pop() ?? path,
  opened,
});

describe("addRecent", () => {
  it("puts the newest entry first", () => {
    const list = addRecent([], { path: "/a.txt", name: "a.txt" }, 100);
    expect(list[0].path).toBe("/a.txt");
    expect(list[0].opened).toBe(100);
  });

  it("de-duplicates by path and moves the entry to the front", () => {
    let list = [r("/a.txt", 1), r("/b.txt", 2)];
    list = addRecent(list, { path: "/b.txt", name: "b.txt" }, 300);
    expect(list.map((x) => x.path)).toEqual(["/b.txt", "/a.txt"]);
    expect(list).toHaveLength(2);
    expect(list[0].opened).toBe(300);
  });

  it("caps the list so it cannot grow without bound", () => {
    let list: RecentFile[] = [];
    for (let i = 0; i < 40; i++) {
      list = addRecent(list, { path: `/f${i}.txt`, name: `f${i}.txt` }, i);
    }
    expect(list).toHaveLength(20);
    expect(list[0].path).toBe("/f39.txt"); // newest retained
    expect(list.some((x) => x.path === "/f0.txt")).toBe(false); // oldest evicted
  });
});

describe("togglePin", () => {
  it("adds a pin when absent", () => {
    expect(togglePin([], "/a")).toEqual(["/a"]);
  });

  it("removes a pin when present", () => {
    expect(togglePin(["/a", "/b"], "/a")).toEqual(["/b"]);
  });

  it("does not mutate the input", () => {
    const pins = ["/a"];
    togglePin(pins, "/b");
    expect(pins).toEqual(["/a"]);
  });
});
