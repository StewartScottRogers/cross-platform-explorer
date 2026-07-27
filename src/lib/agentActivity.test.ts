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
  foldConsulted,
  normalizeActivityByKind,
  folderActivityKindNorm,
  folderOwnerNorm,
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

  it("carries the per-event actor through foldActivities and mergeTimeline (CPE-1101)", () => {
    // Distinct actors on the same path: the conflict radar (CPE-1100) needs each write attributed.
    const items: FsActivity[] = [
      { kind: "modified", path: "/shared.rs", actor: "sess-1" },
      { kind: "created", path: "/mine.txt", actor: "user" },
    ];
    const map = foldActivities({}, items, 100);
    expect(map["/shared.rs"]).toEqual({ kind: "modified", at: 100, actor: "sess-1" });
    expect(map["/mine.txt"]).toEqual({ kind: "created", at: 100, actor: "user" });

    const tl = mergeTimeline([], items, 100, 0);
    expect(tl.map((e) => e.actor)).toEqual(["user", "sess-1"]); // newest-first
    // A later same-path write by a *different* actor overwrites the annotation's actor.
    const map2 = foldActivities(map, [{ kind: "modified", path: "/shared.rs", actor: "sess-2" }], 200);
    expect(map2["/shared.rs"]).toEqual({ kind: "modified", at: 200, actor: "sess-2" });
  });

  it("ingestActivity updates the store and clearActivity empties it", () => {
    clearActivity();
    ingestActivity([{ kind: "created", path: "/store/a" }], 500);
    // Via ingest→normalize, an untagged payload folds in with actor "unknown" (CPE-1101).
    expect(read()["/store/a"]).toEqual({ kind: "created", at: 500, actor: "unknown" });
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
    expect(read()["/r/consulted.rs"]).toEqual({ kind: "read", at: 700, actor: "unknown" });
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

describe("foldConsulted", () => {
  const reads = (...p: string[]): FsActivity[] => p.map((path) => ({ kind: "read", path }));

  it("accumulates reads newest-first, ignoring non-reads", () => {
    let s = foldConsulted([], [...reads("/a"), { kind: "modified", path: "/w" }, ...reads("/b")], 100);
    expect(s.map((e) => e.path)).toEqual(["/b", "/a"]);
    expect(s.every((e) => e.count === 1)).toBe(true);
  });

  it("dedupes by path, bumping count + recency and moving to the front", () => {
    let s = foldConsulted([], reads("/a", "/b"), 10);
    s = foldConsulted(s, reads("/a"), 20);
    expect(s.map((e) => e.path)).toEqual(["/a", "/b"]);
    expect(s[0]).toEqual({ path: "/a", at: 20, count: 2 });
  });

  it("returns the same reference when the batch has no reads", () => {
    const s = foldConsulted([], reads("/a"), 1);
    const again = foldConsulted(s, [{ kind: "modified", path: "/w" }], 2);
    expect(again).toBe(s);
  });

  it("bounds the set to the cap (drops the oldest)", () => {
    const s = foldConsulted([], reads("/a", "/b", "/c"), 1, 2);
    expect(s.map((e) => e.path)).toEqual(["/c", "/b"]);
  });
});

describe("folderActivityKindNorm (read-vs-write heat, CPE-742)", () => {
  const mk = (m: Record<string, AgentActivity["kind"]>) =>
    normalizeActivityByKind(Object.fromEntries(Object.entries(m).map(([p, kind]) => [p, { kind }])));

  it("reports write when the subtree has any change", () => {
    const sets = mk({ "Z:/r/src/a.ts": "modified", "Z:/r/docs/b.md": "read" });
    expect(folderActivityKindNorm(sets, "Z:/r/src")).toBe("write");
  });

  it("reports read when the subtree has only reads", () => {
    const sets = mk({ "Z:/r/docs/b.md": "read" });
    expect(folderActivityKindNorm(sets, "Z:/r/docs")).toBe("read");
  });

  it("write outranks read when both are inside the same folder", () => {
    const sets = mk({ "Z:/r/x/a.ts": "read", "Z:/r/x/b.ts": "created" });
    expect(folderActivityKindNorm(sets, "Z:/r/x")).toBe("write");
  });

  it("returns null when nothing is inside (and excludes the folder itself)", () => {
    const sets = mk({ "Z:/r/x": "modified", "Z:/r/other/a": "read" });
    expect(folderActivityKindNorm(sets, "Z:/r/x")).toBeNull();
  });

  it("treats created/modified/removed/renamed as writes, read as read", () => {
    const sets = mk({ "Z:/r/w/a": "removed", "Z:/r/rd/b": "read" });
    expect(folderActivityKindNorm(sets, "Z:/r/w")).toBe("write");
    expect(folderActivityKindNorm(sets, "Z:/r/rd")).toBe("read");
  });
});

describe("folderOwnerNorm (owner-coloured heat-map rollup, CPE-1116)", () => {
  // Ports the sidecar's conflict_owner.rs (per-file owner) + conflict_region.rs (region rollup)
  // cases to this module's snapshot data model: the activity map holds one *current* actor per
  // path (not a running per-agent touch count), so each path already IS a single-file
  // attribution, and folderOwnerNorm is the region rollup — every path under `dir` casts one vote
  // for its own actor; most votes ("most-touches") wins; ties break to the lexically-least actor.
  const mkOwner = (m: Record<string, { kind: AgentActivity["kind"]; actor?: string }>) =>
    normalizeActivityByKind(m);

  it("single owner: a folder with one actor's write is owned by that actor", () => {
    // Mirrors conflict_owner::single_agent_path_is_owned_by_that_agent.
    const sets = mkOwner({ "Z:/r/a/x.rs": { kind: "modified", actor: "alice" } });
    expect(folderOwnerNorm(sets, "Z:/r/a")).toBe("alice");
  });

  it("contested folder: owner is whichever actor touched the most files (most-touches wins)", () => {
    // Mirrors conflict_owner::contested_path_owner_is_top_editor, at folder granularity: alice
    // touched 2 files under Z:/r/a, bob touched 1 -> alice owns the folder.
    const sets = mkOwner({
      "Z:/r/a/x.rs": { kind: "modified", actor: "alice" },
      "Z:/r/a/y.rs": { kind: "created", actor: "alice" },
      "Z:/r/a/z.rs": { kind: "modified", actor: "bob" },
    });
    expect(folderOwnerNorm(sets, "Z:/r/a")).toBe("alice");
  });

  it("full tie on touches breaks to the lexically-least actor", () => {
    // Mirrors conflict_owner::full_tie_breaks_lexically_least_agent /
    // conflict_region::tie_on_votes_broken_by_lexically_least_owner.
    const sets = mkOwner({
      "Z:/r/a/x.rs": { kind: "modified", actor: "zoe" },
      "Z:/r/a/y.rs": { kind: "modified", actor: "amy" },
    });
    expect(folderOwnerNorm(sets, "Z:/r/a")).toBe("amy");
  });

  it("write outranks read: a folder with both a write and a read is owned by the writer", () => {
    const sets = mkOwner({
      "Z:/r/a/read.md": { kind: "read", actor: "bob" },
      "Z:/r/a/write.rs": { kind: "modified", actor: "alice" },
    });
    expect(folderOwnerNorm(sets, "Z:/r/a")).toBe("alice");
  });

  it("a read-only folder (no writes at all) is owned by its top reader", () => {
    // Write only outranks read when a write actually exists in the subtree — an all-read folder
    // still gets an owner rather than falling back to null (that's reserved for "nothing here").
    const sets = mkOwner({
      "Z:/r/a/x.md": { kind: "read", actor: "bob" },
      "Z:/r/a/y.md": { kind: "read", actor: "bob" },
      "Z:/r/a/z.md": { kind: "read", actor: "alice" },
    });
    expect(folderOwnerNorm(sets, "Z:/r/a")).toBe("bob");
  });

  it("empty subtree — nothing inside, or only the folder itself — returns null", () => {
    // Mirrors folderActivityKindNorm's "excludes the folder itself" behavior.
    const sets = mkOwner({
      "Z:/r/a": { kind: "modified", actor: "alice" },
      "Z:/r/other/x": { kind: "modified", actor: "bob" },
    });
    expect(folderOwnerNorm(sets, "Z:/r/a")).toBeNull();
    expect(folderOwnerNorm(sets, "")).toBeNull();
  });

  it("is deterministic regardless of input order", () => {
    const build = (order: string[]) =>
      mkOwner(
        Object.fromEntries(
          order.map((actor, i) => [`Z:/r/a/${i}.rs`, { kind: "modified" as const, actor }]),
        ),
      );
    const a = folderOwnerNorm(build(["zoe", "amy", "bob"]), "Z:/r/a");
    const b = folderOwnerNorm(build(["bob", "zoe", "amy"]), "Z:/r/a");
    const c = folderOwnerNorm(build(["amy", "bob", "zoe"]), "Z:/r/a");
    expect(a).toBe(b);
    expect(b).toBe(c);
  });

  it("carries actor through normalizeActivityByKind, defaulting a missing actor to 'unknown'", () => {
    const sets = normalizeActivityByKind({ "Z:/r/a/x.rs": { kind: "modified" } });
    expect(folderOwnerNorm(sets, "Z:/r/a")).toBe("unknown");
  });
});
