import { describe, it, expect } from "vitest";
import {
  foldActivities,
  pruneActivities,
  recentActivities,
  mergeTimeline,
  affectsListing,
  folderHasActivity,
  folderHasActivityNorm,
  normalizeActivityPaths,
  ingestActivity,
  clearActivity,
  fsActivity,
  agentTimeline,
  ACTIVITY_TTL_MS,
  type AgentActivity,
  type TimelineEntry,
} from "./agentActivity";
import type { FsActivity } from "./sidecar";

const read = () => {
  let v: Record<string, AgentActivity> = {};
  fsActivity.subscribe((x) => (v = x))();
  return v;
};
const readTimeline = () => {
  let v: TimelineEntry[] = [];
  agentTimeline.subscribe((x) => (v = x))();
  return v;
};

describe("Agent Watch activity folding (CPE-399)", () => {
  it("folds a batch in, newest kind + timestamp winning per path", () => {
    const items: FsActivity[] = [
      { kind: "created", path: "/a" },
      { kind: "modified", path: "/b" },
    ];
    const m1 = foldActivities({}, items, 100);
    expect(m1).toEqual({ "/a": { kind: "created", at: 100 }, "/b": { kind: "modified", at: 100 } });
    const m2 = foldActivities(m1, [{ kind: "removed", path: "/a" }], 200);
    expect(m2["/a"]).toEqual({ kind: "removed", at: 200 }); // latest wins
  });

  it("prunes entries older than the TTL and keeps fresh ones", () => {
    const now = 10_000;
    const map = { "/old": { kind: "created" as const, at: now - ACTIVITY_TTL_MS - 1 }, "/new": { kind: "modified" as const, at: now - 10 } };
    const pruned = pruneActivities(map, now);
    expect(Object.keys(pruned)).toEqual(["/new"]);
    // returns the SAME reference when nothing expired (cheap no-op)
    const fresh = { "/x": { kind: "created" as const, at: now } };
    expect(pruneActivities(fresh, now)).toBe(fresh);
  });

  it("recentActivities returns newest-first, capped", () => {
    const map = { "/a": { kind: "created" as const, at: 1 }, "/b": { kind: "modified" as const, at: 3 }, "/c": { kind: "removed" as const, at: 2 } };
    expect(recentActivities(map, 2).map((r) => r.path)).toEqual(["/b", "/c"]);
  });

  it("ingestActivity updates the store and clearActivity empties it", () => {
    clearActivity();
    ingestActivity([{ kind: "created", path: "/store/a" }], 500);
    expect(read()["/store/a"]).toEqual({ kind: "created", at: 500 });
    ingestActivity("garbage");
    expect(read()["/store/a"]).toBeTruthy(); // malformed ignored, prior state intact
    clearActivity();
    expect(read()).toEqual({});
  });
});

describe("Agent Watch reads (CPE-405)", () => {
  it("ingests a read kind into the map and timeline", () => {
    clearActivity();
    ingestActivity([{ kind: "read", path: "/r/consulted.rs" }], 700);
    expect(read()["/r/consulted.rs"]).toEqual({ kind: "read", at: 700 });
    expect(readTimeline()[0]).toMatchObject({ kind: "read", path: "/r/consulted.rs" });
    clearActivity();
  });

  it("a read is the weakest signal — it never downgrades a mutation, but a mutation upgrades a read", () => {
    // read after modified: the modified annotation stays.
    const m1 = foldActivities({ "/f": { kind: "modified", at: 10 } }, [{ kind: "read", path: "/f" }], 20);
    expect(m1["/f"]).toEqual({ kind: "modified", at: 10 });
    // modified after read: the mutation wins and refreshes the timestamp.
    const m2 = foldActivities({ "/f": { kind: "read", at: 10 } }, [{ kind: "modified", path: "/f" }], 20);
    expect(m2["/f"]).toEqual({ kind: "modified", at: 20 });
    // read after read: just refreshes.
    const m3 = foldActivities({ "/f": { kind: "read", at: 10 } }, [{ kind: "read", path: "/f" }], 20);
    expect(m3["/f"]).toEqual({ kind: "read", at: 20 });
  });

  it("a read never triggers a re-list (membership is unchanged)", () => {
    expect(affectsListing([{ kind: "read", path: "Z:/repos/app/consulted.ts" }], "Z:/repos/app")).toBe(false);
  });

  it("unknown kinds are still dropped by normalization (graceful for other agents)", () => {
    clearActivity();
    ingestActivity([{ kind: "opened", path: "/x" }, { kind: "read", path: "/y" }]);
    expect(read()["/x"]).toBeUndefined();
    expect(read()["/y"]).toBeTruthy();
    clearActivity();
  });
});

