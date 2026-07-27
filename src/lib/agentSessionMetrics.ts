import { writable, type Readable } from "svelte/store";
import type { AgentSession, SessionAnnouncement } from "./sidecar";
import type { FsDiff } from "./agentDiffs";
import { currentCost, type AgentCost } from "./agentCost";
import { commands, type SessionMetricsRecord } from "./bindings.gen";

/**
 * Live per-session cost+activity metrics for the Agent Watch Cost tab (CPE-1107, epic CPE-731 slice a).
 *
 * Everything here is a **frontend JOIN on `sessionId`** of streams the app already receives — there is
 * no new sidecar capture and no new `listen()`/timer. A running accumulator is folded **inside the
 * existing ingest paths**: `agentSessions.ts`'s `ingestSessionState` calls {@link ingestSessionAnnouncement}
 * on every `session:<json>` announcement, and `agentDiffs.ts`'s `ingestDiff` calls
 * {@link ingestDiffsForMetrics} on every `ai-console://fs-diff` batch. This module has no listener of its
 * own — it is purely a second fold hung off the two that already exist, preserving the
 * single-shared-listener invariant (AGENT-WATCH.md).
 *
 * Why a running accumulator rather than scanning `agentDiffs`/`agentTimeline` at read time: both those
 * stores are **capped** (`DIFF_CAP`/`TIMELINE_CAP`) so a long session's early activity would be evicted
 * and an end-of-session re-scan would undercount. Folding as each batch arrives is cap-immune.
 *
 * `SessionAccumulator` holds the raw per-session tallies (identity + counters); `SessionMetrics` is the
 * derived, render-ready shape produced by {@link deriveSessionMetrics}, which also joins in tokens/cost
 * from `agentCost` (kept as a separate store — this module doesn't depend on its shape beyond the three
 * numeric fields it reads). Ratios are computed division-safe: a zero/undefined denominator yields
 * `undefined` rather than `NaN`/`Infinity`, and the UI hides the row instead of rendering garbage.
 *
 * Strictly additive + idle-by-default: the store is empty and nothing accrues until a session actually
 * announces itself, and `clearAgentSessionMetrics()` resets it when the Agent Deck fully stops watching
 * (mirrors `agentCost.ts`/`agentDiffs.ts` — "off means off"). A per-session `ended` does NOT clear the
 * whole store (only full teardown does); it just stamps `endedAt` so the session's final tallies remain
 * visible until the next full stop, exactly like `agentCost`.
 *
 * Persistence (flushing a `SessionMetricsRecord` to a `metrics_journal` on session end) is a later slice
 * (CPE-731b) — this module is the live, in-memory panel only. The field names here are chosen to align
 * with what that slice will persist (see the plan for the full rationale).
 */

/** One session's raw running tallies — folded from batches as they arrive, cap-immune. Identity fields
 *  are "" until a `started` announcement supplies them (a `fs-diff` can arrive first in a race and will
 *  lazily create an entry with blank identity, which `started` then fills in). */
export interface SessionAccumulator {
  sessionId: string;
  agentId: string;
  agentName: string;
  provider: string;
  model: string;
  cwd: string;
  /** Epoch ms when `started` was folded, or `null` if this entry was created lazily by an `fs-diff`
   *  that raced ahead of the session announcement. */
  startedAt: number | null;
  /** Epoch ms when `ended` was folded, or `null` while the session is still live. */
  endedAt: number | null;
  /** Distinct paths touched this session (a Set encoded as a lookup record so the store stays a plain
   *  JSON-shaped object, matching the rest of the codebase's fold-store style). */
  filesTouched: Record<string, true>;
  /** Total fs-diff writes attributed to this session (not deduped by path — repeated edits all count). */
  editCount: number;
  /** Σ |after.length − before.length| across every write this session made. */
  churnBytes: number;
}

