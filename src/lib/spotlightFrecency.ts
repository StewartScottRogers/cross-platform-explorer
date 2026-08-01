/**
 * Spotlight frecency (CPE-1216, backed by CPE-952's pure ranking core + CPE-1214's `spotlight_frecent`
 * command): tracks how often and how recently each item was opened/revealed from the overlay, and
 * drives its empty-query default view ("most-frecent first"). Persisted like every other
 * settings.ts-owned document — this module owns the pure add/parse/serialize (mirroring
 * colorRulesStore.ts's pattern), settings.ts owns the actual `KEYS` entry + read/write.
 */
import { commands } from "./bindings.gen";
import type { Visit, SpotSection, SpotResult } from "./bindings.gen";

/** Hard cap on how many paths the frecency store remembers. Beyond this, the STALEST entries (oldest
 *  `last_used_s`) are pruned first when a new visit is recorded — a lightweight decay so years of use
 *  can't grow the persisted document without bound. */
export const MAX_FRECENT_ENTRIES = 300;

/** How many ranked paths the overlay's empty-query default view shows. */
export const DEFAULT_VIEW_LIMIT = 20;

function isVisit(x: unknown): x is Visit {
  if (!x || typeof x !== "object") return false;
  const o = x as Record<string, unknown>;
  return (
    typeof o.path === "string" &&
    typeof o.count === "number" &&
    Number.isFinite(o.count) &&
    typeof o.last_used_s === "number" &&
    Number.isFinite(o.last_used_s)
  );
}

/** Parse a persisted frecency document, dropping any malformed entry — a corrupt/hand-edited value
 *  degrades to `[]` rather than crashing startup (colorRulesStore.ts's tolerant-parse convention). */
export function parseFrecent(json: string | null | undefined): Visit[] {
  if (!json) return [];
  try {
    const raw = JSON.parse(json);
    return Array.isArray(raw) ? raw.filter(isVisit) : [];
  } catch {
    return [];
  }
}

export function serializeFrecent(list: Visit[]): string {
  return JSON.stringify(list);
}

/**
 * Record a visit (open/reveal/run) to `path`: increments its count and bumps `last_used_s` to `nowS`
 * if it's already tracked, or appends a fresh entry (`count: 1`) otherwise. Returns a new list (pure).
 * When the store grows past `MAX_FRECENT_ENTRIES`, the stalest entries are dropped first.
 */
export function recordVisit(list: Visit[], path: string, nowS: number = Math.floor(Date.now() / 1000)): Visit[] {
  const i = list.findIndex((v) => v.path === path);
  let next: Visit[];
  if (i === -1) {
    next = [...list, { path, count: 1, last_used_s: nowS }];
  } else {
    next = list.slice();
    next[i] = { ...next[i], count: next[i].count + 1, last_used_s: nowS };
  }
  if (next.length > MAX_FRECENT_ENTRIES) {
    next = next.slice().sort((a, b) => b.last_used_s - a.last_used_s).slice(0, MAX_FRECENT_ENTRIES);
  }
  return next;
}

/**
 * The overlay's empty-query default view (CPE-1216 acceptance: "most-frecent first"): rank `visits`
 * through the backend `spotlight_frecent` core and wrap the resulting paths as a single "recent"
 * `SpotSection` — the same shape a scored search returns, so the overlay renders it through the exact
 * same row component. Nothing was fuzzy-matched for a blank query, so `score`/`positions` are neutral
 * (0 / `[]` — no highlight). Empty `visits` (nothing recorded yet) short-circuits to `[]` without a
 * round-trip.
 */
export async function defaultView(visits: Visit[], nowS: number, limit = DEFAULT_VIEW_LIMIT): Promise<SpotSection[]> {
  if (visits.length === 0) return [];
  const paths = await commands.spotlightFrecent(visits, nowS, limit);
  if (paths.length === 0) return [];
  const results: SpotResult[] = paths.map((text) => ({ text, kind: "recent", score: 0, positions: [] }));
  return [{ kind: "recent", results }];
}
