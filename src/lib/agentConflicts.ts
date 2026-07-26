import type { TimelineEntry } from "./agentActivity";
import type { AgentSession } from "./sidecar";

/**
 * Activity-overlap radar (CPE-1100) — the last GUI slice of the conflict-radar work. Folds the
 * actor-tagged session timeline (CPE-1099/1101: every {@link TimelineEntry} now carries `actor`, a
 * `sessionId` / `"user"` / `"unknown"`) into "two distinct actors touched this path within a short
 * window" signals for the Agent Watch drawer's Radar tab.
 *
 * Deliberately hedged: a raw filesystem watcher can't *prove* two writes came from unrelated
 * processes vs. the same agent touching its own file twice — so this surfaces as "activity overlap",
 * never "conflict" (see the ⚠ note in the ticket and AGENT-WATCH.md). The honest unknown-vs-agent
 * upgrade (CPE-1099c) is a deferred, separate ticket.
 */

/** How close together (ms) two different actors' touches of the same path must land to count as an
 *  overlap — "a few seconds" per the ticket. A named const so the window is a single, testable knob. */
export const OVERLAP_WINDOW_MS = 5000;

/** One detected overlap: a path touched by ≥2 distinct actors within {@link OVERLAP_WINDOW_MS}. */
export interface ActivityOverlap {
  path: string;
  /** Deduped actor ids (sessionId / "user" / "unknown"), in first-seen order within the window. */
  actors: string[];
  /** The timestamp of the most recent touch in the qualifying window (epoch ms). */
  lastAt: number;
}

/** An entry's actor, defaulting to `"unknown"` when untagged (older/degenerate data). */
const actorOf = (e: TimelineEntry): string => e.actor || "unknown";

/**
 * Fold timeline entries into activity-overlap signals (pure). Groups by `path`; for each path, slides
 * a `windowMs`-wide trailing window across its entries (sorted oldest-first) and, whenever a window
 * contains ≥2 *distinct* actor values, records that as a candidate overlap ending at that window's
 * latest timestamp. Two same-valued actors (including two untagged/"unknown" touches — attribution
 * can't tell them apart) never count as an overlap with themselves; dedupe is per actor value, not
 * per event. Only the most-recent qualifying window per path is kept. Returns overlaps sorted
 * newest-first (matches "most recent first" in the radar UI). Empty/degenerate input -> `[]`.
 */
export function foldOverlaps(
  entries: TimelineEntry[],
  windowMs: number = OVERLAP_WINDOW_MS,
): ActivityOverlap[] {
  if (entries.length === 0) return [];

  const byPath = new Map<string, TimelineEntry[]>();
  for (const e of entries) {
    if (!e.path) continue;
    const list = byPath.get(e.path);
    if (list) list.push(e);
    else byPath.set(e.path, [e]);
  }

  const overlaps: ActivityOverlap[] = [];
  for (const [path, list] of byPath) {
    const sorted = [...list].sort((a, b) => a.at - b.at);

    let bestActors: string[] | null = null;
    let bestAt = -Infinity;
    let left = 0;
    for (let right = 0; right < sorted.length; right++) {
      while (sorted[right].at - sorted[left].at > windowMs) left++;

      const seen = new Set<string>();
      const ordered: string[] = [];
      for (let k = left; k <= right; k++) {
        const a = actorOf(sorted[k]);
        if (!seen.has(a)) {
          seen.add(a);
          ordered.push(a);
        }
      }
      if (ordered.length >= 2 && sorted[right].at >= bestAt) {
        bestAt = sorted[right].at;
        bestActors = ordered;
      }
    }

    if (bestActors) overlaps.push({ path, actors: bestActors, lastAt: bestAt });
  }

  return overlaps.sort((a, b) => b.lastAt - a.lastAt);
}

/** Whether an overlap involves the best-effort `"unknown"` actor tag — the radar shows a small hedge
 *  note in this case since attribution couldn't be resolved to a specific agent/user for one touch. */
export function overlapHasUnknown(overlap: ActivityOverlap): boolean {
  return overlap.actors.includes("unknown");
}

/** A coarse relative-time label ("just now" / "12s ago" / "3m ago" / "2h ago") for an overlap's
 *  `lastAt`, given the current time. Pure — no timer of its own; the radar re-renders (and this
 *  re-evaluates) whenever the underlying timeline changes, which is often enough while watching and
 *  harmless (a slightly stale label) while idle. */
export function relativeLabel(at: number, now: number): string {
  const diffMs = Math.max(0, now - at);
  if (diffMs < 1000) return "just now";
  const s = Math.floor(diffMs / 1000);
  if (s < 60) return `${s}s ago`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  return `${h}h ago`;
}

/** Map an actor id to a friendly display label: `"user"` -> "You", `"unknown"` -> "Unknown", a
 *  resolvable `sessionId` -> that agent's name (joined against {@link AgentSession}s), else the id
 *  itself shortened. Pure — takes the current session list rather than reading a store. */
export function friendlyActor(actor: string, sessions: AgentSession[]): string {
  if (actor === "user") return "You";
  if (actor === "unknown" || !actor) return "Unknown";
  const s = sessions.find((x) => x.sessionId === actor);
  if (s && s.agentName) return s.agentName;
  return actor.length > 10 ? `${actor.slice(0, 10)}…` : actor;
}