/** A fresh, empty accumulator for a session id (the lazy-create / `started` reset value). */
export function emptySessionAccumulator(sessionId: string): SessionAccumulator {
  return {
    sessionId,
    agentId: "",
    agentName: "",
    provider: "",
    model: "",
    cwd: "",
    startedAt: null,
    endedAt: null,
    filesTouched: {},
    editCount: 0,
    churnBytes: 0,
  };
}

const store = writable<Record<string, SessionAccumulator>>({});

/** Reactive per-session accumulator map (keyed by sessionId). Empty when not watching. */
export const agentSessionMetrics: Readable<Record<string, SessionAccumulator>> = store;

/** Session ids already flushed to the metrics journal this watch (CPE-1113), so a re-reconcile can't
 *  double-write a row. Cleared on full teardown ({@link clearAgentSessionMetrics}); an id is un-marked
 *  when its session (re)starts so a genuine restart of the same id can flush again. */
const flushedSessionIds = new Set<string>();

/** Fold a `started` announcement into the map (pure): (re)creates a fresh accumulator stamped with
 *  `startedAt=now` and the announced identity — a genuine restart of that session id starts clean. */
export function foldSessionStarted(
  prev: Record<string, SessionAccumulator>,
  session: AgentSession,
  now: number,
): Record<string, SessionAccumulator> {
  return {
    ...prev,
    [session.sessionId]: {
      ...emptySessionAccumulator(session.sessionId),
      agentId: session.agentId,
      agentName: session.agentName,
      provider: session.provider,
      model: session.model,
      cwd: session.cwd,
      startedAt: now,
    },
  };
}

/** Fold an `ended` announcement into the map (pure): stamps `endedAt=now` on the existing accumulator.
 *  A no-op if the session never accrued an entry, or was already ended (idempotent). */
export function foldSessionEnded(
  prev: Record<string, SessionAccumulator>,
  sessionId: string,
  now: number,
): Record<string, SessionAccumulator> {
  const existing = prev[sessionId];
  if (!existing || existing.endedAt !== null) return prev;
  return { ...prev, [sessionId]: { ...existing, endedAt: now } };
}

/** Fold one `SessionAnnouncement` into the map (pure) — dispatches to started/ended. */
export function foldSessionAnnouncement(
  prev: Record<string, SessionAccumulator>,
  ann: SessionAnnouncement,
  now: number,
): Record<string, SessionAccumulator> {
  return ann.event === "started"
    ? foldSessionStarted(prev, ann.session, now)
    : foldSessionEnded(prev, ann.session.sessionId, now);
}

/** Actors that never accrue session metrics — not a real agent session id. */
const NON_SESSION_ACTORS = new Set(["user", "unknown"]);

/** Fold a batch of `fs-diff` items into the map (pure), attributing each to its `actor` (a sessionId):
 *  adds the path to `filesTouched`, `editCount++`, `churnBytes += |after.len − before.len|`. Items with
 *  no actor, or an actor of `"user"`/`"unknown"`, are skipped — those aren't agent sessions. An actor
 *  with no existing entry yet (its `started` hasn't arrived, or raced behind this diff) lazily gets a
 *  blank-identity accumulator so no activity is ever dropped. */
export function foldDiffsForMetrics(
  prev: Record<string, SessionAccumulator>,
  items: FsDiff[],
): Record<string, SessionAccumulator> {
  let next = prev;
  let changed = false;
  for (const it of items) {
    const actor = it.actor;
    if (!actor || NON_SESSION_ACTORS.has(actor)) continue;
    if (!changed) {
      next = { ...prev };
      changed = true;
    }
    const existing = next[actor] ?? emptySessionAccumulator(actor);
    const churn = Math.abs(it.after.length - it.before.length);
    const filesTouched: Record<string, true> = existing.filesTouched[it.path]
      ? existing.filesTouched
      : { ...existing.filesTouched, [it.path]: true as const };
    next[actor] = {
      ...existing,
      filesTouched,
      editCount: existing.editCount + 1,
      churnBytes: existing.churnBytes + churn,
    };
  }
  return changed ? next : prev;
}

/** Fold a `session:<json>` announcement into the store (called from `agentSessions.ts`'s
 *  `ingestSessionState` — no listener of its own). Exposed for headless tests. */
