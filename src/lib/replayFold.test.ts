import { describe, it, expect } from "vitest";
import { stateAtFrom, childrenAt, baselineToState } from "./replayFold";
import type { AuditEvent, Baseline } from "./bindings.gen";

/**
 * These tests port the Rust reconstruction tests (`crates/server/src/replay.rs` +
 * `replay_view.rs`) verbatim as the ORACLE: each scenario asserts the exact same outcome the Rust
 * `#[test]` of the same name asserts, so the TS fold and the Rust fold provably agree.
 */

const ev = (ts: number, kind: string, path: string): AuditEvent => ({
  ts,
  session: "s1",
  kind,
  path,
  actor: null,
  detail: null,
});

const renamed = (ts: number, from: string, to: string): AuditEvent => ({
  ts,
  session: "s1",
  kind: "renamed",
  path: from,
  actor: null,
  detail: `-> ${to}`,
});

/** Build a baseline (paths → ts 0, kind "baseline"), mirroring `replay_baseline::Baseline`. */
const baseline = (paths: string[]): Baseline => ({ paths, truncated: false });

/** Sorted key list of a reconstructed state — the analogue of `state_at(..).into_keys().collect()`. */
const keys = (s: Map<string, unknown>): string[] => [...s.keys()].sort();

// --- state_at (empty-baseline fold) — mirrors replay.rs tests ---

describe("stateAtFrom — plain fold (empty baseline == state_at)", () => {
  it("empty stream yields empty state, no panic", () => {
    expect(stateAtFrom(null, [], 0).size).toBe(0);
    expect(stateAtFrom(null, [], Number.MAX_SAFE_INTEGER).size).toBe(0);
  });

  it("created appears at or after its ts", () => {
    const events = [ev(10, "created", "/a")];
    expect(stateAtFrom(null, events, 9).has("/a")).toBe(false);
    expect(stateAtFrom(null, events, 10).has("/a")).toBe(true);
    expect(stateAtFrom(null, events, 100).has("/a")).toBe(true);
  });

  it("removed deletes the path", () => {
    const events = [ev(10, "created", "/a"), ev(20, "removed", "/a")];
    expect(stateAtFrom(null, events, 15).has("/a")).toBe(true);
    expect(stateAtFrom(null, events, 20).has("/a")).toBe(false);
    expect(stateAtFrom(null, events, 100).has("/a")).toBe(false);
  });

  it("renamed relocates: old gone, target present with event ts/kind", () => {
    const events = [ev(10, "created", "/a"), renamed(20, "/a", "/b")];
    const s15 = stateAtFrom(null, events, 15);
    expect(s15.has("/a")).toBe(true);
    expect(s15.has("/b")).toBe(false);
    const s20 = stateAtFrom(null, events, 20);
    expect(s20.has("/a")).toBe(false);
    expect(s20.has("/b")).toBe(true);
    expect(s20.get("/b")).toEqual({ ts: 20, kind: "renamed" });
  });

  it("renamed with unparseable detail is a no-op", () => {
    const bad1: AuditEvent = { ts: 20, session: "s1", kind: "renamed", path: "/a", actor: null, detail: "nonsense" };
    const bad2 = ev(20, "renamed", "/a"); // detail null
    for (const bad of [bad1, bad2]) {
      const s = stateAtFrom(null, [ev(10, "created", "/a"), bad], 20);
      expect(s.has("/a")).toBe(true);
      expect(s.size).toBe(1);
    }
  });

  it("read is a no-op (does not overwrite last-touch ts/kind)", () => {
    const events = [ev(10, "created", "/a"), ev(20, "read", "/a")];
    const s = stateAtFrom(null, events, 20);
    expect(s.size).toBe(1);
    expect(s.get("/a")).toEqual({ ts: 10, kind: "created" });
  });

  it("modified updates ts and kind", () => {
    const s = stateAtFrom(null, [ev(10, "created", "/a"), ev(30, "modified", "/a")], 30);
    expect(s.get("/a")).toEqual({ ts: 30, kind: "modified" });
  });

  it("modified inserts when absent", () => {
    const s = stateAtFrom(null, [ev(30, "modified", "/a")], 30);
    expect(s.get("/a")).toEqual({ ts: 30, kind: "modified" });
  });

  it("state correct across several moments", () => {
    const events = [
      ev(10, "created", "/a"),
      ev(20, "created", "/b"),
      ev(30, "modified", "/a"),
      renamed(40, "/b", "/c"),
      ev(50, "removed", "/a"),
    ];
    expect(stateAtFrom(null, events, 5).size).toBe(0);
    expect(keys(stateAtFrom(null, events, 15))).toEqual(["/a"]);
    expect(keys(stateAtFrom(null, events, 25))).toEqual(["/a", "/b"]);
    expect(keys(stateAtFrom(null, events, 45))).toEqual(["/a", "/c"]);
    expect(keys(stateAtFrom(null, events, 100))).toEqual(["/c"]);
  });

  it("unsorted input folds in ts order, same as sorted", () => {
    const sorted = [
      ev(10, "created", "/a"),
      ev(20, "created", "/b"),
      ev(30, "modified", "/a"),
      renamed(40, "/b", "/c"),
    ];
    const unsorted = [...sorted].reverse();
    for (const t of [0, 10, 15, 20, 25, 30, 35, 40, 100]) {
      expect(keys(stateAtFrom(null, unsorted, t))).toEqual(keys(stateAtFrom(null, sorted, t)));
      // Also assert full node equality, not just keys.
      expect([...stateAtFrom(null, unsorted, t).entries()].sort()).toEqual(
        [...stateAtFrom(null, sorted, t).entries()].sort(),
      );
    }
  });

  it("does not mutate the input events array", () => {
    const events = [ev(30, "created", "/b"), ev(10, "created", "/a")];
    const snapshot = events.map((e) => e.ts);
    stateAtFrom(null, events, 100);
    expect(events.map((e) => e.ts)).toEqual(snapshot);
  });
});

