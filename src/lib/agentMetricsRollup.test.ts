import { describe, it, expect } from "vitest";
import { rollup, overTime, emptyTotals, type MetricsTotals } from "./agentMetricsRollup";
import type { SessionMetricsRecord } from "./bindings.gen";

/** A full, well-formed record with sensible defaults, overridable per test. */
const rec = (over: Partial<SessionMetricsRecord> = {}): SessionMetricsRecord => ({
  sessionId: "s1",
  agentId: "claude",
  agentName: "Claude Code",
  provider: "openrouter",
  model: "sonnet",
  cwd: "/work",
  startedAt: 1_000,
  endedAt: 2_000,
  wallClockMs: 1_000,
  inputTokens: 100,
  outputTokens: 50,
  totalTokens: 150,
  costUsd: 0.5,
  filesTouched: 2,
  churnBytes: 500,
  editCount: 3,
  ...over,
});

describe("rollup — empty input", () => {
  it("is all-zero, no NaN, empty maps, all-undefined ratios", () => {
    const r = rollup([]);
    expect(r.totals).toEqual(emptyTotals());
    expect(r.byModel.size).toBe(0);
    expect(r.byAgent.size).toBe(0);
    expect(r.averages).toEqual({
      avgCostUsd: 0,
      avgTokens: 0,
      avgWallClockMs: 0,
      avgFilesTouched: 0,
      avgChurnBytes: 0,
    });
    expect(r.ratios.tokensPerMinute).toBeUndefined();
    expect(r.ratios.usdPerSession).toBeUndefined();
    expect(r.ratios.usdPerFile).toBeUndefined();
    expect(r.ratios.churnPer1kTokens).toBeUndefined();
    // Belt-and-braces: nothing in the whole rollup is ever NaN.
    for (const v of Object.values(r.totals)) expect(Number.isNaN(v)).toBe(false);
    for (const v of Object.values(r.averages)) expect(Number.isNaN(v)).toBe(false);
  });
});

describe("rollup — single record", () => {
  it("totals/byModel/byAgent/averages/ratios all match the one record exactly", () => {
    const r = rollup([rec()]);

    expect(r.totals).toEqual<MetricsTotals>({
      sessions: 1,
      inputTokens: 100,
      outputTokens: 50,
      totalTokens: 150,
      costUsd: 0.5,
      wallClockMs: 1_000,
      filesTouched: 2,
      churnBytes: 500,
      editCount: 3,
    });

    expect([...r.byModel.keys()]).toEqual(["sonnet"]);
    expect(r.byModel.get("sonnet")).toEqual({ model: "sonnet", ...r.totals });

    expect([...r.byAgent.keys()]).toEqual(["claude"]);
    expect(r.byAgent.get("claude")).toEqual({ agentId: "claude", agentName: "Claude Code", ...r.totals });

    expect(r.averages).toEqual({
      avgCostUsd: 0.5,
      avgTokens: 150,
      avgWallClockMs: 1_000,
      avgFilesTouched: 2,
      avgChurnBytes: 500,
    });

    // 150 tokens / (1000ms / 60000) = 150 * 60 = 9000 tokens/min.
    expect(r.ratios.tokensPerMinute).toBeCloseTo(9_000, 6);
    // $0.5 / 1 session.
    expect(r.ratios.usdPerSession).toBeCloseTo(0.5, 6);
    // $0.5 / 2 files.
    expect(r.ratios.usdPerFile).toBeCloseTo(0.25, 6);
    // 500 bytes / (150 tokens / 1000) = 500 / 0.15 = 3333.33...
    expect(r.ratios.churnPer1kTokens).toBeCloseTo(500 / 0.15, 6);
  });
});

describe("rollup — multi-model", () => {
  it("splits totals per model correctly and sorts model keys", () => {
    const records = [
      rec({ sessionId: "s1", model: "opus", costUsd: 1, totalTokens: 100, wallClockMs: 1_000 }),
      rec({ sessionId: "s2", model: "opus", costUsd: 2, totalTokens: 200, wallClockMs: 2_000 }),
      rec({ sessionId: "s3", model: "haiku", costUsd: 0.1, totalTokens: 50, wallClockMs: 500 }),
    ];
    const r = rollup(records);

    expect(r.totals.sessions).toBe(3);
    expect(r.totals.costUsd).toBeCloseTo(3.1, 6);
    expect(r.totals.totalTokens).toBe(350);

    // Sorted keys: "haiku" < "opus" lexicographically.
    expect([...r.byModel.keys()]).toEqual(["haiku", "opus"]);

    const opus = r.byModel.get("opus")!;
    expect(opus.sessions).toBe(2);
    expect(opus.costUsd).toBeCloseTo(3, 6);
    expect(opus.totalTokens).toBe(300);
    expect(opus.wallClockMs).toBe(3_000);

    const haiku = r.byModel.get("haiku")!;
    expect(haiku.sessions).toBe(1);
    expect(haiku.costUsd).toBeCloseTo(0.1, 6);
  });
});