export function ingestSessionAnnouncement(ann: SessionAnnouncement, now = Date.now()): void {
  // A genuine restart of a session id resets its accumulator (foldSessionStarted) — un-mark it so its
  // next end flushes a fresh row (CPE-1113).
  if (ann.event === "started") flushedSessionIds.delete(ann.session.sessionId);
  store.update((prev) => foldSessionAnnouncement(prev, ann, now));
}

/** Fold a normalized `fs-diff` batch into the store (called from `agentDiffs.ts`'s `ingestDiff` — no
 *  listener of its own). Exposed for headless tests. */
export function ingestDiffsForMetrics(items: FsDiff[]): void {
  if (items.length) store.update((prev) => foldDiffsForMetrics(prev, items));
}

/** Test/introspection helper: the current accumulator map synchronously. */
export function currentSessionMetrics(): Record<string, SessionAccumulator> {
  let snapshot: Record<string, SessionAccumulator> = {};
  store.subscribe((v) => (snapshot = v))();
  return snapshot;
}

/** Clear the accumulator map (on full stop-watching), so a stale session never bleeds into a new one.
 *  Mirrors `clearAgentCost()`/`clearDiffs()` — call from the same place those are cleared. Also resets
 *  the flushed-ids guard (CPE-1113) so the next watch starts clean. */
export function clearAgentSessionMetrics(): void {
  store.set({});
  flushedSessionIds.clear();
}

// ---------------------------------------------------------------------------------------------
// Derived, render-ready metrics + division-safe ratio/format helpers.
// ---------------------------------------------------------------------------------------------

/** The fuller, render-ready per-session metric set the Cost tab displays (CPE-1107 DoD). Token/cost
 *  fields are joined in from `agentCost` by the caller (see {@link deriveSessionMetrics}); everything
 *  else comes straight from the accumulator. Ratio fields are `undefined` when their denominator is
 *  zero/invalid — the UI hides the row rather than rendering `NaN`/`Infinity`. */
export interface SessionMetrics {
  sessionId: string;
  agentId: string;
  agentName: string;
  provider: string;
  model: string;
  cwd: string;
  startedAt: number | null;
  endedAt: number | null;
  /** `endedAt - startedAt`, or `now - startedAt` while still live; `null` if never started. */
  wallClockMs: number | null;
  inputTokens: number;
  outputTokens: number;
  totalTokens: number;
  costUsd: number;
  filesTouched: number;
  editCount: number;
  churnBytes: number;
  /** Total tokens per minute of wall-clock, or `undefined` if wall-clock is 0/unknown. */
  tokensPerMinute?: number;
  /** USD cost per distinct file touched, or `undefined` if no files were touched. */
  usdPerFile?: number;
  /** Churn bytes per 1,000 tokens, or `undefined` if no tokens were reported. */
  churnPer1kTokens?: number;
}

/** Wall-clock duration for an accumulator (pure, injectable `now` for tests): `endedAt - startedAt` if
 *  ended, else `now - startedAt` while live, or `null` if `startedAt` is unknown. */
export function wallClockMs(acc: Pick<SessionAccumulator, "startedAt" | "endedAt">, now = Date.now()): number | null {
  if (acc.startedAt === null) return null;
  return (acc.endedAt ?? now) - acc.startedAt;
}

/** `count` per minute of `ms` milliseconds, division-safe: `undefined` if `ms` is null/non-finite/≤0 or
 *  `count` is invalid, never `NaN`/`Infinity`. */
export function ratioPerMinute(count: number, ms: number | null): number | undefined {
  if (ms === null || !Number.isFinite(ms) || ms <= 0) return undefined;
  if (!Number.isFinite(count) || count < 0) return undefined;
  return count / (ms / 60000);
}

/** `numerator / denominator`, division-safe: `undefined` if either side is non-finite/negative or the
 *  denominator is `≤0`, never `NaN`/`Infinity`. General-purpose ratio helper for `usdPerFile` /
 *  `churnPer1kTokens`. */
