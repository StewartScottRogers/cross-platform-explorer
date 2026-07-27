import { describe, it, expect } from "vitest";
import type { AgentSession, SessionAnnouncement } from "./sidecar";
import type { FsDiff } from "./agentDiffs";
import {
  emptySessionAccumulator,
  foldSessionStarted,
  foldSessionEnded,
  foldSessionAnnouncement,
  foldDiffsForMetrics,
  wallClockMs,
  ratioPerMinute,
  safeRatio,
  deriveSessionMetrics,
  formatBytes,
  formatDuration,
  formatPerMinute,
  ingestSessionAnnouncement,
  ingestDiffsForMetrics,
  currentSessionMetrics,
  clearAgentSessionMetrics,
  type SessionAccumulator,
} from "./agentSessionMetrics";

const session = (id: string, cwd = "/work"): AgentSession => ({
  sessionId: id,
  agentId: "claude",
  agentName: "Claude Code",
  provider: "openrouter",
  model: "sonnet",
  cwd,
});

const diff = (path: string, before: string, after: string, actor?: string): FsDiff => ({
  path,
  before,
  after,
  actor,
});

describe("foldSessionStarted / foldSessionEnded / foldSessionAnnouncement", () => {
  it("creates a fresh accumulator on started, stamped with identity + startedAt", () => {
    const s = foldSessionStarted({}, session("s1"), 1000);
    expect(s.s1).toEqual({
      ...emptySessionAccumulator("s1"),
      agentId: "claude",
      agentName: "Claude Code",
      provider: "openrouter",
      model: "sonnet",
      cwd: "/work",
      startedAt: 1000,
    });
  });

  it("a second started for the same id resets it clean (a genuine restart)", () => {
    let s = foldSessionStarted({}, session("s1"), 1000);
    s = foldDiffsForMetrics(s, [diff("/a", "", "x", "s1")]);
    expect(s.s1.editCount).toBe(1);
    s = foldSessionStarted(s, session("s1"), 5000);
    expect(s.s1.editCount).toBe(0);
    expect(s.s1.startedAt).toBe(5000);
  });

  it("stamps endedAt on an existing accumulator; a no-op for an unknown id", () => {
    let s = foldSessionStarted({}, session("s1"), 1000);
    s = foldSessionEnded(s, "s1", 2000);
    expect(s.s1.endedAt).toBe(2000);
    const unchanged = foldSessionEnded(s, "no-such-session", 3000);
    expect(unchanged).toBe(s); // identity-stable no-op
  });

  it("is idempotent: a second ended doesn't move endedAt", () => {
    let s = foldSessionStarted({}, session("s1"), 1000);
    s = foldSessionEnded(s, "s1", 2000);
    const again = foldSessionEnded(s, "s1", 9999);
    expect(again).toBe(s); // identity-stable no-op
    expect(again.s1.endedAt).toBe(2000);
  });

  it("foldSessionAnnouncement dispatches started/ended correctly", () => {
    const startAnn: SessionAnnouncement = { event: "started", session: session("s1") };
    let s = foldSessionAnnouncement({}, startAnn, 1000);
    expect(s.s1.startedAt).toBe(1000);
    const endAnn: SessionAnnouncement = { event: "ended", session: session("s1") };
    s = foldSessionAnnouncement(s, endAnn, 2000);
    expect(s.s1.endedAt).toBe(2000);
  });
});

describe("foldDiffsForMetrics", () => {
  it("tallies files/edits/churn by actor, ignoring user/unknown/no-actor", () => {
    let s = foldSessionStarted({}, session("s1"), 1000);
    s = foldDiffsForMetrics(s, [
      diff("/a.txt", "", "hello", "s1"), // +5 churn, new file
      diff("/b.txt", "1234567890", "12345", "s1"), // -5 churn (10 -> 5)
      diff("/c.txt", "x", "y", "user"), // skipped: not a session actor
      diff("/d.txt", "x", "y", "unknown"), // skipped
      diff("/e.txt", "x", "y"), // no actor at all: skipped
    ]);
    expect(s.s1.editCount).toBe(2);
    expect(s.s1.churnBytes).toBe(10); // 5 + 5
    expect(Object.keys(s.s1.filesTouched).sort()).toEqual(["/a.txt", "/b.txt"]);
    expect(s.user).toBeUndefined();
    expect(s.unknown).toBeUndefined();
  });

  it("counts a repeated edit to the same path as another edit + more churn, not a new file", () => {
    let s = foldSessionStarted({}, session("s1"), 1000);
    s = foldDiffsForMetrics(s, [diff("/a.txt", "", "a", "s1")]);
    s = foldDiffsForMetrics(s, [diff("/a.txt", "a", "aa", "s1")]);
    expect(s.s1.editCount).toBe(2);
    expect(s.s1.churnBytes).toBe(2); // 1 + 1
    expect(Object.keys(s.s1.filesTouched)).toEqual(["/a.txt"]); // still one distinct file
  });

  it("lazily creates a blank-identity accumulator when a diff races ahead of `started`", () => {
    const s = foldDiffsForMetrics({}, [diff("/a.txt", "", "x", "s-race")]);
    expect(s["s-race"].agentId).toBe("");
    expect(s["s-race"].editCount).toBe(1);
  });

  it("returns the same reference (no-op) when every item is skipped", () => {
    const prev: Record<string, SessionAccumulator> = {};
    const next = foldDiffsForMetrics(prev, [diff("/a", "x", "y", "user")]);
    expect(next).toBe(prev);
  });
});

