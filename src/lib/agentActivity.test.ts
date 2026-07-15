import { describe, it, expect } from "vitest";
import {
  foldActivities,
  pruneActivities,
  recentActivities,
  ingestActivity,
  clearActivity,
  fsActivity,
  ACTIVITY_TTL_MS,
  type AgentActivity,
} from "./agentActivity";
import type { FsActivity } from "./sidecar";

const read = () => {
  let v: Record<string, AgentActivity> = {};
  fsActivity.subscribe((x) => (v = x))();
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
