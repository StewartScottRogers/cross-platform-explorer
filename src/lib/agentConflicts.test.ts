import { describe, it, expect } from "vitest";
import {
  foldOverlaps,
  overlapHasUnknown,
  friendlyActor,
  relativeLabel,
  OVERLAP_WINDOW_MS,
  type ActivityOverlap,
} from "./agentConflicts";
import type { TimelineEntry } from "./agentActivity";
import type { AgentSession } from "./sidecar";

const entry = (over: Partial<TimelineEntry> = {}): TimelineEntry => ({
  id: 1,
  kind: "modified",
  path: "/repo/shared.ts",
  at: 0,
  actor: "session-a",
  ...over,
});

describe("foldOverlaps (CPE-1100)", () => {
  it("returns no overlap for a single actor touching a path repeatedly", () => {
    const entries = [
      entry({ id: 1, at: 0, actor: "session-a" }),
      entry({ id: 2, at: 1000, actor: "session-a" }),
      entry({ id: 3, at: 2000, actor: "session-a" }),
    ];
    expect(foldOverlaps(entries)).toEqual([]);
  });

  it("detects a 2-actor overlap within the window", () => {
    const entries = [
      entry({ id: 1, at: 0, actor: "session-a" }),
      entry({ id: 2, at: 1500, actor: "session-b" }),
    ];
    const overlaps = foldOverlaps(entries);
    expect(overlaps).toHaveLength(1);
    expect(overlaps[0]).toMatchObject({
      path: "/repo/shared.ts",
      lastAt: 1500,
    });
    expect(overlaps[0].actors.sort()).toEqual(["session-a", "session-b"]);
  });

  it("detects a 3-actor overlap and dedupes actor values", () => {
    const entries = [
      entry({ id: 1, at: 0, actor: "session-a" }),
      entry({ id: 2, at: 1000, actor: "session-b" }),
      entry({ id: 3, at: 1200, actor: "session-a" }), // repeat actor — must not double-count
      entry({ id: 4, at: 2000, actor: "session-c" }),
    ];
    const overlaps = foldOverlaps(entries);
    expect(overlaps).toHaveLength(1);
    expect(overlaps[0].actors.sort()).toEqual(["session-a", "session-b", "session-c"]);
    expect(overlaps[0].lastAt).toBe(2000);
  });

  it("does not overlap when the two touches fall outside the window", () => {
    const entries = [
      entry({ id: 1, at: 0, actor: "session-a" }),
      entry({ id: 2, at: OVERLAP_WINDOW_MS + 1, actor: "session-b" }),
    ];
    expect(foldOverlaps(entries)).toEqual([]);
  });

  it("returns [] for empty input", () => {
    expect(foldOverlaps([])).toEqual([]);
  });

  it("does not overlap two untagged/unknown touches of the same path (can't prove distinctness)", () => {
    const entries = [
      entry({ id: 1, at: 0, actor: "unknown" }),
      entry({ id: 2, at: 1000, actor: undefined }),
    ];
    expect(foldOverlaps(entries)).toEqual([]);
  });

  it("does overlap a known actor against an unknown one, and flags it via overlapHasUnknown", () => {
    const entries = [
      entry({ id: 1, at: 0, actor: "session-a" }),
      entry({ id: 2, at: 1000, actor: "unknown" }),
    ];
    const overlaps = foldOverlaps(entries);
    expect(overlaps).toHaveLength(1);
    expect(overlapHasUnknown(overlaps[0])).toBe(true);
  });

  it("overlapHasUnknown is false when every actor is resolved", () => {
    const overlap: ActivityOverlap = { path: "/a", actors: ["user", "session-a"], lastAt: 0 };
    expect(overlapHasUnknown(overlap)).toBe(false);
  });

  it("keeps distinct paths separate and sorts overlaps most-recent first", () => {
    const entries = [
      entry({ id: 1, path: "/a", at: 0, actor: "session-a" }),
      entry({ id: 2, path: "/a", at: 1000, actor: "session-b" }),
      entry({ id: 3, path: "/b", at: 5000, actor: "user" }),
      entry({ id: 4, path: "/b", at: 5500, actor: "session-c" }),
    ];
    const overlaps = foldOverlaps(entries);
    expect(overlaps.map((o) => o.path)).toEqual(["/b", "/a"]);
  });

  it("a user op racing an agent write on the same path counts as an overlap", () => {
    const entries = [
      entry({ id: 1, at: 0, actor: "user" }),
      entry({ id: 2, at: 800, actor: "session-a" }),
    ];
    const overlaps = foldOverlaps(entries);
    expect(overlaps).toHaveLength(1);
    expect(overlaps[0].actors.sort()).toEqual(["session-a", "user"]);
  });

  it("ignores entries with an empty path", () => {
    const entries = [
      entry({ id: 1, path: "", at: 0, actor: "session-a" }),
      entry({ id: 2, path: "", at: 100, actor: "session-b" }),
    ];
    expect(foldOverlaps(entries)).toEqual([]);
  });
});

describe("friendlyActor (CPE-1100)", () => {
  const sessions: AgentSession[] = [
    {
      sessionId: "sess-1",
      agentId: "claude-code",
      agentName: "Claude Code",
      provider: "anthropic",
      model: "claude",
      cwd: "/repo",
    },
  ];

  it("maps 'user' to 'You'", () => {
    expect(friendlyActor("user", [])).toBe("You");
  });

  it("maps 'unknown' (and empty) to 'Unknown'", () => {
    expect(friendlyActor("unknown", [])).toBe("Unknown");
    expect(friendlyActor("", [])).toBe("Unknown");
  });

  it("resolves a sessionId to its agent name when the session is known", () => {
    expect(friendlyActor("sess-1", sessions)).toBe("Claude Code");
  });

  it("falls back to a shortened id when the session is not found", () => {
    expect(friendlyActor("some-unresolvable-session-id-1234", [])).toBe("some-unres…");
  });
});

describe("relativeLabel (CPE-1100)", () => {
  it("reads as 'just now' for a sub-second gap", () => {
    expect(relativeLabel(1000, 1500)).toBe("just now");
  });

  it("reads in seconds under a minute", () => {
    expect(relativeLabel(0, 12_000)).toBe("12s ago");
  });

  it("reads in minutes under an hour", () => {
    expect(relativeLabel(0, 3 * 60_000)).toBe("3m ago");
  });

  it("reads in hours beyond that", () => {
    expect(relativeLabel(0, 2 * 60 * 60_000)).toBe("2h ago");
  });

  it("never goes negative when `at` is (clock-skew) after `now`", () => {
    expect(relativeLabel(5000, 1000)).toBe("just now");
  });
});
