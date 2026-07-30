import type { TimelineEntry } from "./agentActivity";
import type { Checkpoint } from "./bindings.gen";

/**
 * Pure helpers behind the Agent Watch **replay scrubber** (CPE-1094). `agentTimeline` (durable,
 * cap 300, entries `{id, kind, path, at}`, newest-first) is already a scrubbable event log; these
 * functions turn a selected timestamp `t` into "what the timeline looked like at that moment"
 * without touching any store or DOM, so they're trivially unit-testable and reusable from the
 * Replay tab in `AgentTimeline.svelte`.
 *
 * Kept deliberately dependency-free (no Svelte imports) — pure data in, pure data out.
 */

/** The scrub range spanning a timeline's earliest and latest `at` (epoch ms). */
export interface SliderRange {
  firstAt: number;
  lastAt: number;
}

/**
 * The `[firstAt, lastAt]` span of `timeline`, or `null` when there's nothing meaningful to scrub
 * (fewer than 2 entries — the ticket's disabled/empty state). A range with `firstAt === lastAt` is
 * a valid *degenerate* range (every entry landed in the same batch/timestamp) — callers must treat
 * that as "one point on the slider", not divide by the (zero) span.
 */
export function sliderRange(timeline: TimelineEntry[]): SliderRange | null {
  if (timeline.length < 2) return null;
  let firstAt = Infinity;
  let lastAt = -Infinity;
  for (const e of timeline) {
    if (e.at < firstAt) firstAt = e.at;
    if (e.at > lastAt) lastAt = e.at;
  }
  return { firstAt, lastAt };
}

/**
 * Where `t` sits within `range` as a 0..1 fraction, for positioning a slider thumb. Division-safe:
 * a degenerate range (`firstAt === lastAt`) always reports `0` rather than `NaN`/`Infinity`.
 */
export function sliderFraction(range: SliderRange, t: number): number {
  if (range.lastAt === range.firstAt) return 0;
  const frac = (t - range.firstAt) / (range.lastAt - range.firstAt);
  if (frac < 0) return 0;
  if (frac > 1) return 1;
  return frac;
}

/** The timeline frozen at `t`: every entry with `at <= t`, in the same order as `timeline` (so the
 *  Replay list renders identically to the Live list, just truncated to the chosen moment). */
export function entriesUpTo(timeline: TimelineEntry[], t: number): TimelineEntry[] {
  return timeline.filter((e) => e.at <= t);
}

/**
 * The "current" entry at moment `t` — the most recent entry with `at <= t`, i.e. what the agent had
 * just done at that point in the replay. Ties (same `at`, from a coalesced batch) break on `id`
 * (higher `id` = inserted later = happened later within the batch), so the result is deterministic
 * even when a whole batch shares one timestamp. Returns `null` when no entry qualifies (`t` before
 * every entry, or an empty timeline).
 */
export function currentEntry(timeline: TimelineEntry[], t: number): TimelineEntry | null {
  let best: TimelineEntry | null = null;
  for (const e of timeline) {
    if (e.at > t) continue;
    if (!best || e.at > best.at || (e.at === best.at && e.id > best.id)) best = e;
  }
  return best;
}

/** `timeline` sorted oldest-first (ascending `at`, then `id` to break batch ties) — the order the
 *  transport controls (step/play) advance through, independent of the store's newest-first order. */
export function chronological(timeline: TimelineEntry[]): TimelineEntry[] {
  return [...timeline].sort((a, b) => a.at - b.at || a.id - b.id);
}

/** The next distinct timestamp after `t` in `timeline`'s chronological order, or `null` if `t` is
 *  already at or past the last entry. Used by step-forward/play to advance the scrubber. */
export function nextTimestamp(timeline: TimelineEntry[], t: number): number | null {
  for (const e of chronological(timeline)) {
    if (e.at > t) return e.at;
  }
  return null;
}

