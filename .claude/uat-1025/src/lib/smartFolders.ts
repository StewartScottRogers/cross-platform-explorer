// Smart folders (CPE-667, epic CPE-614): a saved, named query surfaced as a virtual folder that
// re-evaluates live. v1 scopes a query to a single tag — opening the smart folder lists every path in
// the tag store carrying that tag, across whatever real folders they live in, and refreshes as tags
// change. The helpers below are pure + DOM-free (unit-tested); the store tail persists to localStorage.
// Idle by default: with no smart folders saved, the section is hidden and nothing runs.

import { writable, type Writable } from "svelte/store";
import { lsGet, lsSet } from "./persist";
import { hasTag, type TagStore } from "./tags";

/** One saved smart folder. v1: a single-`tag` query. `id` is stable across rename for the active check. */
export interface SmartFolder {
  id: string;
  name: string;
  tag: string;
}

const STORE_KEY = "cpe.smartFolders";

/** A short unique id — enough to distinguish saved folders without pulling in a uuid dependency. */
function newId(): string {
  return `sf_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 7)}`;
}

/** Parse the persisted JSON into a clean `SmartFolder[]`, dropping anything malformed. Pure. */
export function parseSmartFolders(json: string | null): SmartFolder[] {
  if (!json) return [];
  try {
    const v = JSON.parse(json);
    if (!Array.isArray(v)) return [];
    return v
      .filter((x): x is SmartFolder =>
        !!x && typeof x.id === "string" && typeof x.name === "string" && typeof x.tag === "string")
      .map((x) => ({ id: x.id, name: x.name, tag: x.tag }));
  } catch {
    return [];
  }
}

/** Append a smart folder for `tag` named `name` (trimmed). A folder with the same name AND tag already
    present is returned unchanged (no duplicates). Pure — returns a new array. */
export function addSmartFolder(list: SmartFolder[], name: string, tag: string): SmartFolder[] {
  const n = name.trim();
  const t = tag.trim();
  if (!n || !t) return list;
  if (list.some((sf) => sf.name === n && sf.tag === t)) return list;
  return [...list, { id: newId(), name: n, tag: t }];
}

/** Rename the smart folder with `id` (no-op on empty/unknown). Pure. */
export function renameSmartFolder(list: SmartFolder[], id: string, name: string): SmartFolder[] {
  const n = name.trim();
  if (!n) return list;
  return list.map((sf) => (sf.id === id ? { ...sf, name: n } : sf));
}

/** Remove the smart folder with `id`. Pure. */
export function removeSmartFolder(list: SmartFolder[], id: string): SmartFolder[] {
  return list.filter((sf) => sf.id !== id);
}

/** Every path in the tag store matching the smart folder's query, sorted. Pure — the single source of
    truth for what a smart folder contains, so it's the unit-tested core. */
export function smartFolderPaths(store: TagStore, sf: SmartFolder): string[] {
  if (!store || !sf?.tag) return [];
  return Object.keys(store)
    .filter((path) => hasTag(store, path, sf.tag))
    .sort();
}

const store: Writable<SmartFolder[]> = writable(parseSmartFolders(lsGet(STORE_KEY)));
// Persist every change so saved folders survive restart (AC), reusing the shared persist layer.
store.subscribe((list) => lsSet(STORE_KEY, JSON.stringify(list)));

/** Reactive list of saved smart folders (persisted). */
export const smartFolders = store;

/** Save a new tag-query smart folder (used by "Save as smart folder"). */
export function saveSmartFolder(name: string, tag: string): void {
  store.update((list) => addSmartFolder(list, name, tag));
}

/** Rename a saved smart folder by id. */
export function renameSaved(id: string, name: string): void {
  store.update((list) => renameSmartFolder(list, id, name));
}

/** Delete a saved smart folder by id. */
export function removeSaved(id: string): void {
  store.update((list) => removeSmartFolder(list, id));
}