export function safeRatio(numerator: number, denominator: number): number | undefined {
  if (!Number.isFinite(numerator) || numerator < 0) return undefined;
  if (!Number.isFinite(denominator) || denominator <= 0) return undefined;
  return numerator / denominator;
}

/** Build the render-ready {@link SessionMetrics} for one accumulator, joining in tokens/cost from the
 *  `agentCost` record for this session (absent/malformed → zeros, mirroring `totalTokens`'s NaN-safety).
 *  Pure aside from the injectable `now` (defaults to the real clock) so tests can pin wall-clock math. */
export function deriveSessionMetrics(
  acc: SessionAccumulator,
  cost?: Pick<AgentCost, "inputTokens" | "outputTokens" | "costUsd">,
  now = Date.now(),
): SessionMetrics {
  const inputTokens = cost && Number.isFinite(cost.inputTokens) && cost.inputTokens > 0 ? cost.inputTokens : 0;
  const outputTokens = cost && Number.isFinite(cost.outputTokens) && cost.outputTokens > 0 ? cost.outputTokens : 0;
  const totalTok = inputTokens + outputTokens;
  const costUsd = cost && Number.isFinite(cost.costUsd) && cost.costUsd > 0 ? cost.costUsd : 0;
  const filesTouched = Object.keys(acc.filesTouched).length;
  const ms = wallClockMs(acc, now);
  return {
    sessionId: acc.sessionId,
    agentId: acc.agentId,
    agentName: acc.agentName,
    provider: acc.provider,
    model: acc.model,
    cwd: acc.cwd,
    startedAt: acc.startedAt,
    endedAt: acc.endedAt,
    wallClockMs: ms,
    inputTokens,
    outputTokens,
    totalTokens: totalTok,
    costUsd,
    filesTouched,
    editCount: acc.editCount,
    churnBytes: acc.churnBytes,
    tokensPerMinute: ratioPerMinute(totalTok, ms),
    usdPerFile: safeRatio(costUsd, filesTouched),
    churnPer1kTokens: safeRatio(acc.churnBytes, totalTok / 1000),
  };
}

// ---------------------------------------------------------------------------------------------
// Human-format helpers (byte counts, durations, ratios) — all division/NaN-safe.
// ---------------------------------------------------------------------------------------------

/** A byte count as a human-readable size (`"0 B"`, `"512 B"`, `"1.5 KB"`, `"3 MB"`, …), NaN/negative
 *  safe (renders `"0 B"` rather than `"NaN B"`). */