describe("rollup — multi-agent", () => {
  it("splits totals per agent id correctly, keeps a display name, and sorts agent keys", () => {
    const records = [
      rec({ sessionId: "s1", agentId: "claude", agentName: "Claude Code", filesTouched: 2, editCount: 1 }),
      rec({ sessionId: "s2", agentId: "claude", agentName: "Claude Code", filesTouched: 3, editCount: 2 }),
      rec({ sessionId: "s3", agentId: "codex", agentName: "Codex", filesTouched: 1, editCount: 5 }),
    ];
    const r = rollup(records);

    expect([...r.byAgent.keys()]).toEqual(["claude", "codex"]);

    const claude = r.byAgent.get("claude")!;
    expect(claude.agentName).toBe("Claude Code");
    expect(claude.sessions).toBe(2);
    expect(claude.filesTouched).toBe(5);
    expect(claude.editCount).toBe(3);

    const codex = r.byAgent.get("codex")!;
    expect(codex.agentName).toBe("Codex");
    expect(codex.sessions).toBe(1);
    expect(codex.editCount).toBe(5);
  });

  it("falls back to the agent id as the display name when agentName is blank", () => {
    const r = rollup([rec({ agentId: "mystery", agentName: "" })]);
    expect(r.byAgent.get("mystery")!.agentName).toBe("mystery");
  });
});

describe("rollup — ratios are 0-safe (division-by-zero guards)", () => {
  it("zero wall-clock -> tokensPerMinute undefined, not Infinity/NaN", () => {
    const r = rollup([rec({ wallClockMs: 0 })]);
    expect(r.ratios.tokensPerMinute).toBeUndefined();
  });

  it("zero files touched -> usdPerFile undefined", () => {
    const r = rollup([rec({ filesTouched: 0 })]);
    expect(r.ratios.usdPerFile).toBeUndefined();
  });

  it("zero total tokens -> churnPer1kTokens undefined", () => {
    const r = rollup([rec({ totalTokens: 0 })]);
    expect(r.ratios.churnPer1kTokens).toBeUndefined();
  });

  it("a malformed (NaN/negative) field never poisons the totals with NaN", () => {
    const r = rollup([rec({ costUsd: NaN, churnBytes: -5, filesTouched: Infinity })]);
    expect(Number.isNaN(r.totals.costUsd)).toBe(false);
    expect(r.totals.costUsd).toBe(0);
    expect(r.totals.churnBytes).toBe(0);
    expect(r.totals.filesTouched).toBe(0);
  });
});

describe("overTime — bucketing", () => {
  it("empty input yields an empty array", () => {
    expect(overTime([], "day")).toEqual([]);
  });

  it("a single record produces one bucket with its own totals", () => {
    const dayMs = 24 * 60 * 60 * 1000;
    const startedAt = dayMs * 3 + 1_000; // day 3, 1s past midnight UTC
    const points = overTime([rec({ startedAt, costUsd: 1.5, totalTokens: 100 })], "day");
    expect(points).toHaveLength(1);
    expect(points[0]).toEqual({ bucketStart: dayMs * 3, costUsd: 1.5, totalTokens: 100, sessions: 1 });
  });

  it("buckets multiple records into the same day and sums them, sorted ascending", () => {
    const dayMs = 24 * 60 * 60 * 1000;
    const day0 = 0;
    const day1 = dayMs;
    const records = [
      rec({ sessionId: "a", startedAt: day1 + 5_000, costUsd: 1, totalTokens: 10 }),
      rec({ sessionId: "b", startedAt: day0 + 1_000, costUsd: 2, totalTokens: 20 }),
      rec({ sessionId: "c", startedAt: day0 + 50_000, costUsd: 3, totalTokens: 30 }),
    ];
    const points = overTime(records, "day");
    expect(points).toHaveLength(2);
    // Sorted ascending by bucketStart.
    expect(points[0].bucketStart).toBe(day0);
    expect(points[0].costUsd).toBeCloseTo(5, 6); // b + c
    expect(points[0].totalTokens).toBe(50);
    expect(points[0].sessions).toBe(2);
    expect(points[1].bucketStart).toBe(day1);
    expect(points[1].costUsd).toBeCloseTo(1, 6);
    expect(points[1].sessions).toBe(1);
  });

  it("hour bucketing groups within the same hour and splits across hours", () => {
    const hourMs = 60 * 60 * 1000;
    const records = [
      rec({ sessionId: "a", startedAt: 1_000, costUsd: 1, totalTokens: 5 }),
      rec({ sessionId: "b", startedAt: hourMs - 1, costUsd: 1, totalTokens: 5 }), // same hour bucket (0)
      rec({ sessionId: "c", startedAt: hourMs + 500, costUsd: 1, totalTokens: 5 }), // next hour bucket
    ];
    const points = overTime(records, "hour");
    expect(points).toHaveLength(2);
    expect(points[0].bucketStart).toBe(0);
    expect(points[0].sessions).toBe(2);
    expect(points[1].bucketStart).toBe(hourMs);
    expect(points[1].sessions).toBe(1);
  });

  it("skips records with no usable startedAt rather than creating a bogus bucket", () => {
    const records = [
      rec({ sessionId: "a", startedAt: 0 }),
      rec({ sessionId: "b", startedAt: -100 }),
      rec({ sessionId: "c", startedAt: NaN }),
      rec({ sessionId: "d", startedAt: 5_000, costUsd: 9 }),
    ];
    const points = overTime(records, "day");
    expect(points).toHaveLength(1);
    expect(points[0].sessions).toBe(1);
    expect(points[0].costUsd).toBeCloseTo(9, 6);
  });
});
