import { describe, it, expect } from "vitest";
import { dirSetOf, toDirEntries, overlayEntriesAt, resolveOverlay, type ReplaySource } from "./replayOverlay";
import { stateAtFrom } from "./replayFold";
import type { AuditEvent, Baseline, ReplayEntry } from "./bindings.gen";

const ev = (ts: number, kind: string, path: string): AuditEvent => ({
  ts,
  session: "s1",
  kind,
  path,
  actor: null,
  detail: null,
});

const baseline = (paths: string[]): Baseline => ({ paths, truncated: false });

describe("dirSetOf", () => {
  it("flags ancestors of nested paths, not leaves", () => {
    const state = stateAtFrom(null, [ev(1, "created", "/a/b/c.txt")], 1);
    const dirs = dirSetOf(state);
    expect(dirs.has("a")).toBe(true);
    expect(dirs.has("a/b")).toBe(true);
    expect(dirs.has("a/b/c.txt")).toBe(false);
  });

  it("empty state yields an empty set", () => {
    expect(dirSetOf(new Map()).size).toBe(0);
  });
});

describe("toDirEntries", () => {
  it("maps a folder row: is_dir true, no extension, size 0", () => {
    const rows: ReplayEntry[] = [{ name: "sub", path: "/root/sub", ts: 10, kind: "created" }];
    const dirs = new Set(["root/sub"]);
    const out = toDirEntries(rows, dirs);
    expect(out).toEqual([
      {
        name: "sub", path: "/root/sub", is_dir: true, size: 0, modified: 10, extension: "", hidden: false,
        is_symlink: false,
      },
    ]);
  });

  it("maps a file row: is_dir false, lowercased extension from the name", () => {
    const rows: ReplayEntry[] = [{ name: "Photo.PNG", path: "/root/Photo.PNG", ts: 20, kind: "modified" }];
    const out = toDirEntries(rows, new Set());
    expect(out[0]).toEqual({
      name: "Photo.PNG",
      path: "/root/Photo.PNG",
      is_dir: false,
      size: 0,
      modified: 20,
      extension: "png",
      hidden: false,
      is_symlink: false,
    });
  });

  it("a baseline entry (pre-existing, never itself an event) has modified: null", () => {
    const rows: ReplayEntry[] = [{ name: "pre.txt", path: "/root/pre.txt", ts: 0, kind: "baseline" }];
    expect(toDirEntries(rows, new Set())[0].modified).toBeNull();
  });

  it("flags a leading-dot name as hidden, OS-agnostically", () => {
    const rows: ReplayEntry[] = [{ name: ".env", path: "/root/.env", ts: 5, kind: "created" }];
    expect(toDirEntries(rows, new Set())[0].hidden).toBe(true);
  });

  it("does not mutate its inputs", () => {
    const rows: ReplayEntry[] = [{ name: "a", path: "/a", ts: 1, kind: "created" }];
    const rowsSnapshot = JSON.parse(JSON.stringify(rows));
    const dirs = new Set(["x"]);
    const dirsSnapshot = new Set(dirs);
    toDirEntries(rows, dirs);
    expect(rows).toEqual(rowsSnapshot);
    expect(dirs).toEqual(dirsSnapshot);
  });
});

describe("overlayEntriesAt", () => {
  it("reconstructs a baseline + events listing exactly like the in-drawer fold (Rust-oracle scenario)", () => {
    const base = baseline(["/root/pre1.txt", "/root/pre2.txt"]);
    const events = [ev(10, "created", "/root/added.txt"), ev(20, "removed", "/root/pre1.txt")];
    const names = overlayEntriesAt(base, events, 20, "/root")
      .map((e) => e.name)
      .sort();
    expect(names).toEqual(["added.txt", "pre2.txt"]);
  });

  it("distinguishes files from folders in the same directory", () => {
    const events = [ev(10, "created", "/a/dir"), ev(20, "created", "/a/dir/inside.txt"), ev(30, "created", "/a/file.txt")];
    const entries = overlayEntriesAt(null, events, 30, "/a");
    const byName = new Map(entries.map((e) => [e.name, e]));
    expect(byName.get("dir")?.is_dir).toBe(true);
    expect(byName.get("file.txt")?.is_dir).toBe(false);
  });

  it("does not mutate the baseline or events it is given", () => {
    const base = baseline(["/root/pre.txt"]);
    const baseSnapshot = { ...base, paths: [...base.paths] };
    const events = [ev(10, "removed", "/root/pre.txt")];
    const eventsSnapshot = events.map((e) => ({ ...e }));
    overlayEntriesAt(base, events, 100, "/root");
    expect(base).toEqual(baseSnapshot);
    expect(events).toEqual(eventsSnapshot);
  });
});

describe("resolveOverlay — Replay-mode gating + restore-on-exit", () => {
  const source: ReplaySource = {
    baseline: baseline(["/root/pre.txt"]),
    events: [ev(10, "created", "/root/new.txt")],
  };

  it("off means off: inactive returns null even with a fully loaded source", () => {
    expect(resolveOverlay(false, source, 10, "/root")).toBeNull();
  });

  it("active with no source yet (session not loaded / superseded) returns null, never throws", () => {
    expect(resolveOverlay(true, null, 10, "/root")).toBeNull();
  });

  it("active + source populated returns the reconstructed listing", () => {
    const out = resolveOverlay(true, source, 10, "/root");
    expect(out).not.toBeNull();
    expect(out?.map((e) => e.name).sort()).toEqual(["new.txt", "pre.txt"]);
  });

  it("restore-on-exit: flipping active back to false on the same source is exactly null again", () => {
    const on = resolveOverlay(true, source, 10, "/root");
    expect(on).not.toBeNull();
    const off = resolveOverlay(false, source, 10, "/root");
    expect(off).toBeNull();
  });

  it("restore-on-exit via source loss: session change clears the source (mirrors sessionId invalidation in AgentTimeline), gating still active", () => {
    // `active` can stay true across a session switch (the user never touched the toggle) — it's the
    // caller resetting `source` to null (as AgentTimeline already does on sessionId change) that must
    // be enough, on its own, to fall back to the live listing.
    expect(resolveOverlay(true, source, 10, "/root")).not.toBeNull();
    expect(resolveOverlay(true, null, 10, "/root")).toBeNull();
  });

  it("is a pure function: same inputs, same output, no shared mutable state across calls", () => {
    const a = resolveOverlay(true, source, 10, "/root");
    const b = resolveOverlay(true, source, 10, "/root");
    expect(a).toEqual(b);
    expect(a).not.toBe(b); // distinct arrays — callers can't accidentally alias/mutate each other
  });
});