export function formatBytes(n: number): string {
  if (!Number.isFinite(n) || n < 0) return "0 B";
  if (n < 1024) return `${Math.trunc(n)} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  // One decimal place under 10 (e.g. "1.5 KB"), whole above (e.g. "15 KB") — but never a trailing
  // ".0" when the value is exactly whole (e.g. "3 MB", not "3.0 MB").
  const rounded = v < 10 ? Math.round(v * 10) / 10 : Math.round(v);
  const text = Number.isInteger(rounded) ? String(rounded) : rounded.toFixed(1);
  return `${text} ${units[i]}`;
}

/** A millisecond duration as a human-readable span (`"12s"`, `"3m 5s"`, `"2h 10m"`, …). `null`/NaN/
 *  negative renders `"—"` (an unknown wall-clock, e.g. no `started` seen yet). */
export function formatDuration(ms: number | null): string {
  if (ms === null || !Number.isFinite(ms) || ms < 0) return "—";
  const totalSec = Math.floor(ms / 1000);
  if (totalSec < 60) return `${totalSec}s`;
  const totalMin = Math.floor(totalSec / 60);
  const sec = totalSec % 60;
  if (totalMin < 60) return sec > 0 ? `${totalMin}m ${sec}s` : `${totalMin}m`;
  const hours = Math.floor(totalMin / 60);
  const min = totalMin % 60;
  return min > 0 ? `${hours}h ${min}m` : `${hours}h`;
}

/** A per-minute rate (`"12.3/min"`), or `null` if `n` is `undefined`/non-finite/negative — the caller
 *  hides the row on `null` rather than rendering a bogus rate. */
export function formatPerMinute(n: number | undefined): string | null {
  if (n === undefined || !Number.isFinite(n) || n < 0) return null;
  return `${n.toFixed(1)}/min`;
}

// ---------------------------------------------------------------------------------------------
// Flush-on-session-end persistence (CPE-1113, epic CPE-731 slice b).
//
// When a watched session ends, one `SessionMetricsRecord` is flushed to the sibling metrics journal
// (`agent-metrics/history.jsonl`, via `commands.metricsRecord`) so the cost dashboard can show
// cross-session history (731c). The record is built from the LIVE accumulator (which holds its own
// cap-immune copy) at the `stopAgentWatch(id)` seam + the full-stop loop, BEFORE the stores clear.
// Advisory / best-effort — a flush failure is swallowed so it can never break teardown or the live view.
// ---------------------------------------------------------------------------------------------

/** Only sessions that actually STARTED and produced *some* signal are worth a row — skip never-started
 *  (lazy fs-diff) entries and empty started-but-did-nothing sessions, so the journal has no junk rows. */
export function sessionMetricsWorthPersisting(m: SessionMetrics): boolean {
  if (m.startedAt === null) return false; // lazy fs-diff entry, never a real `started`
  return (
    m.filesTouched > 0 ||
    m.editCount > 0 ||
    m.churnBytes > 0 ||
    m.totalTokens > 0 ||
    m.costUsd > 0
  );
}

/** Build the persisted, wire-shaped {@link SessionMetricsRecord} from a derived {@link SessionMetrics}
 *  (pure; injectable `now` for tests). `endedAt`/`wallClockMs` are coerced to concrete numbers — a flush
 *  only happens at/after session end, but if `endedAt` is somehow still null it's stamped `now`. */
export function buildMetricsRecord(m: SessionMetrics, now = Date.now()): SessionMetricsRecord {
  const startedAt = m.startedAt ?? 0;
  const endedAt = m.endedAt ?? now;
  const wallClockMs = Math.max(0, endedAt - startedAt);
  return {
    sessionId: m.sessionId,
    agentId: m.agentId,
    agentName: m.agentName,
    provider: m.provider,
    model: m.model,
    cwd: m.cwd,
    startedAt,
    endedAt,
    wallClockMs,
    inputTokens: m.inputTokens,
    outputTokens: m.outputTokens,
    totalTokens: m.totalTokens,
    costUsd: m.costUsd,
    filesTouched: m.filesTouched,
    churnBytes: m.churnBytes,
    editCount: m.editCount,
  };
}

/** Flush one ended session's metrics row to the journal (idempotent — at most once per session end).
 *  Reads the live accumulator + its `agentCost` snapshot, builds the record, and appends it. A no-op for
 *  an unknown, already-flushed, or not-worth-persisting session. Errors are swallowed (advisory). */
export async function flushSession(sessionId: string, now = Date.now()): Promise<void> {
  if (flushedSessionIds.has(sessionId)) return; // already flushed this watch
  const acc = currentSessionMetrics()[sessionId];
  if (!acc) return; // unknown session
  const m = deriveSessionMetrics(acc, currentCost()[sessionId], now);
  if (!sessionMetricsWorthPersisting(m)) return; // empty / never-started
  flushedSessionIds.add(sessionId); // mark BEFORE the await so a re-reconcile can't double-write
  try {
    await commands.metricsRecord(buildMetricsRecord(m, now));
  } catch (e) {
    // Advisory/best-effort persistence — never break teardown or the live view on a flush failure.
    console.debug("session metrics flush failed:", e);
  }
}

/** Flush every currently-known session's metrics row (the full-stop path), each idempotently. Call
 *  BEFORE {@link clearAgentSessionMetrics} so nothing is lost when the whole Agent Deck stops. */
export async function flushAllSessions(now = Date.now()): Promise<void> {
  for (const id of Object.keys(currentSessionMetrics())) await flushSession(id, now);
}
