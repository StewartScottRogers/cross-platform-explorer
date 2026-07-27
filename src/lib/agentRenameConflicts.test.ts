/**
 * Unit tests for the competing-rename fold (CPE-1118, epic CPE-730). Ports the sidecar's
 * `conflict_rename.rs` test suite (CPE-1067) one-for-one, adapted to fold over
 * {@link TimelineEntry} instead of a `RenameActivity` list. Also covers the CPE-1121 time gate
 * (folds are now windowed by `OVERLAP_WINDOW_MS`, mirroring `agentConflicts.ts`'s overlap fold).
 */
import { describe, it, expect } from "vitest";
import { foldRenameConflicts, renameConflictNote } from "./agentRenameConflicts";
import { OVERLAP_WINDOW_MS } from "./agentConflicts";
import type { TimelineEntry } from "./agentActivity";

let nextId = 1;
const rename = (from: string, to: string, actor: string, at = 0): TimelineEntry => ({
  id: nextId++,
  kind: "renamed",
  path: to,
  at,
  actor,
  from,
  to,
});

describe("foldRenameConflicts (CPE-1118)", () => {
  it("returns [] for empty input", () => {
    expect(foldRenameConflicts([])).toEqual([]);
  });

  it("disjoint renames have no conflicts", () => {
    const entries = [rename("a.rs", "a2.rs", "alice"), rename("b.rs", "b2.rs", "bob")];
    expect(foldRenameConflicts(entries)).toEqual([]);
  });

  it("same from, different to is a divergence", () => {
    const entries = [rename("shared.rs", "alice.rs", "alice"), rename("shared.rs", "bob.rs", "bob")];
    const c = foldRenameConflicts(entries);
    expect(c).toHaveLength(1);
    expect(c[0].path).toBe("shared.rs");
    expect(c[0].kind).toBe("divergence");
    expect(c[0].actors).toEqual(["alice", "bob"]); // sorted
  });

  it("different from, same to is a collision", () => {
    const entries = [rename("a.rs", "final.rs", "alice"), rename("b.rs", "final.rs", "bob")];
    const c = foldRenameConflicts(entries);
    expect(c).toHaveLength(1);
    expect(c[0].path).toBe("final.rs");
    expect(c[0].kind).toBe("collision");
    expect(c[0].actors).toEqual(["alice", "bob"]); // sorted
  });

  it("same-actor double rename is not a conflict", () => {
    // One actor renaming the same source to two different targets (e.g. retried) — only distinct
    // actors count.
    const entries = [rename("shared.rs", "v1.rs", "solo"), rename("shared.rs", "v2.rs", "solo")];
    expect(foldRenameConflicts(entries)).toEqual([]);
  });

  it("same-actor repeated target is not a conflict", () => {
    const entries = [rename("a.rs", "final.rs", "solo"), rename("b.rs", "final.rs", "solo")];
    expect(foldRenameConflicts(entries)).toEqual([]);
  });

  it("from === to no-op is ignored", () => {
    const entries = [rename("same.rs", "same.rs", "alice"), rename("same.rs", "same.rs", "bob")];
    expect(foldRenameConflicts(entries)).toEqual([]);
  });

  it("a no-op entry doesn't suppress or distort a genuine conflict elsewhere", () => {
    const entries = [
      rename("noop.rs", "noop.rs", "alice"),
      rename("shared.rs", "alice.rs", "alice"),
      rename("shared.rs", "bob.rs", "bob"),
    ];
    const c = foldRenameConflicts(entries);
    expect(c).toHaveLength(1);
    expect(c[0].path).toBe("shared.rs");
    expect(c[0].kind).toBe("divergence");
  });

  it("divergence needs two distinct targets (same source, same target across actors is neither)", () => {
    const entries = [
      rename("shared.rs", "same_target.rs", "alice"),
      rename("shared.rs", "same_target.rs", "bob"),
    ];
    expect(foldRenameConflicts(entries)).toEqual([]);
  });

  it("three-actor divergence has actors sorted", () => {
    const entries = [
      rename("shared.rs", "c.rs", "carol"),
      rename("shared.rs", "a.rs", "alice"),
      rename("shared.rs", "b.rs", "bob"),
    ];
    const c = foldRenameConflicts(entries);
    expect(c).toHaveLength(1);
    expect(c[0].actors).toEqual(["alice", "bob", "carol"]);
  });

  it("deterministic sorted output by path", () => {
    const entries = [
      rename("zzz.rs", "z1.rs", "alice"),
      rename("aaa.rs", "a1.rs", "alice"),
      rename("zzz.rs", "z2.rs", "bob"),
      rename("aaa.rs", "a2.rs", "bob"),
    ];
    const c = foldRenameConflicts(entries);
    expect(c).toHaveLength(2);
    expect(c[0].path).toBe("aaa.rs");
    expect(c[1].path).toBe("zzz.rs");
  });

  it("divergence and collision can both be reported, sorted by path", () => {
    // alice/bob diverge on "shared.rs"; separately, carol and dave collide onto "final.rs".
    const entries = [
      rename("shared.rs", "alice.rs", "alice"),
      rename("shared.rs", "bob.rs", "bob"),
      rename("c.rs", "final.rs", "carol"),
      rename("d.rs", "final.rs", "dave"),
    ];
    const c = foldRenameConflicts(entries);
    expect(c).toHaveLength(2);
    expect(c[0].path).toBe("final.rs");
    expect(c[0].kind).toBe("collision");
    expect(c[1].path).toBe("shared.rs");
    expect(c[1].kind).toBe("divergence");
  });

  it("ignores renamed entries missing from/to (degraded single-path rename, CPE-1117 fallback)", () => {
    const entries: TimelineEntry[] = [
      { id: 1, kind: "renamed", path: "shared.rs", at: 0, actor: "alice" }, // no from/to
      { id: 2, kind: "renamed", path: "shared.rs", at: 0, actor: "bob", from: "shared.rs" }, // no to
    ];
    expect(foldRenameConflicts(entries)).toEqual([]);
  });

  it("ignores non-renamed entries even if they happen to carry from/to", () => {
    const entries: TimelineEntry[] = [
      { id: 1, kind: "modified", path: "shared.rs", at: 0, actor: "alice", from: "shared.rs", to: "alice.rs" },
      { id: 2, kind: "modified", path: "shared.rs", at: 0, actor: "bob", from: "shared.rs", to: "bob.rs" },
    ];
    expect(foldRenameConflicts(entries)).toEqual([]);
  });

  it("an untagged actor defaults to 'unknown' and can still participate in a conflict", () => {
    const entries: TimelineEntry[] = [
      { id: 1, kind: "renamed", path: "alice.rs", at: 0, from: "shared.rs", to: "alice.rs" }, // actor absent
      rename("shared.rs", "bob.rs", "bob"),
    ];
    const c = foldRenameConflicts(entries);
    expect(c).toHaveLength(1);
    expect(c[0].actors).toEqual(["bob", "unknown"]);
  });

  it("carries the latest contributing timestamp as lastAt", () => {
    const entries = [
      rename("shared.rs", "alice.rs", "alice", 1_000),
      rename("shared.rs", "bob.rs", "bob", 2_500),
    ];
    const c = foldRenameConflicts(entries);
    expect(c[0].lastAt).toBe(2_500);
  });

  it("(CPE-1121) two renames within OVERLAP_WINDOW_MS of each other still fold into a conflict", () => {
    const entries = [
      rename("shared.rs", "alice.rs", "alice", 0),
      rename("shared.rs", "bob.rs", "bob", OVERLAP_WINDOW_MS - 1),
    ];
    const c = foldRenameConflicts(entries);
    expect(c).toHaveLength(1);
    expect(c[0].path).toBe("shared.rs");
    expect(c[0].kind).toBe("divergence");
    expect(c[0].actors).toEqual(["alice", "bob"]);
    expect(c[0].lastAt).toBe(OVERLAP_WINDOW_MS - 1);
  });

  it("(CPE-1121) two same-from renames further apart than OVERLAP_WINDOW_MS do NOT fold — stale rename no longer reads as a live race", () => {
    const entries = [
      rename("shared.rs", "alice.rs", "alice", 0),
      rename("shared.rs", "bob.rs", "bob", OVERLAP_WINDOW_MS + 1),
    ];
    expect(foldRenameConflicts(entries)).toEqual([]);
  });

  it("(CPE-1121) a caller-supplied windowMs overrides the OVERLAP_WINDOW_MS default", () => {
    const entries = [rename("shared.rs", "alice.rs", "alice", 0), rename("shared.rs", "bob.rs", "bob", 100)];
    // Within the default window, but outside a tighter caller-supplied one.
    expect(foldRenameConflicts(entries)).toHaveLength(1);
    expect(foldRenameConflicts(entries, 50)).toEqual([]);
  });
});

describe("renameConflictNote", () => {
  it("is hedged, kind-specific wording", () => {
    expect(renameConflictNote("divergence")).toMatch(/different names/i);
    expect(renameConflictNote("collision")).toMatch(/same name/i);
    expect(renameConflictNote("divergence")).toMatch(/best-effort/i);
    expect(renameConflictNote("collision")).toMatch(/best-effort/i);
  });
});