// --- state_at_from (baseline-seeded) — mirrors replay.rs state_at_from tests ---

describe("stateAtFrom — baseline-seeded", () => {
  it("no events is exactly the baseline", () => {
    const base = baseline(["/root/pre1.txt", "/root/pre2.txt", "/root/sub"]);
    const seed = baselineToState(base);
    expect([...stateAtFrom(base, [], Number.MAX_SAFE_INTEGER).entries()].sort()).toEqual(
      [...seed.entries()].sort(),
    );
    expect([...stateAtFrom(base, [], 0).entries()].sort()).toEqual([...seed.entries()].sort());
  });

  it("empty baseline equals plain fold at every moment", () => {
    const events = [ev(10, "created", "/a"), ev(20, "modified", "/a"), ev(30, "created", "/b")];
    for (const t of [0, 10, 15, 20, 25, 30, 100]) {
      expect([...stateAtFrom(baseline([]), events, t).entries()].sort()).toEqual(
        [...stateAtFrom(null, events, t).entries()].sort(),
      );
    }
  });

  it("delete over baseline removes the pre-existing path", () => {
    const base = baseline(["/root/pre.txt", "/root/keep.txt"]);
    const events = [ev(10, "removed", "/root/pre.txt")];
    const before = stateAtFrom(base, events, 5);
    expect(before.has("/root/pre.txt")).toBe(true);
    expect(before.has("/root/keep.txt")).toBe(true);
    const after = stateAtFrom(base, events, 10);
    expect(after.has("/root/pre.txt")).toBe(false);
    expect(after.has("/root/keep.txt")).toBe(true);
  });

  it("create over baseline adds the new path", () => {
    const base = baseline(["/root/pre.txt"]);
    const after = stateAtFrom(base, [ev(10, "created", "/root/new.txt")], 10);
    expect(after.has("/root/pre.txt")).toBe(true);
    expect(after.get("/root/new.txt")).toEqual({ ts: 10, kind: "created" });
  });

  it("modify over baseline updates ts and kind by ts order", () => {
    const base = baseline(["/root/pre.txt"]);
    const events = [ev(30, "modified", "/root/pre.txt")];
    expect(stateAtFrom(base, events, 15).get("/root/pre.txt")).toEqual({ ts: 0, kind: "baseline" });
    expect(stateAtFrom(base, events, 30).get("/root/pre.txt")).toEqual({ ts: 30, kind: "modified" });
  });

  it("rename over baseline moves the pre-existing path", () => {
    const base = baseline(["/root/old.txt"]);
    const after = stateAtFrom(base, [renamed(20, "/root/old.txt", "/root/new.txt")], 20);
    expect(after.has("/root/old.txt")).toBe(false);
    expect(after.has("/root/new.txt")).toBe(true);
  });

  it("does not mutate the baseline", () => {
    const base = baseline(["/root/pre.txt"]);
    stateAtFrom(base, [ev(10, "removed", "/root/pre.txt")], 100);
    // The source Baseline object is untouched (stateAtFrom builds its own Map seed).
    expect(base.paths).toEqual(["/root/pre.txt"]);
  });
});

