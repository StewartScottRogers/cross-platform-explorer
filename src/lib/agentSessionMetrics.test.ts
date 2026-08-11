import { describe, it, expect, vi, beforeEach } from "vitest";
import type { AgentSession, SessionAnnouncement } from "./sidecar";
import type { FsDiff } from "./agentDiffs";

// vi.mock is hoisted; create the mock fn via vi.hoisted so the factory closes over an initialised
// binding (matches sidecar.test.ts). The typed client's `metricsRecord` is the only member the flush uses.
const { metricsRecord } = vi.hoisted(() => ({ metricsRecord: vi.fn() }));
vi.mock("./bindings.gen", () => ({ commands: { metricsRecord } }));

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
  sessionMetricsWorthPersisting,
  buildMetricsRecord,
  flushSession,
  flushAllSessions,
  type SessionAccumulator,
  type SessionMetrics,
} from "./agentSessionMetrics";
import { ingestCost, clearAgentCost } from "./agentCost";

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

describe("sessionMetricsWorthPersisting (skip empty / never-started)", () => {
  const m = (over: Partial<SessionMetrics>): SessionMetrics =>
    deriveSessionMetrics(
      { ...emptySessionAccumulator("s1"), ...over } as SessionAccumulator,
      undefined,
      2000,
    );

  it("is false for a never-started (lazy fs-diff) entry, even with activity", () => {
    expect(sessionMetricsWorthPersisting(m({ startedAt: null, editCount: 5, churnBytes: 99 }))).toBe(false);
  });

  it("is false for a started-but-did-nothing session (no files/edits/churn/tokens/cost)", () => {
    expect(sessionMetricsWorthPersisting(m({ startedAt: 1000, endedAt: 2000 }))).toBe(false);
  });

  it("is true once any signal is present (edit / churn / files)", () => {
    expect(sessionMetricsWorthPersisting(m({ startedAt: 1000, endedAt: 2000, editCount: 1 }))).toBe(true);
    expect(
      sessionMetricsWorthPersisting(m({ startedAt: 1000, endedAt: 2000, churnBytes: 10 })),
    ).toBe(true);
  });

  it("is true for a token-only session (cost present, no filesystem activity)", () => {
    const acc: SessionAccumulator = { ...emptySessionAccumulator("s1"), startedAt: 1000, endedAt: 2000 };
    const withTokens = deriveSessionMetrics(acc, { inputTokens: 10, outputTokens: 5, costUsd: 0.1 }, 2000);
    expect(sessionMetricsWorthPersisting(withTokens)).toBe(true);
  });
});

describe("buildMetricsRecord (pure, wire-shaped)", () => {
  it("builds the camelCase record from a derived SessionMetrics, joining tokens/cost", () => {
    const acc: SessionAccumulator = {
      ...emptySessionAccumulator("s1"),
      agentId: "claude",
      agentName: "Claude Code",
      provider: "openrouter",
      model: "sonnet",
      cwd: "/work",
      startedAt: 1000,
      endedAt: 61000, // 1 minute
      filesTouched: { "/a": true, "/b": true },
      editCount: 3,
      churnBytes: 2048,
    };
    const m = deriveSessionMetrics(acc, { inputTokens: 100, outputTokens: 50, costUsd: 1.25 }, 99999);
    const rec = buildMetricsRecord(m);
    expect(rec).toEqual({
      sessionId: "s1",
      agentId: "claude",
      agentName: "Claude Code",
      provider: "openrouter",
      model: "sonnet",
      cwd: "/work",
      startedAt: 1000,
      endedAt: 61000,
      wallClockMs: 60000,
      inputTokens: 100,
      outputTokens: 50,
      totalTokens: 150,
      costUsd: 1.25,
      filesTouched: 2,
      churnBytes: 2048,
      editCount: 3,
    });
  });

  it("stamps endedAt=now + recomputes wallClock when endedAt is somehow still null", () => {
    const acc: SessionAccumulator = { ...emptySessionAccumulator("s1"), startedAt: 1000, editCount: 1 };
    const m = deriveSessionMetrics(acc, undefined, 4000); // still live -> endedAt null
    const rec = buildMetricsRecord(m, 4000);
    expect(rec.endedAt).toBe(4000);
    expect(rec.wallClockMs).toBe(3000);
  });
});

