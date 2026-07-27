import type { SessionMetricsRecord } from "./bindings.gen";

/**
 * Cross-session cost/activity rollup (CPE-1114, epic CPE-731 slice c — final slice). Pure fold over
 * the persisted `SessionMetricsRecord[]` history (`commands.metricsHistory()`, CPE-1113) — no I/O, no
 * store, no side effects. Mirrors the tested Rust rollup formulas so the numbers the dashboard shows
 * match the backend's own math (they're never wired together — the backend rollup exists for the
 * unwired PTY-capture path; this is the TS-side equivalent over the frontend-flushed journal):
 *
 * - Totals/per-model/per-agent grouping + division-safe averages mirror
 *   `sidecar/ai-console/src/fleet_metrics.rs::aggregate`.
 * - The USD/file, tokens/file, churn/1k-tokens, tokens/minute ratios mirror
 *   `sidecar/ai-console/src/efficiency.rs`.
 *
 * Every ratio/average is division-safe: an empty `records` array (or a zero denominator) yields `0`
 * for the always-present total/average fields (mirroring the Rust `SessionAverages::default()`), and
 * `undefined` — never `NaN`/`Infinity` — for the optional ratio fields (mirroring
 * `agentSessionMetrics.ts`'s `safeRatio`/`ratioPerMinute` convention, which the UI already knows to
 * hide a row on).
 */

/** Summable counters for one session, a rollup group, or the grand totals. */
export interface MetricsTotals {
  sessions: number;
  inputTokens: number;
  outputTokens: number;
  totalTokens: number;
  costUsd: number;
  wallClockMs: number;
  filesTouched: number;
  churnBytes: number;
  editCount: number;
}

/** A fresh all-zero {@link MetricsTotals} — the fold's identity element. */
export function emptyTotals(): MetricsTotals {
  return {
    sessions: 0,
    inputTokens: 0,
    outputTokens: 0,
    totalTokens: 0,
    costUsd: 0,
    wallClockMs: 0,
    filesTouched: 0,
    churnBytes: 0,
    editCount: 0,
  };
}

/** `n` if finite and non-negative, else `0` — guards a malformed/adversarial record field from ever
 *  poisoning a running sum with `NaN`. */
function safe(n: number): number {
  return Number.isFinite(n) && n >= 0 ? n : 0;
}

/** Add one record's counters into `totals` (pure — returns a new object). Mirrors
 *  `fleet_metrics::add_saturating`'s field-by-field sum; JS numbers don't need saturation, but every
 *  field is still `safe()`-guarded so one malformed row can't NaN-poison the whole rollup. */
function addRecord(totals: MetricsTotals, r: SessionMetricsRecord): MetricsTotals {
  return {
    sessions: totals.sessions + 1,
    inputTokens: totals.inputTokens + safe(r.inputTokens),
    outputTokens: totals.outputTokens + safe(r.outputTokens),
    totalTokens: totals.totalTokens + safe(r.totalTokens),
    costUsd: totals.costUsd + safe(r.costUsd),
    wallClockMs: totals.wallClockMs + safe(r.wallClockMs),
    filesTouched: totals.filesTouched + safe(r.filesTouched),
    churnBytes: totals.churnBytes + safe(r.churnBytes),
    editCount: totals.editCount + safe(r.editCount),
  };
}

/** A {@link MetricsTotals} group keyed by model, carrying the key for display. */
export interface ModelRollupRow extends MetricsTotals {
  model: string;
}

/** A {@link MetricsTotals} group keyed by `agentId`, carrying the id + a display name. */
export interface AgentRollupRow extends MetricsTotals {
  agentId: string;
  agentName: string;
}

/** Division-safe per-session averages across the whole rollup. Mirrors `fleet_metrics::SessionAverages`
 *  — all-zero (never `NaN`) when there are no sessions. */
export interface MetricsAverages {
  avgCostUsd: number;
  avgTokens: number;
  avgWallClockMs: number;
  avgFilesTouched: number;
  avgChurnBytes: number;
}

/** Division-safe efficiency ratios over the grand totals. Mirrors `efficiency.rs`'s per-session ratio
 *  functions applied to the rolled-up totals, plus `usdPerSession` (fleet-level, no direct Rust
 *  counterpart). `undefined` — never `NaN`/`Infinity` — when the denominator is zero. */
export interface MetricsRatios {
  /** Total tokens per minute of total wall-clock. `undefined` if total wall-clock is 0. */
  tokensPerMinute?: number;
  /** Total USD per session. `undefined` if there are no sessions. */
  usdPerSession?: number;
  /** Total USD per distinct file touched. `undefined` if no files were touched. */
  usdPerFile?: number;
  /** Total churn bytes per 1,000 total tokens. `undefined` if no tokens were reported. */
  churnPer1kTokens?: number;
}

export interface MetricsRollup {
  totals: MetricsTotals;
  /** Sorted (by model id) for a stable, deterministic dashboard order — mirrors the Rust `BTreeMap`. */
  byModel: Map<string, ModelRollupRow>;
  /** Sorted (by agent id) for a stable, deterministic dashboard order — mirrors the Rust `BTreeMap`. */
  byAgent: Map<string, AgentRollupRow>;
  averages: MetricsAverages;
  ratios: MetricsRatios;
}

/** `numerator / denominator`, division-safe: `undefined` if the denominator is non-finite/`<= 0`, or
 *  the numerator is non-finite/negative — never `NaN`/`Infinity`. */
