import type { TimelineEntry } from "./agentActivity";

/**
 * Competing-rename detection (CPE-1118, epic CPE-730) — a pure frontend fold over the
 * actor-tagged, `from`→`to`-paired rename stream (CPE-1117) that finds where two agents/actors'
 * renames contend: the same source renamed to different targets (**divergence**), or different
 * sources renamed onto the same target (**collision**). Mirrors the set logic in the sidecar's
 * `conflict_rename.rs` (CPE-1067) exactly — same divergence/collision definitions, same
 * same-agent/no-op exclusions, same sorted-by-path output — adapted to fold over
 * {@link TimelineEntry} instead of a `RenameActivity` list, since the sidecar module isn't wired
 * into this process (see the conflict-radar close-plan research entry).
 *
 * Deliberately hedged like `agentConflicts.ts`: a raw filesystem watcher can't *prove* two
 * renames came from unrelated actors vs. the same agent correcting itself, so the radar surfaces
 * this as "competing renames" with best-effort wording, never a hard "conflict".
 */

/** What kind of rename contention a path is under. */
export type RenameConflictKind = "divergence" | "collision";

/** A detected rename conflict: `path` is the shared `from` (divergence) or shared `to`
 *  (collision); `actors` are the distinct actor ids involved, sorted ascending for a stable
 *  radar order (mirrors `conflict_rename.rs`). `lastAt` is the most recent contributing rename's
 *  timestamp, for the radar's relative-time label — the sidecar module has no notion of time; this
 *  is a UI-only addition. */
export interface RenameConflict {
  path: string;
  kind: RenameConflictKind;
  actors: string[];
  lastAt: number;
}

/** An entry's actor, defaulting to `"unknown"` when untagged (mirrors `agentConflicts.ts`). */
const actorOf = (e: TimelineEntry): string => e.actor || "unknown";

interface Bucket {
  actors: Set<string>;
  /** Distinct `to` values for a `by-from` bucket, or distinct `from` values for a `by-to` bucket. */
  others: Set<string>;
  lastAt: number;
}

function fold(
  renames: Array<{ from: string; to: string; actor: string; at: number }>,
  keyOf: (from: string, to: string) => string,
  otherOf: (from: string, to: string) => string,
): Map<string, Bucket> {
  const buckets = new Map<string, Bucket>();
  for (const r of renames) {
    const key = keyOf(r.from, r.to);
    let b = buckets.get(key);
    if (!b) {
      b = { actors: new Set(), others: new Set(), lastAt: -Infinity };
      buckets.set(key, b);
    }
    b.actors.add(r.actor);
    b.others.add(otherOf(r.from, r.to));
    if (r.at > b.lastAt) b.lastAt = r.at;
  }
  return buckets;
}

/** Ordinal string comparison (mirrors Rust's default `&str` `Ord`, avoiding locale surprises). */
const ordinalCmp = (a: string, b: string): number => (a < b ? -1 : a > b ? 1 : 0);

/**
 * Detect competing renames across `entries` (pure).
 *
 * - **Divergence**: 2+ distinct actors rename the same `from` to different `to` paths — conflict
 *   keyed on `from`.
 * - **Collision**: 2+ distinct actors rename different `from` paths onto the same `to` — conflict
 *   keyed on `to`.
 *
 * Only `kind === "renamed"` entries carrying both `from` and `to` participate (older/degenerate
 * single-path renames are silently skipped, per CPE-1117's graceful-degradation contract). A
 * `from === to` no-op rename is ignored entirely, and same-actor repeated renames of a path never
 * count as a conflict — only distinct actors do. Results are sorted by path, with each conflict's
 * actors sorted, for a stable radar order. Empty/degenerate input -> `[]`.
 */
export function foldRenameConflicts(entries: TimelineEntry[]): RenameConflict[] {
  const renames: Array<{ from: string; to: string; actor: string; at: number }> = [];
  for (const e of entries) {
    if (e.kind !== "renamed" || !e.from || !e.to || e.from === e.to) continue;
    renames.push({ from: e.from, to: e.to, actor: actorOf(e), at: e.at });
  }
  if (renames.length === 0) return [];

  const byFrom = fold(
    renames,
    (from) => from,
    (_from, to) => to,
  );
  const byTo = fold(
    renames,
    (_from, to) => to,
    (from) => from,
  );

  const out: RenameConflict[] = [];
  for (const [from, b] of byFrom) {
    if (b.actors.size >= 2 && b.others.size >= 2) {
      out.push({ path: from, kind: "divergence", actors: [...b.actors].sort(ordinalCmp), lastAt: b.lastAt });
    }
  }
  for (const [to, b] of byTo) {
    if (b.actors.size >= 2 && b.others.size >= 2) {
      out.push({ path: to, kind: "collision", actors: [...b.actors].sort(ordinalCmp), lastAt: b.lastAt });
    }
  }

  return out.sort((a, b) => ordinalCmp(a.path, b.path));
}

/** Hedged, kind-specific radar wording (consistent with `agentConflicts.ts`'s "overlap", never a
 *  hard "conflict" claim — a filesystem watcher can't prove the renames were actually contending). */
export function renameConflictNote(kind: RenameConflictKind): string {
  return kind === "divergence"
    ? "Renamed the same file to different names — best-effort, not a guaranteed conflict."
    : "Different files renamed onto the same name — best-effort, not a guaranteed conflict.";
}
