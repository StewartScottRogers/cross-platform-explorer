// Persisted store for saved searches (CPE-1228, epic CPE-978 "Smart folders & saved searches").
// `savedSearch.ts` (CPE-986) owns the pure `SavedSearch` model + evaluator + serialize/parse; this file
// is the thin persistence wrapper that wires that model to localStorage, mirroring `smartFolders.ts`'s
// writable-store shape exactly: `lsGet`/`lsSet` via `persist.ts`, a module-level `writable` seeded from a
// tolerant parse, `store.subscribe` to persist every change, and add/rename/remove helpers. No UI here —
// that's CPE-1229.

import { writable, type Writable } from "svelte/store";
import { lsGet, lsSet } from "./persist";
import { type SavedSearch, parseSavedSearch } from "./savedSearch";
import type { Condition } from "./colorRules";

const STORE_KEY = "cpe.savedSearches";

/** A short unique id — enough to distinguish saved searches without pulling in a uuid dependency. */
function newId(): string {
  return `ss_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 7)}`;
}

/** Parse the persisted JSON into a clean `SavedSearch[]`, dropping anything malformed. Pure. */
export function parseSavedSearches(json: string | null): SavedSearch[] {
  if (!json) return [];
  try {
    const raw = JSON.parse(json);
    if (!Array.isArray(raw)) return [];
    // Reuse the single-item tolerant parser (via its serialise form) so validation logic lives in one
    // place — a corrupted/hand-edited entry is dropped here rather than throwing later in the evaluator.
    return raw
      .map((x) => parseSavedSearch(JSON.stringify(x)))
      .filter((x): x is SavedSearch => x !== null);
  } catch {
    return [];
  }
}

/** Serialise the whole list for persistence. */
export function serializeSavedSearches(list: SavedSearch[]): string {
  return JSON.stringify(list);
}

/** Append a new saved search named `name` (trimmed) over `conditions`/`match`. Pure — returns a new
    array; a blank name is a no-op. */
export function addSavedSearchTo(
  list: SavedSearch[],
  name: string,
  conditions: Condition[],
  match: "all" | "any",
): SavedSearch[] {
  const n = name.trim();
  if (!n) return list;
  return [...list, { id: newId(), name: n, conditions, match }];
}

/** Rename the saved search with `id` (no-op on empty/unknown). Pure. */
export function renameSavedSearchIn(list: SavedSearch[], id: string, name: string): SavedSearch[] {
  const n = name.trim();
  if (!n) return list;
  return list.map((s) => (s.id === id ? { ...s, name: n } : s));
}

/** Remove the saved search with `id`. Pure. */
export function removeSavedSearchFrom(list: SavedSearch[], id: string): SavedSearch[] {
  return list.filter((s) => s.id !== id);
}

const store: Writable<SavedSearch[]> = writable(parseSavedSearches(lsGet(STORE_KEY)));
// Persist every change so saved searches survive restart (AC), reusing the shared persist layer.
store.subscribe((list) => lsSet(STORE_KEY, serializeSavedSearches(list)));

/** Reactive list of saved searches (persisted). */
export const savedSearches = store;

/** Save a new saved search. */
export function addSavedSearch(name: string, conditions: Condition[], match: "all" | "any"): void {
  store.update((list) => addSavedSearchTo(list, name, conditions, match));
}

/** Rename a saved search by id. */
export function renameSavedSearch(id: string, name: string): void {
  store.update((list) => renameSavedSearchIn(list, id, name));
}

/** Delete a saved search by id. */
export function removeSavedSearch(id: string): void {
  store.update((list) => removeSavedSearchFrom(list, id));
}