describe("wallClockMs", () => {
  it("is null when never started", () => {
    expect(wallClockMs({ startedAt: null, endedAt: null })).toBeNull();
  });

  it("is endedAt - startedAt when ended", () => {
    expect(wallClockMs({ startedAt: 1000, endedAt: 4500 })).toBe(3500);
  });

  it("is now - startedAt while still live", () => {
    expect(wallClockMs({ startedAt: 1000, endedAt: null }, 6000)).toBe(5000);
  });
});

describe("ratioPerMinute (division-safe)", () => {
  it("computes count per minute of ms", () => {
    expect(ratioPerMinute(120, 60000)).toBe(120); // 120 in 1 minute
    expect(ratioPerMinute(60, 30000)).toBe(120); // 60 in 30s -> 120/min
  });

  it("is undefined for a null/zero/negative/non-finite denominator", () => {
    expect(ratioPerMinute(10, null)).toBeUndefined();
    expect(ratioPerMinute(10, 0)).toBeUndefined();
    expect(ratioPerMinute(10, -5)).toBeUndefined();
    expect(ratioPerMinute(10, NaN)).toBeUndefined();
    expect(ratioPerMinute(10, Infinity)).toBeUndefined();
  });

  it("is undefined for an invalid numerator", () => {
    expect(ratioPerMinute(NaN, 60000)).toBeUndefined();
    expect(ratioPerMinute(-1, 60000)).toBeUndefined();
  });
});

describe("safeRatio (division-safe)", () => {
  it("divides normally", () => {
    expect(safeRatio(10, 2)).toBe(5);
  });

  it("is undefined for a zero/negative/non-finite denominator", () => {
    expect(safeRatio(10, 0)).toBeUndefined();
    expect(safeRatio(10, -1)).toBeUndefined();
    expect(safeRatio(10, NaN)).toBeUndefined();
  });

  it("is undefined for an invalid numerator", () => {
    expect(safeRatio(NaN, 2)).toBeUndefined();
    expect(safeRatio(-1, 2)).toBeUndefined();
  });
});

describe("deriveSessionMetrics", () => {
  it("joins tokens/cost from agentCost and computes ratios", () => {
    const acc: SessionAccumulator = {
      ...emptySessionAccumulator("s1"),
      startedAt: 0,
      endedAt: 60000, // 1 minute wall-clock
      filesTouched: { "/a": true, "/b": true },
      editCount: 3,
      churnBytes: 2000,
    };
    const m = deriveSessionMetrics(acc, { inputTokens: 100, outputTokens: 100, costUsd: 2 });
    expect(m.totalTokens).toBe(200);
    expect(m.costUsd).toBe(2);
    expect(m.filesTouched).toBe(2);
    expect(m.wallClockMs).toBe(60000);
    expect(m.tokensPerMinute).toBe(200); // 200 tokens in 1 minute
    expect(m.usdPerFile).toBe(1); // 2 / 2
    expect(m.churnPer1kTokens).toBe(10000); // 2000 / (200/1000)
  });

  it("zeros out tokens/cost when no cost record is joined, without throwing", () => {
    const acc = emptySessionAccumulator("s1");
    const m = deriveSessionMetrics(acc, undefined, 5000);
    expect(m.inputTokens).toBe(0);
    expect(m.outputTokens).toBe(0);
    expect(m.totalTokens).toBe(0);
    expect(m.costUsd).toBe(0);
  });

  it("hides ratios when their denominator is 0 (no wall-clock / no files / no tokens)", () => {
    const acc = emptySessionAccumulator("s1"); // never started -> wallClockMs null; 0 files
    const m = deriveSessionMetrics(acc, { inputTokens: 10, outputTokens: 0, costUsd: 0.5 });
    expect(m.tokensPerMinute).toBeUndefined(); // wall-clock unknown
    expect(m.usdPerFile).toBeUndefined(); // 0 files touched
    // totalTokens is 10 here, so churn/1k tokens IS defined (churnBytes 0 / 0.01 = 0):
    expect(m.churnPer1kTokens).toBe(0);
  });

  it("hides churn/1k-tokens when no tokens were reported at all", () => {
    const acc: SessionAccumulator = { ...emptySessionAccumulator("s1"), churnBytes: 500 };
    const m = deriveSessionMetrics(acc, { inputTokens: 0, outputTokens: 0, costUsd: 0 });
    expect(m.churnPer1kTokens).toBeUndefined();
  });

  it("treats a malformed cost record (NaN/negative fields) as zero, mirroring totalTokens", () => {
    const acc = emptySessionAccumulator("s1");
    const m = deriveSessionMetrics(acc, { inputTokens: NaN, outputTokens: -5, costUsd: NaN });
    expect(m.inputTokens).toBe(0);
    expect(m.outputTokens).toBe(0);
    expect(m.costUsd).toBe(0);
  });
});