describe("Agent Watch durable timeline (CPE-400)", () => {
  it("mergeTimeline prepends newest-first with unique ids, capped", () => {
    const items: FsActivity[] = [
      { kind: "created", path: "/a" },
      { kind: "modified", path: "/b" },
    ];
    const t1 = mergeTimeline([], items, 100, 0);
    expect(t1.map((e) => e.path)).toEqual(["/b", "/a"]); // last of batch is newest → first
    expect(t1.map((e) => e.id)).toEqual([1, 0]);
    const t2 = mergeTimeline(t1, [{ kind: "removed", path: "/c" }], 200, 2);
    expect(t2[0]).toMatchObject({ id: 2, kind: "removed", path: "/c", at: 200 });
    // cap keeps only the newest `cap` entries
    const big = Array.from({ length: 5 }, (_, i) => ({ kind: "created" as const, path: `/f${i}` }));
    expect(mergeTimeline([], big, 0, 0, 3)).toHaveLength(3);
  });

  it("ingestActivity records to the timeline; clearActivity empties it", () => {
    clearActivity();
    ingestActivity([{ kind: "created", path: "/t/a" }, { kind: "modified", path: "/t/b" }], 1);
    ingestActivity([{ kind: "removed", path: "/t/a" }], 2);
    const tl = readTimeline();
    expect(tl.map((e) => e.path)).toEqual(["/t/a", "/t/b", "/t/a"]); // newest-first, history preserved
    expect(new Set(tl.map((e) => e.id)).size).toBe(3); // ids stay unique across batches
    clearActivity();
    expect(readTimeline()).toEqual([]);
  });
});

describe("affectsListing (CPE-401 — should we re-list the folder)", () => {
  const folder = "Z:/repos/app";
  it("true when a direct child is created/removed/renamed (cross-platform)", () => {
    expect(affectsListing([{ kind: "created", path: "Z:\\repos\\app\\new.ts" }], folder)).toBe(true);
    expect(affectsListing([{ kind: "removed", path: "Z:/repos/app/gone.rs" }], folder)).toBe(true);
    expect(affectsListing([{ kind: "renamed", path: "Z:/repos/app/moved.md" }], folder)).toBe(true);
  });
  it("false for a modified child (row already exists), or a change deeper down / elsewhere", () => {
    expect(affectsListing([{ kind: "modified", path: "Z:/repos/app/edit.ts" }], folder)).toBe(false);
    expect(affectsListing([{ kind: "created", path: "Z:/repos/app/src/deep.ts" }], folder)).toBe(false);
    expect(affectsListing([{ kind: "created", path: "Z:/other/x.ts" }], folder)).toBe(false);
    expect(affectsListing([], folder)).toBe(false);
    expect(affectsListing([{ kind: "created", path: "/x/y.ts" }], "")).toBe(false);
  });
});

describe("folderHasActivity (CPE-402 — is the agent working inside this folder)", () => {
  it("true for a direct or nested descendant, cross-platform", () => {
    expect(folderHasActivity(["Z:/repos/app/src/lib/x.ts"], "Z:\\repos\\app\\src")).toBe(true);
    expect(folderHasActivity(["Z:/repos/app/a.ts"], "Z:/repos/app")).toBe(true);
  });
  it("false for the folder itself, a sibling sharing a prefix, or an empty folder", () => {
    expect(folderHasActivity(["Z:/repos/app"], "Z:/repos/app")).toBe(false); // self, not inside
    expect(folderHasActivity(["Z:/repos/app-2/x.ts"], "Z:/repos/app")).toBe(false); // prefix ≠ descendant
    expect(folderHasActivity([], "Z:/repos/app")).toBe(false);
    expect(folderHasActivity(["Z:/repos/app/x.ts"], "")).toBe(false);
  });
});

describe("folderHasActivityNorm + normalizeActivityPaths (CPE-698 — normalize once, not per row)", () => {
  it("normalizeActivityPaths normalizes each path (separators + case) and drops empties", () => {
    // normalizePath lowercases and forward-slashes for cross-platform comparison.
    expect(normalizeActivityPaths(["Z:\\repos\\app\\x.ts", "", "Z:/repos/app/y.ts"])).toEqual([
      "z:/repos/app/x.ts",
      "z:/repos/app/y.ts",
    ]);
  });

  it("gives the same answers as folderHasActivity when fed pre-normalized paths", () => {
    const raw = ["Z:\\repos\\app\\src\\lib\\x.ts", "Z:/repos/app-2/y.ts"];
    const norm = normalizeActivityPaths(raw);
    for (const dir of ["Z:\\repos\\app\\src", "Z:/repos/app", "Z:/repos/app-2", "Z:/nope", ""]) {
      expect(folderHasActivityNorm(norm, dir)).toBe(folderHasActivity(raw, dir));
    }
  });

  it("excludes the folder itself and prefix-siblings", () => {
    const norm = normalizeActivityPaths(["Z:/repos/app", "Z:/repos/app-2/x.ts"]);
    expect(folderHasActivityNorm(norm, "Z:/repos/app")).toBe(false);
  });
});