describe("flushSession / flushAllSessions (CPE-1113 flush-on-end)", () => {
  beforeEach(() => {
    metricsRecord.mockReset();
    metricsRecord.mockResolvedValue({ status: "ok", data: null });
    clearAgentSessionMetrics(); // also resets the flushed-ids guard
    clearAgentCost();
  });

  it("flushes exactly one record built from the accumulator + agentCost on a session end", async () => {
    ingestSessionAnnouncement({ event: "started", session: session("s1") }, 1000);
    ingestDiffsForMetrics([diff("/a.txt", "", "hello", "s1")]); // 1 edit, +5 churn, 1 file
    ingestCost({ sessionId: "s1", inputTokens: 100, outputTokens: 50, costUsd: 2 });
    ingestSessionAnnouncement({ event: "ended", session: session("s1") }, 61000); // 1 min wall-clock

    await flushSession("s1");

    expect(metricsRecord).toHaveBeenCalledTimes(1);
    expect(metricsRecord).toHaveBeenCalledWith({
      sessionId: "s1",
      agentId: "claude",
      agentName: "Claude Code",
      provider: "openrouter",
      model: "sonnet",
      cwd: "/work",
      startedAt: 1000,
      endedAt: 61000,
      wallClockMs: 60000,
      inputTokens: 100,
      outputTokens: 50,
      totalTokens: 150,
      costUsd: 2,
      filesTouched: 1,
      churnBytes: 5,
      editCount: 1,
    });
  });

  it("flushes at most once per session end (a re-reconcile can't double-write)", async () => {
    ingestSessionAnnouncement({ event: "started", session: session("s1") }, 1000);
    ingestDiffsForMetrics([diff("/a.txt", "", "x", "s1")]);
    ingestSessionAnnouncement({ event: "ended", session: session("s1") }, 2000);

    await flushSession("s1");
    await flushSession("s1"); // idempotent
    await flushAllSessions(); // full-stop path — still no double-write

    expect(metricsRecord).toHaveBeenCalledTimes(1);
  });

  it("does NOT flush an empty / never-started session (no junk rows)", async () => {
    // Never-started: a lone fs-diff races in with no `started`.
    ingestDiffsForMetrics([diff("/a.txt", "", "x", "s-race")]);
    // Started but did nothing.
    ingestSessionAnnouncement({ event: "started", session: session("s-idle") }, 1000);
    ingestSessionAnnouncement({ event: "ended", session: session("s-idle") }, 2000);

    await flushSession("s-race");
    await flushSession("s-idle");
    await flushAllSessions();

    expect(metricsRecord).not.toHaveBeenCalled();
  });

  it("swallows a flush failure so teardown/live view is unaffected", async () => {
    metricsRecord.mockRejectedValueOnce(new Error("boom"));
    ingestSessionAnnouncement({ event: "started", session: session("s1") }, 1000);
    ingestDiffsForMetrics([diff("/a.txt", "", "x", "s1")]);
    ingestSessionAnnouncement({ event: "ended", session: session("s1") }, 2000);

    await expect(flushSession("s1")).resolves.toBeUndefined(); // does not throw
  });

  it("a restart of the same session id un-marks it so its next end flushes again", async () => {
    ingestSessionAnnouncement({ event: "started", session: session("s1") }, 1000);
    ingestDiffsForMetrics([diff("/a.txt", "", "x", "s1")]);
    ingestSessionAnnouncement({ event: "ended", session: session("s1") }, 2000);
    await flushSession("s1");
    expect(metricsRecord).toHaveBeenCalledTimes(1);

    // Same id restarts (accumulator resets clean, flushed-guard un-marked) and ends again.
    ingestSessionAnnouncement({ event: "started", session: session("s1") }, 3000);
    ingestDiffsForMetrics([diff("/b.txt", "", "y", "s1")]);
    ingestSessionAnnouncement({ event: "ended", session: session("s1") }, 4000);
    await flushSession("s1");
    expect(metricsRecord).toHaveBeenCalledTimes(2);
  });

  it("flushAllSessions flushes every worth-persisting session once", async () => {
    for (const id of ["s1", "s2"]) {
      ingestSessionAnnouncement({ event: "started", session: session(id) }, 1000);
      ingestDiffsForMetrics([diff(`/${id}.txt`, "", "x", id)]);
      ingestSessionAnnouncement({ event: "ended", session: session(id) }, 2000);
    }
    await flushAllSessions();
    expect(metricsRecord).toHaveBeenCalledTimes(2);
  });

  // CPE-1626: `reconcileAgentWatch`'s "stop the removed" loop (App.svelte) calls `flushSession` for
  // ANY session dropped from the armed set — including a still-RUNNING session that was merely
  // disarmed because the explorer navigated away (a "pause", not an end). Before this fix, `flushSession`
  // had no way to tell a pause from a real end: it would persist whatever was in the accumulator at that
  // moment (an INCOMPLETE row) and mark the id flushed — so the later, genuine `ended` flush became a
  // silent no-op (the `flushedSessionIds` guard at L393/L398) and every post-pause edit was dropped from
  // the journal forever. This is NOT the "two fragmented rows" failure the old code comment described —
  // it's a single incomplete row plus silent data loss for the rest of the session.
  it("CPE-1626 (negative control against pre-fix code): a pause (flush called before the session has actually ended) must not persist an incomplete row — the eventual real-end flush must carry ALL the session's activity in ONE row", async () => {
    ingestSessionAnnouncement({ event: "started", session: session("s1") }, 1000);
    ingestDiffsForMetrics([diff("/a.txt", "", "hello", "s1")]); // pre-pause activity: 1 edit

    // Simulate `reconcileAgentWatch` disarming this still-running session because the explorer
    // navigated away — NOT because the session ended. flushSession is called at this "stop the
    // removed" seam exactly as App.svelte calls it.
    await flushSession("s1", 5000);
    expect(metricsRecord).not.toHaveBeenCalled(); // must NOT persist a premature/incomplete row

    // Resume: the explorer navigates back (or the session stays alive regardless); more activity
    // accrues into the SAME accumulator, since pausing never cleared or flushed it.
    ingestDiffsForMetrics([diff("/b.txt", "", "post-pause world", "s1")]); // post-pause: 1 more edit
    ingestSessionAnnouncement({ event: "ended", session: session("s1") }, 9000); // genuine end

    await flushSession("s1", 9000);

    expect(metricsRecord).toHaveBeenCalledTimes(1); // exactly one row for the whole session
    const rec = metricsRecord.mock.calls[0][0];
    expect(rec.editCount).toBe(2); // BOTH pre- and post-pause edits present — nothing dropped
    expect(rec.startedAt).toBe(1000);
    expect(rec.endedAt).toBe(9000); // the real end, not the pause timestamp
  });
});
