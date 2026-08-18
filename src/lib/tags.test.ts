import { describe, it, expect, vi, beforeEach } from "vitest";
import { get } from "svelte/store";
import {
  entryFor,
  keyToWrite,
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

  describe("entryFor — CPE-1737 round 2 (trailing-slash fallback)", () => {
    it("falls back to a canonical-path match when the exact key misses", () => {
      // `set_tags` has no `require_local`, so a remote folder is genuinely taggable. A directory's
      // listing-row path now legitimately carries a trailing '/' (CPE-1737 round 1); a lookup from a
      // different route (toolbar/sidebar) would not reproduce that shape.
      const remoteStore: TagStore = { "sftp://h/srv/sub": { tags: ["x"], label: "blue" } };
      expect(entryFor(remoteStore, "sftp://h/srv/sub/")).toEqual({ tags: ["x"], label: "blue" });
    });

    it("prefers an exact match over the fallback scan when both could apply", () => {
      const remoteStore: TagStore = {
        "sftp://h/srv/sub": { tags: ["bare"], label: "" },
        "sftp://h/srv/sub/": { tags: ["slashed"], label: "" },
      };
      expect(entryFor(remoteStore, "sftp://h/srv/sub")).toEqual({ tags: ["bare"], label: "" });
      expect(entryFor(remoteStore, "sftp://h/srv/sub/")).toEqual({ tags: ["slashed"], label: "" });
    });

    it("never mutates the store's own keys — only the LOOKUP is canonicalised", () => {
      const remoteStore: TagStore = { "sftp://h/srv/sub": { tags: ["x"], label: "" } };
      entryFor(remoteStore, "sftp://h/srv/sub/");
      expect(Object.keys(remoteStore)).toEqual(["sftp://h/srv/sub"]);
    });
  });

  describe("keyToWrite — CPE-1737 round 3 (the write-side duplicate the round-2 review found)", () => {
    it("reuses an existing entry's key when the exact path already has one", () => {
      const remoteStore: TagStore = { "sftp://h/srv/sub": { tags: ["x"], label: "" } };
      expect(keyToWrite(remoteStore, "sftp://h/srv/sub")).toBe("sftp://h/srv/sub");
    });

    it("reuses an existing entry's key found only by canonical match, instead of forking a second key", () => {
      // entryFor's fallback fixes LOOKUP; without this, tagging the folder again via its listing row
      // (trailing '/') would write a SECOND key beside the pre-upgrade one, so allTags/tagCounts would
      // double-count it and reaching the folder any other way would keep showing the stale entry.
      const remoteStore: TagStore = { "sftp://h/srv/sub": { tags: ["x"], label: "" } };
      expect(keyToWrite(remoteStore, "sftp://h/srv/sub/")).toBe("sftp://h/srv/sub");
    });

    it("falls back to the given path unchanged when nothing already matches (first-time tagging)", () => {
      expect(keyToWrite({}, "sftp://h/srv/new")).toBe("sftp://h/srv/new");
      expect(keyToWrite({ "/other": { tags: [], label: "" } }, "sftp://h/srv/new")).toBe("sftp://h/srv/new");
    });

    it("never rewrites a LOCAL Windows path's separators when nothing matches", () => {
      expect(keyToWrite({}, "C:\\Users\\me\\docs")).toBe("C:\\Users\\me\\docs");
    });

    it("is null-safe", () => {
      expect(keyToWrite(undefined as unknown as TagStore, "/a/one")).toBe("/a/one");
    });
  });

  describe("keyToWrite — CPE-1754 (a trailing backslash is a real POSIX filename char, not a separator)", () => {
    it("does NOT redirect a path ending in a literal backslash onto its non-backslash sibling", () => {
      // On Linux/macOS "/home/x\\" and "/home/x" are two distinct, unrelated paths (a backslash is a
      // legal filename character there). Stripping the trailing backslash before comparing would make
      // the write land on "/home/x"'s entry — silently overwriting a DIFFERENT file's tags. Assert the
      // no-redirect outcome by name, not just a string match, so a regression here reads as "redirected"
      // rather than an opaque toBe failure.
      const store: TagStore = { "/home/x": { tags: ["the-real-directory"], label: "" } };
      const written = keyToWrite(store, "/home/x\\");
      const redirectedOntoSibling = written === "/home/x";
      expect(redirectedOntoSibling, "keyToWrite redirected a path ending in '\\\\' onto its sibling '/home/x' — a POSIX filename's trailing backslash is being stripped as if it were a separator").toBe(false);
      expect(written).toBe("/home/x\\");
    });

    it("still redirects a genuine trailing FORWARD slash (the CPE-1737 remote-listing-row case)", () => {
      const remoteStore: TagStore = { "sftp://h/srv/sub": { tags: ["x"], label: "" } };
      expect(keyToWrite(remoteStore, "sftp://h/srv/sub/")).toBe("sftp://h/srv/sub");
    });

    it("preserves bare roots on all spellings — never strips '/', 'C:/' or 'C:\\\\' down to '' / 'C:'", () => {
      expect(keyToWrite({}, "/")).toBe("/");
      expect(keyToWrite({}, "C:/")).toBe("C:/");
      expect(keyToWrite({}, "C:\\")).toBe("C:\\");
    });

    it("first-time tagging still returns the caller's path unchanged, including a Windows path", () => {
      expect(keyToWrite({}, "C:\\Users\\me\\new-folder")).toBe("C:\\Users\\me\\new-folder");
      expect(keyToWrite({}, "/home/new-file")).toBe("/home/new-file");
    });
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
