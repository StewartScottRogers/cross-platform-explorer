// Pure saved-query model + evaluator (CPE-986, epic CPE-978 "Smart folders & saved searches"). A saved
// search is a serialisable, named bundle of `Condition`s (reused from CPE-774) combined with all/any, so a
// smart folder is a thin listing filtered through `evaluateSavedSearch`. No DOM/IO — unit-tested here — so
// the store (persistence) and the editor UI are thin wrappers over this. Mirrors the idioms of the
// neighbouring rule engines (colorRules / watchRules / selectMatch): one Condition matcher, tolerant parse.

import type { DirEntry } from "./types";
import { matchesCondition, isValidCondition, type Condition } from "./colorRules";

/**
 * A serialisable named query. `conditions` are combined with `match`:
 * - `"all"` — an entry must satisfy every condition (AND); an empty condition list matches everything.
 * - `"any"` — an entry must satisfy at least one condition (OR); an empty condition list matches nothing.
 */
export interface SavedSearch {
  id: string;
  name: string;
  conditions: Condition[];
  match: "all" | "any";
}

/** Whether `entry` satisfies the saved search, combining its conditions with all/any. Pure. */
export function matchesSavedSearch(entry: DirEntry, search: SavedSearch, now: number): boolean {
  const { conditions, match } = search;
  if (match === "all") {
    // Vacuous truth: an "all" search with no conditions matches everything (an unfiltered smart folder).
    return conditions.every((c) => matchesCondition(entry, c, now));
  }
  // "any" with no conditions matches nothing — there is no condition to satisfy.
  return conditions.some((c) => matchesCondition(entry, c, now));
}

/**
 * The entries that satisfy the saved search, combining its conditions via all/any through the existing
 * `matchesCondition`. Returns the matching `DirEntry`s (order preserved), not indices — smart folders
 * consume a filtered listing. Pure.
 */
export function evaluateSavedSearch(entries: DirEntry[], search: SavedSearch, now: number): DirEntry[] {
  return entries.filter((e) => matchesSavedSearch(e, search, now));
}

/** Serialise a saved search to JSON for persistence. */
export function serializeSavedSearch(search: SavedSearch): string {
  return JSON.stringify(search);
}

/**
 * Structural guard for a persisted saved search — validates each field (not just presence), reusing
 * `isValidCondition` so a corrupted/hand-edited condition is caught here rather than throwing later in
 * `matchesCondition`. Requires a non-blank `name`. Pure.
 */
function isValidSavedSearch(x: unknown): x is SavedSearch {
  if (!x || typeof x !== "object") return false;
  const o = x as Record<string, unknown>;
  return (
    typeof o.id === "string" &&
    typeof o.name === "string" &&
    o.name.trim() !== "" &&
    (o.match === "all" || o.match === "any") &&
    Array.isArray(o.conditions) &&
    o.conditions.every(isValidCondition)
  );
}

/**
 * Parse a persisted saved search. Tolerant: bad JSON / wrong shape / missing or blank name / an invalid
 * condition → `null`. Never throws. Pure.
 */
export function parseSavedSearch(json: string): SavedSearch | null {
  try {
    const raw = JSON.parse(json);
    return isValidSavedSearch(raw) ? raw : null;
  } catch {
    return null;
  }
}