function safeRatio(numerator: number, denominator: number): number | undefined {
  if (!Number.isFinite(numerator) || numerator < 0) return undefined;
  if (!Number.isFinite(denominator) || denominator <= 0) return undefined;
  return numerator / denominator;
}

/** `count` per minute of `ms` milliseconds, division-safe (mirrors `agentSessionMetrics.ts`'s
 *  `ratioPerMinute`). */
function ratioPerMinute(count: number, ms: number): number | undefined {
  if (!Number.isFinite(ms) || ms <= 0) return undefined;
  if (!Number.isFinite(count) || count < 0) return undefined;
  return count / (ms / 60000);
}

/**
 * Roll up `records` into fleet-wide totals, per-model and per-agent breakdowns, division-safe
 * averages, and division-safe efficiency ratios. An empty array yields an all-zero rollup (empty
 * maps, zeroed totals/averages, all-`undefined` ratios) — never throws, never divides by zero.
 */
export function rollup(records: SessionMetricsRecord[]): MetricsRollup {
  let totals = emptyTotals();
  const modelAcc = new Map<string, MetricsTotals>();
  const agentAcc = new Map<string, { totals: MetricsTotals; agentName: string }>();

  for (const r of records) {
    totals = addRecord(totals, r);

    const modelKey = r.model || "(unknown)";
    modelAcc.set(modelKey, addRecord(modelAcc.get(modelKey) ?? emptyTotals(), r));

    const agentKey = r.agentId || "(unknown)";
    const prevAgent = agentAcc.get(agentKey);
    agentAcc.set(agentKey, {
      totals: addRecord(prevAgent?.totals ?? emptyTotals(), r),
      // Keep the first non-empty display name seen for this id — every record for the same agent id
      // should carry the same name anyway; this just avoids a later blank overwriting a good one.
      agentName: prevAgent?.agentName || r.agentName || agentKey,
    });
  }

  const byModel = new Map<string, ModelRollupRow>(
    [...modelAcc.keys()].sort().map((model) => [model, { model, ...(modelAcc.get(model) as MetricsTotals) }]),
  );
  const byAgent = new Map<string, AgentRollupRow>(
    [...agentAcc.keys()].sort().map((agentId) => {
      const a = agentAcc.get(agentId) as { totals: MetricsTotals; agentName: string };
      return [agentId, { agentId, agentName: a.agentName, ...a.totals }];
    }),
  );

  const n = totals.sessions;
  const averages: MetricsAverages =
    n === 0
      ? { avgCostUsd: 0, avgTokens: 0, avgWallClockMs: 0, avgFilesTouched: 0, avgChurnBytes: 0 }
      : {
          avgCostUsd: totals.costUsd / n,
          avgTokens: totals.totalTokens / n,
          avgWallClockMs: totals.wallClockMs / n,
          avgFilesTouched: totals.filesTouched / n,
          avgChurnBytes: totals.churnBytes / n,
        };

  const ratios: MetricsRatios = {
    tokensPerMinute: ratioPerMinute(totals.totalTokens, totals.wallClockMs),
    usdPerSession: safeRatio(totals.costUsd, n),
    usdPerFile: safeRatio(totals.costUsd, totals.filesTouched),
    churnPer1kTokens: safeRatio(totals.churnBytes, totals.totalTokens / 1000),
  };

  return { totals, byModel, byAgent, averages, ratios };
}

// ---------------------------------------------------------------------------------------------
// Over-time bucketing (for the History tab's sparkline/bar view).
// ---------------------------------------------------------------------------------------------

export type TimeBucket = "hour" | "day";

const BUCKET_MS: Record<TimeBucket, number> = {
  hour: 60 * 60 * 1000,
  day: 24 * 60 * 60 * 1000,
};

/** One bucketed point on the over-time view: the bucket's UTC-aligned start (epoch ms) and the sum of
 *  cost/tokens/session-count for records whose `startedAt` fell in that bucket. */
export interface OverTimePoint {
  bucketStart: number;
  costUsd: number;
  totalTokens: number;
  sessions: number;
}

/**
 * Bucket `records` by `bucket` (day or hour), summing cost + tokens + session count per bucket.
 * Buckets are UTC-epoch-aligned (`Math.floor(startedAt / bucketMs) * bucketMs`) rather than
 * local-calendar-aligned, so the bucketing is pure/deterministic and doesn't depend on the host's
 * timezone (important for CI, which runs the same suite across OSes). Returns points sorted ascending
 * by `bucketStart`; a record with no usable `startedAt` (non-finite or `<= 0`) is skipped rather than
 * poisoning a bucket. Empty input yields `[]`.
 */
export function overTime(records: SessionMetricsRecord[], bucket: TimeBucket): OverTimePoint[] {
  const size = BUCKET_MS[bucket];
  const acc = new Map<number, OverTimePoint>();

  for (const r of records) {
    const at = r.startedAt;
    if (!Number.isFinite(at) || at <= 0) continue;
    const bucketStart = Math.floor(at / size) * size;
    const existing = acc.get(bucketStart);
    acc.set(bucketStart, {
      bucketStart,
      costUsd: (existing?.costUsd ?? 0) + safe(r.costUsd),
      totalTokens: (existing?.totalTokens ?? 0) + safe(r.totalTokens),
      sessions: (existing?.sessions ?? 0) + 1,
    });
  }

  return [...acc.values()].sort((a, b) => a.bucketStart - b.bucketStart);
}
