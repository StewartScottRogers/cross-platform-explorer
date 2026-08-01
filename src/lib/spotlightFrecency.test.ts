/**
 * Unit tests for the Spotlight frecency store (CPE-1216, backed by the CPE-952 `spotlight_frecent`
 * ranking core via CPE-1214's command). Mocks `@tauri-apps/api/core` the same way
 * `InstantSearch.test.ts` does, since the typed `commands.spotlightFrecent` client flows through it.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import {
  MAX_FRECENT_ENTRIES,
  parseFrecent,
  serializeFrecent,
  recordVisit,
  defaultView,
} from "./spotlightFrecency";
import type { Visit } from "./bindings.gen";

let frecentResponse: string[] = [];
const invoke = vi.fn((cmd: string, args?: any) => {
  if (cmd === "spotlight_frecent") return Promise.resolve(frecentResponse);
  return Promise.reject(new Error(`unexpected command: ${cmd}`));
});
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: unknown) => invoke(cmd, args),
  Channel: class {},
}));

beforeEach(() => {
  invoke.mockClear();
  frecentResponse = [];
});

describe("spotlightFrecency — recordVisit increments/creates entries (CPE-1216)", () => {
  it("appends a fresh entry (count 1) for a first-time path", () => {
    const after = recordVisit([], "/a", 1000);
    expect(after).toEqual([{ path: "/a", count: 1, last_used_s: 1000 }]);
  });

  it("increments count and bumps last_used_s for a tracked path, leaving others untouched", () => {
    const before: Visit[] = [
      { path: "/a", count: 2, last_used_s: 1000 },
      { path: "/b", count: 5, last_used_s: 2000 },
    ];
    const after = recordVisit(before, "/a", 3000);
    expect(after).toEqual([
      { path: "/a", count: 3, last_used_s: 3000 },
      { path: "/b", count: 5, last_used_s: 2000 },
    ]);
    expect(before[0]).toEqual({ path: "/a", count: 2, last_used_s: 1000 }); // original untouched
  });

  it("is pure — never mutates the input list", () => {
    const before: Visit[] = [{ path: "/a", count: 1, last_used_s: 1 }];
    const snapshot = JSON.parse(JSON.stringify(before));
    recordVisit(before, "/a", 2);
    recordVisit(before, "/b", 2);
    expect(before).toEqual(snapshot);
  });
});

describe("spotlightFrecency — decay: stalest entries are pruned once the store overflows (CPE-1216)", () => {
  it("keeps the store at MAX_FRECENT_ENTRIES, dropping the oldest last_used_s first", () => {
    let list: Visit[] = [];
    for (let i = 0; i < MAX_FRECENT_ENTRIES; i++) {
      list = recordVisit(list, `/p${i}`, i); // ascending last_used_s: /p0 is the stalest
    }
    expect(list).toHaveLength(MAX_FRECENT_ENTRIES);
    // One more, fresher visit should push the store over the cap and evict the stalest (/p0).
    list = recordVisit(list, "/new", MAX_FRECENT_ENTRIES + 100);
    expect(list).toHaveLength(MAX_FRECENT_ENTRIES);
    expect(list.some((v) => v.path === "/p0")).toBe(false); // evicted — stalest
    expect(list.some((v) => v.path === "/new")).toBe(true); // freshest — kept
  });
});

describe("spotlightFrecency — parse/serialize round-trip (CPE-1216)", () => {
  it("round-trips a visit list", () => {
    const list: Visit[] = [{ path: "/a", count: 3, last_used_s: 100 }];
    expect(parseFrecent(serializeFrecent(list))).toEqual(list);
  });

  it("tolerates null/garbage/malformed entries, degrading to []", () => {
    expect(parseFrecent(null)).toEqual([]);
    expect(parseFrecent(undefined)).toEqual([]);
    expect(parseFrecent("not json")).toEqual([]);
    expect(parseFrecent(JSON.stringify({ not: "an array" }))).toEqual([]);
    const good: Visit = { path: "/ok", count: 1, last_used_s: 1 };
    const mixed = JSON.stringify([
      good,
      { path: "/no-count" }, // missing count
      { count: 1, last_used_s: 1 }, // missing path
      { path: "/bad-count", count: "many", last_used_s: 1 }, // count not a number
    ]);
    expect(parseFrecent(mixed)).toEqual([good]);
  });
});

describe("spotlightFrecency — defaultView (CPE-1216 acceptance: most-frecent first)", () => {
  it("short-circuits to [] with no visits, without calling the backend", async () => {
    const secs = await defaultView([], 1000);
    expect(secs).toEqual([]);
    expect(invoke).not.toHaveBeenCalled();
  });

  it("ranks visits through spotlight_frecent and wraps the paths as one 'recent' section", async () => {
    frecentResponse = ["/hot", "/warm"];
    const visits: Visit[] = [
      { path: "/hot", count: 5, last_used_s: 900 },
      { path: "/warm", count: 1, last_used_s: 950 },
    ];
    const secs = await defaultView(visits, 1000, 10);
    expect(invoke).toHaveBeenCalledWith("spotlight_frecent", { visits, nowS: 1000, limit: 10 });
    expect(secs).toEqual([
      {
        kind: "recent",
        results: [
          { text: "/hot", kind: "recent", score: 0, positions: [] },
          { text: "/warm", kind: "recent", score: 0, positions: [] },
        ],
      },
    ]);
  });

  it("returns [] when the backend ranks nothing (defensive — shouldn't happen with non-empty visits)", async () => {
    frecentResponse = [];
    const secs = await defaultView([{ path: "/a", count: 1, last_used_s: 1 }], 1000);
    expect(secs).toEqual([]);
  });
});