/** The previous distinct timestamp before `t` in `timeline`'s chronological order, or `null` if `t`
 *  is already at or before the first entry. Used by step-back. */
export function prevTimestamp(timeline: TimelineEntry[], t: number): number | null {
  const chrono = chronological(timeline);
  let prev: number | null = null;
  for (const e of chrono) {
    if (e.at >= t) break;
    prev = e.at;
  }
  return prev;
}

/**
 * The play-interval period (ms) for a given playback `speed` multiplier (CPE-1104): `baseMs / speed`
 * — e.g. 2× plays twice as fast (half the period), 0.5× plays half as fast (double the period).
 * Division-safe: the Replay tab only ever passes a positive speed from its fixed selector set (0.5,
 * 1, 2, 4 — never a user-typed value), but a non-positive `speed` here falls back to `baseMs`
 * unchanged (1×) rather than dividing by zero or going negative/infinite.
 */
export function cadenceForSpeed(baseMs: number, speed: number): number {
  if (!(speed > 0)) return baseMs;
  return baseMs / speed;
}

/** A kind that carries a captured before/after diff (mirrors `AgentTimeline.svelte`'s `isWrite`). */
export const isWriteKind = (k: TimelineEntry["kind"]): boolean => k === "created" || k === "modified";

/**
 * Whether `path` was written (created/modified) more than once anywhere in `timeline`. `agentDiffs`
 * only retains the *latest* before/after per path (CPE-744), so a multiply-written path's diff at
 * an earlier scrub position `t` is really the session's final diff, not what changed at `t` — the
 * Replay tab must badge this rather than silently showing a diff for the wrong moment.
 */
export function isMultiplyEdited(timeline: TimelineEntry[], path: string): boolean {
  let count = 0;
  for (const e of timeline) {
    if (e.path === path && isWriteKind(e.kind)) {
      count++;
      if (count > 1) return true;
    }
  }
  return false;
}

/**
 * A checkpoint (CPE-1123/1125) positioned on the Replay scrubber track (CPE-1126, epic CPE-732).
 * `fraction` is a 0..1 track position for `left: fraction*100%`, computed with the SAME math as
 * `sliderFraction`. `inRange` is whether the checkpoint's `ts` actually falls within the recorded
 * `[firstAt, lastAt]` window — a checkpoint taken before the session began recording, or after the
 * last recorded action, is `inRange:false` and its `fraction` is CLAMPED to the nearest track edge
 * (0 or 1) so its pin still shows, pinned to the edge, rather than overflowing off the track.
 */
export interface CheckpointMarker {
  cp: Checkpoint;
  fraction: number;
  inRange: boolean;
}

/**
 * Map each checkpoint's `ts` onto the scrubber track for `range`. Pure — no store/DOM. Mirrors
 * `sliderFraction`'s division-safety and clamping so markers line up exactly with the thumb:
 * - `range === null` (nothing to scrub, <2 entries) → `[]`.
 * - degenerate `firstAt === lastAt` → every marker at `fraction:0` (never divides by zero); a
 *   marker is `inRange` only if its `ts` equals that single point.
 * - otherwise `fraction = (ts - firstAt) / (lastAt - firstAt)`, clamped to `[0, 1]`, with
 *   `inRange = firstAt <= ts <= lastAt`.
 * Input order is preserved (the caller decides ordering).
 */
export function checkpointMarkers(
  range: SliderRange | null,
  checkpoints: Checkpoint[],
): CheckpointMarker[] {
  if (!range) return [];
  const { firstAt, lastAt } = range;
  const degenerate = lastAt === firstAt;
  return checkpoints.map((cp) => {
    const inRange = cp.ts >= firstAt && cp.ts <= lastAt;
    let fraction: number;
    if (degenerate) {
      fraction = 0;
    } else {
      const raw = (cp.ts - firstAt) / (lastAt - firstAt);
      fraction = raw < 0 ? 0 : raw > 1 ? 1 : raw;
    }
    return { cp, fraction, inRange };
  });
}
