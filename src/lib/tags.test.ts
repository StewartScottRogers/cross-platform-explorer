import { describe, it, expect, vi, beforeEach } from "vitest";
import { get } from "svelte/store";
import {
  entryFor,
  hasTag,
  allTags,
  labelColor,
  LABEL_COLORS,
  tags as tagStore,
  nativeTagStoreName,
  pullNativeTags,
  pushNativeTags,
  type TagStore,
} from "./tags";

// The typed `commands.*` client routes through `./invoke`; mock it so the native-sync functions
// (CPE-828) drive off predictable command results.
const invokeMock = vi.fn(async (_cmd: string, _args?: unknown): Promise<unknown> => null);
vi.mock("./invoke", () => ({
  invoke: (...a: unknown[]) => (invokeMock as (...x: unknown[]) => unknown)(...a),
  unwrap: <T>(r: { status: string; data?: T; error?: unknown }): T => {
    if (r.status === "ok") return r.data as T;
    throw r.error instanceof Error ? r.error : new Error(String(r.error));
  },
}));

const store: TagStore = {
  "/a/one": { tags: ["work", "urgent"], label: "red" },
  "/a/two": { tags: ["home", "work"], label: "" },
};

describe("tags helpers (CPE-636)", () => {
  it("entryFor returns the present entry and an empty entry for a missing path", () => {
    expect(entryFor(store, "/a/one")).toEqual({ tags: ["work", "urgent"], label: "red" });
    expect(entryFor(store, "/a/missing")).toEqual({ tags: [], label: "" });
  });

  it("entryFor is null-safe — an absent/empty store yields an empty entry (CPE-638)", () => {
    // The row renderer calls entryFor($tags, path) on every entry; the store must never crash it,
    // even before the tag store has loaded.
    expect(entryFor(undefined as unknown as TagStore, "/a/one")).toEqual({ tags: [], label: "" });
    expect(entryFor({}, "/a/one")).toEqual({ tags: [], label: "" });
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

describe("native tag sync (CPE-828)", () => {
  beforeEach(() => invokeMock.mockReset());

  it("nativeTagStoreName returns the OS store's display name", async () => {
    invokeMock.mockImplementation(async (cmd) => (cmd === "native_tags_name" ? "NTFS alternate data streams" : null));
    expect(await nativeTagStoreName()).toBe("NTFS alternate data streams");
    expect(invokeMock).toHaveBeenCalledWith("native_tags_name");
  });

  it("pullNativeTags calls the command and updates the store from the returned whole store", async () => {
    const returned: TagStore = { "/f": { tags: ["report", "q3"], label: "red" } };
    invokeMock.mockImplementation(async (cmd) => (cmd === "native_tags_pull" ? returned : null));
    await pullNativeTags("/f");
    expect(invokeMock).toHaveBeenCalledWith("native_tags_pull", { path: "/f" });
    expect(get(tagStore)["/f"]).toEqual({ tags: ["report", "q3"], label: "red" });
  });

  it("pushNativeTags calls the push command for the path", async () => {
    invokeMock.mockImplementation(async () => null);
    await pushNativeTags("/f");
    expect(invokeMock).toHaveBeenCalledWith("native_tags_push", { path: "/f" });
  });
});