describe("formatBytes", () => {
  it("formats sub-1024 as whole bytes", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1023)).toBe("1023 B");
  });

  it("formats KB/MB/GB with a decimal under 10, whole above", () => {
    expect(formatBytes(1536)).toBe("1.5 KB"); // 1.5 KB
    expect(formatBytes(15 * 1024)).toBe("15 KB");
    expect(formatBytes(1024 * 1024 * 3)).toBe("3 MB");
    expect(formatBytes(1024 * 1024 * 1024 * 2)).toBe("2 GB");
  });

  it("is NaN/negative-safe, rendering 0 B", () => {
    expect(formatBytes(NaN)).toBe("0 B");
    expect(formatBytes(-10)).toBe("0 B");
    expect(formatBytes(Infinity)).toBe("0 B");
  });
});

describe("formatDuration", () => {
  it("formats seconds under a minute", () => {
    expect(formatDuration(0)).toBe("0s");
    expect(formatDuration(45000)).toBe("45s");
  });

  it("formats minutes (+ seconds when non-zero)", () => {
    expect(formatDuration(60000)).toBe("1m");
    expect(formatDuration(125000)).toBe("2m 5s");
  });

  it("formats hours (+ minutes when non-zero)", () => {
    expect(formatDuration(3600000)).toBe("1h");
    expect(formatDuration(3600000 + 600000)).toBe("1h 10m");
  });

  it("renders — for null/NaN/negative (unknown wall-clock)", () => {
    expect(formatDuration(null)).toBe("—");
    expect(formatDuration(NaN)).toBe("—");
    expect(formatDuration(-1)).toBe("—");
  });
});

describe("formatPerMinute", () => {
  it("formats one decimal place with a /min suffix", () => {
    expect(formatPerMinute(120)).toBe("120.0/min");
    expect(formatPerMinute(3.456)).toBe("3.5/min");
  });

  it("is null (hide the row) for undefined/non-finite/negative", () => {
    expect(formatPerMinute(undefined)).toBeNull();
    expect(formatPerMinute(NaN)).toBeNull();
    expect(formatPerMinute(-1)).toBeNull();
  });
});

describe("full accumulator lifecycle: started -> diffs -> ended", () => {
  it("yields the right files/churn/edit-count/wall-clock for a whole session (pure folds, no store)", () => {
    let s = foldSessionAnnouncement({}, { event: "started", session: session("s1") }, 1000);
    s = foldDiffsForMetrics(s, [
      diff("/a.txt", "", "hello world", "s1"), // created: churn 11
      diff("/b.txt", "0123456789", "01234", "s1"), // 10 -> 5: churn 5
    ]);
    s = foldDiffsForMetrics(s, [diff("/a.txt", "hello world", "hello", "s1")]); // 11 -> 5: churn 6
    s = foldSessionAnnouncement({ ...s }, { event: "ended", session: session("s1") }, 9000);
    const acc = s.s1;
    expect(acc.editCount).toBe(3);
    expect(acc.churnBytes).toBe(11 + 5 + 6);
    expect(Object.keys(acc.filesTouched).sort()).toEqual(["/a.txt", "/b.txt"]);
    expect(acc.startedAt).toBe(1000);
    expect(acc.endedAt).toBe(9000);
    expect(wallClockMs(acc)).toBe(8000);
  });
});

describe("store lifecycle (ingestSessionAnnouncement / ingestDiffsForMetrics / clear)", () => {
  it("folds a full session through the store and clears back to empty", () => {
    clearAgentSessionMetrics();
    ingestSessionAnnouncement({ event: "started", session: session("s1") }, 1000);
    ingestDiffsForMetrics([diff("/a.txt", "", "hi", "s1")]);
    ingestSessionAnnouncement({ event: "ended", session: session("s1") }, 2000);
    const snap = currentSessionMetrics();
    expect(snap.s1.editCount).toBe(1);
    expect(snap.s1.churnBytes).toBe(2);
    expect(snap.s1.endedAt).toBe(2000);
    clearAgentSessionMetrics();
    expect(currentSessionMetrics()).toEqual({});
  });

  it("ignores an empty diff batch without touching the store", () => {
    clearAgentSessionMetrics();
    ingestDiffsForMetrics([]);
    expect(currentSessionMetrics()).toEqual({});
  });
});