// --- children_at — mirrors replay_view.rs children_at tests ---

describe("childrenAt", () => {
  const stateOf = (entries: [string, number, string][]): Map<string, { ts: number; kind: string }> =>
    new Map(entries.map(([p, ts, kind]) => [p, { ts, kind }]));

  it("root returns only top-level", () => {
    const s = stateOf([
      ["/a", 1, "created"],
      ["/b", 2, "created"],
      ["/a/nested", 3, "created"],
    ]);
    expect(childrenAt(s, "/").map((e) => e.name)).toEqual(["a", "b"]);
  });

  it("root with empty-string dir is equivalent", () => {
    const s = stateOf([
      ["/a", 1, "created"],
      ["/a/b", 2, "created"],
    ]);
    expect(childrenAt(s, "")).toEqual(childrenAt(s, "/"));
  });

  it("nested dir excludes grandchildren", () => {
    const s = stateOf([
      ["/a", 1, "created"],
      ["/a/b", 2, "created"],
      ["/a/b/c.txt", 3, "created"],
      ["/a/other", 4, "created"],
    ]);
    const kids = childrenAt(s, "/a");
    expect(kids.map((e) => e.name).sort()).toEqual(["b", "other"]);
    expect(kids.every((e) => e.path !== "/a/b/c.txt")).toBe(true);
  });

  it("dir with trailing slash matches without", () => {
    const s = stateOf([["/a/b", 1, "created"]]);
    expect(childrenAt(s, "/a")).toEqual(childrenAt(s, "/a/"));
  });

  it("is deterministically sorted by path", () => {
    const s = stateOf([
      ["/z", 1, "created"],
      ["/a", 2, "created"],
      ["/m", 3, "created"],
    ]);
    expect(childrenAt(s, "/").map((e) => e.path)).toEqual(["/a", "/m", "/z"]);
  });

  it("returns entry fields from node", () => {
    const s = stateOf([["/a", 42, "modified"]]);
    const kids = childrenAt(s, "/");
    expect(kids).toEqual([{ name: "a", path: "/a", ts: 42, kind: "modified" }]);
  });

  it("empty state is empty", () => {
    const s = new Map<string, { ts: number; kind: string }>();
    expect(childrenAt(s, "/")).toEqual([]);
    expect(childrenAt(s, "/a")).toEqual([]);
  });

  it("no matches under unrelated dir is empty", () => {
    const s = stateOf([["/a/b", 1, "created"]]);
    expect(childrenAt(s, "/x")).toEqual([]);
  });

  it("backslash input is normalized", () => {
    const s = stateOf([["a/b", 1, "created"]]);
    expect(childrenAt(s, "a\\")).toEqual(childrenAt(s, "a/"));
  });
});

// --- integration: state_at_from + children_at (replay.rs oracle) ---

describe("stateAtFrom + childrenAt integration", () => {
  it("shows pre-existing plus events (Rust oracle scenario)", () => {
    const base = baseline(["/root/pre1.txt", "/root/pre2.txt"]);
    const events = [ev(10, "created", "/root/added.txt"), ev(20, "removed", "/root/pre1.txt")];
    const listing = stateAtFrom(base, events, 20);
    const names = childrenAt(listing, "/root")
      .map((e) => e.name)
      .sort();
    // pre1 deleted, pre2 (untouched) still shown, added.txt created.
    expect(names).toEqual(["added.txt", "pre2.txt"]);
  });

  it("children_at over an events-only fold matches the Rust replay_view integration test", () => {
    const events = [
      ev(10, "created", "/a"),
      ev(20, "created", "/a/b"),
      ev(30, "created", "/a/b/c.txt"),
    ];
    const before = stateAtFrom(null, events, 15);
    const after = stateAtFrom(null, events, 30);
    expect(childrenAt(before, "/a")).toEqual([]);
    const kids = childrenAt(after, "/a");
    expect(kids.length).toBe(1);
    expect(kids[0].path).toBe("/a/b");
  });
});
