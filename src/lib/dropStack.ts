// Drop Stack (CPE-1530, foundation of epic CPE-1489): a persistent "shelf" of files/folders accumulated
// across DIFFERENT folder navigations — Path Finder's Drop Stack. The idea: pick items up while browsing
// one folder, navigate elsewhere, pick up more, then (in a later ticket) move/copy the whole accumulated
// set at once. This ticket is the store + persistence ONLY — no UI, no context-menu entry point, no
// panel. CPE-1531 (entry points), CPE-1532 (panel), and CPE-1533 (move/copy-all) all import from here.
//
// Shape mirrors transfers.ts: pure reducer functions (unit-testable with no DOM/store) plus a thin
// writable tail. Persistence rides the EXISTING settings.ts settings.json document (a `dropStack` field,
// see settings.ts's loadDropStack/saveDropStack) rather than a second ad-hoc mechanism — this module
// never touches localStorage directly.

import { writable, type Readable } from "svelte/store";
import { loadDropStack, saveDropStack } from "./settings";

/** One shelved item: which path, the folder it was picked up FROM, and when. */
export interface DropStackEntry {
  path: string;
  addedFrom: string;
  addedAt: number;
}

/**
 * Add one or more paths, all picked up from the same source folder `addedFrom`. De-duped by path: a path
 * already on the stack is NOT duplicated as a second entry — instead it's refreshed (new `addedFrom` /
 * `addedAt`) and moved to the END of the returned list.
 *
 * Decision (ticket asked to pick one and note it): the list order is "oldest pick-up first, most
 * recently touched last" — appending new entries, and re-adding an already-shelved path bumps it to the
 * end rather than leaving it stranded at its original position or silently no-op'ing. This gives the
 * future panel (CPE-1532) a stable "most recent at the bottom" reading order, and means re-selecting an
 * item you'd already dropped (e.g. from a different folder) visibly surfaces it as freshly touched.
 *
 * Pure: never mutates `list`; a duplicate path *within* the same `paths` batch is also collapsed to one
 * entry (last occurrence wins) so a single call can't itself create duplicates.
 */
export function addEntries(
  list: DropStackEntry[],
  paths: string[],
  addedFrom: string,
  now: number = Date.now(),
): DropStackEntry[] {
  if (paths.length === 0) return list;
  const uniquePaths = [...new Set(paths)];
  const incoming = new Set(uniquePaths);
  const without = list.filter((e) => !incoming.has(e.path));
  const added = uniquePaths.map((path) => ({ path, addedFrom, addedAt: now }));
  return [...without, ...added];
}

/** Remove a single entry by path. No-op (returns an equivalent list) if the path is absent. Pure. */
export function removeEntry(list: DropStackEntry[], path: string): DropStackEntry[] {
  return list.filter((e) => e.path !== path);
}

const store = writable<DropStackEntry[]>([]);

/** Reactive drop-stack list for the (future, CPE-1532) panel to render. Empty until `initDropStack()`
 *  loads the persisted stack, or an item is added. */
export const dropStackEntries: Readable<DropStackEntry[]> = store;

let started = false;
/** Load the persisted stack from settings.json into the reactive store. Call once at app start
 *  (idempotent) — same pattern as transfers.ts's `initTransfers()`. */
export function initDropStack(): void {
  if (started) return;
  started = true;
  store.set(loadDropStack());
}

/** Add one or more paths, all picked up from `addedFrom`, to the reactive stack and persist. */
export function addToDropStack(paths: string | string[], addedFrom: string): void {
  const list = Array.isArray(paths) ? paths : [paths];
  store.update((l) => {
    const next = addEntries(l, list, addedFrom);
    saveDropStack(next);
    return next;
  });
}

/** Remove one path from the reactive stack and persist. */
export function removeFromDropStack(path: string): void {
  store.update((l) => {
    const next = removeEntry(l, path);
    saveDropStack(next);
    return next;
  });
}

/** Empty the reactive stack and persist. */
export function clearDropStack(): void {
  store.set([]);
  saveDropStack([]);
}
