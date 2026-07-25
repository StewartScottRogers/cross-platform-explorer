import { describe, it, expect } from "vitest";
import {
  addRecent, removeRecent, togglePin, toggleFavorite, mergeLegacy,
  loadAutoRestore, saveAutoRestore, loadLastSession, saveLastSession,
} from "./settings";
import type { RecentFile, Favorite } from "./types";
import type { WorkspaceTab } from "./workspaces";

describe("auto-restore session settings (CPE-789)", () => {
  it("defaults to off with no saved session", () => {
    expect(loadAutoRestore()).toBe(false);
    expect(loadLastSession()).toEqual([]);
  });

  it("round-trips the flag and the captured tabs", () => {
    saveAutoRestore(true);
    expect(loadAutoRestore()).toBe(true);
    const tabs: WorkspaceTab[] = [{ path: "/a", view: "list" }, { path: "/b" }];
    saveLastSession(tabs);
    expect(loadLastSession()).toEqual(tabs);
    saveAutoRestore(false); // reset module state for other tests
  });

  it("drops corrupt tabs from a persisted session (tolerant parse)", () => {
    saveLastSession([{ path: "/ok" }, { path: "" }, { bogus: 1 } as unknown as WorkspaceTab]);
    expect(loadLastSession()).toEqual([{ path: "/ok" }]);
    saveLastSession([]); // reset
  });
});

describe("mergeLegacy (localStorage → settings.json migration, CPE-226)", () => {
  it("backfills keys the file lacks from legacy localStorage values", () => {
    const ls: Record<string, string> = {
      "cpe.view": JSON.stringify("list"),
      "cpe.sidebarWidth": JSON.stringify(260),
    };
    const merged = mergeLegacy({}, (k) => ls[k] ?? null);
    expect(merged["cpe.view"]).toBe("list");
    expect(merged["cpe.sidebarWidth"]).toBe(260);
  });

  it("lets the file win over localStorage for keys it already has", () => {
    const ls: Record<string, string> = { "cpe.view": JSON.stringify("icons") };
    const merged = mergeLegacy({ "cpe.view": "details" }, (k) => ls[k] ?? null);
    expect(merged["cpe.view"]).toBe("details");
  });

  it("ignores an unparseable legacy value", () => {
    const merged = mergeLegacy({}, (k) => (k === "cpe.view" ? "not json" : null));
    expect("cpe.view" in merged).toBe(false);
  });
});

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

describe("removeRecent (CPE-341)", () => {
  it("drops only the matching path and keeps the rest in order", () => {
    const list = [r("/a.txt", 3), r("/b.txt", 2), r("/c.txt", 1)];
    expect(removeRecent(list, "/b.txt").map((x) => x.path)).toEqual(["/a.txt", "/c.txt"]);
  });

  it("is a no-op when the path is absent, and does not mutate the input", () => {
    const list = [r("/a.txt", 1)];
    expect(removeRecent(list, "/z.txt")).toEqual(list);
    expect(list.map((x) => x.path)).toEqual(["/a.txt"]);
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

describe("toggleFavorite (CPE-338)", () => {
  const file = { path: "/a.txt", name: "a.txt", is_dir: false };
  const dir = { path: "/docs", name: "docs", is_dir: true };

  it("adds a favorite (file or folder) when absent, preserving is_dir", () => {
    const list = toggleFavorite([], dir);
    expect(list).toEqual([{ path: "/docs", name: "docs", is_dir: true }]);
    expect(toggleFavorite(list, file).map((f) => f.path)).toEqual(["/docs", "/a.txt"]);
  });

  it("removes a favorite when the path is already present", () => {
    const list: Favorite[] = [
      { path: "/docs", name: "docs", is_dir: true },
      { path: "/a.txt", name: "a.txt", is_dir: false },
    ];
    expect(toggleFavorite(list, dir).map((f) => f.path)).toEqual(["/a.txt"]);
  });

  it("does not mutate the input", () => {
    const list: Favorite[] = [{ path: "/a.txt", name: "a.txt", is_dir: false }];
    toggleFavorite(list, dir);
    expect(list).toEqual([{ path: "/a.txt", name: "a.txt", is_dir: false }]);
  });
});
