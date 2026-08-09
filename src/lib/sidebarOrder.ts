// Sidebar section ORDER (CPE-1520, epic CPE-660): a persisted array of section ids controlling the
// left-pane's top-to-bottom layout, so a user can drag sections into the arrangement they want. This is
// deliberately a separate store from sidebarSections.ts (CPE-675, which tracks only open/collapsed) —
// order and collapse state are orthogonal and must not interact: reordering never touches which
// sections are open, and collapsing/expanding never touches the order.
//
// Pure reducers below are unit-tested; the store tail persists via the shared localStorage layer, same
// pattern as sidebarSections.ts.

import { writable } from "svelte/store";
import { lsGet, lsSet } from "./persist";

const KEY = "cpe.sidebarOrder";

/** Every section id, in the app's original hardcoded top-to-bottom order (Sidebar.svelte, pre-CPE-1520).
 *  This doubles as the default order and the source of truth for which ids are valid. */
export const SECTION_IDS = [
  "agents",
  "favorites",
  "tags",
  "smart",
  "savedSearch",
  "explore",
  "places",
  "drives",
  "network",
] as const;

export type SectionId = (typeof SECTION_IDS)[number];

/** The default order — a fresh array each call so callers can't mutate the shared constant. */
export function defaultOrder(): string[] {
  return [...SECTION_IDS];
}

/**
 * Reconcile a candidate order against the known section ids: drop anything unrecognized (a stale id
 * from a since-removed section) and any duplicate, then append any known id missing from the candidate
 * — in its default relative position — so a future new section can never be stranded off-list for a
 * user with an older persisted order. Pure.
 */
export function mergeOrder(candidate: string[]): string[] {
  const known = new Set<string>(SECTION_IDS);
  const seen = new Set<string>();
  const out: string[] = [];
  for (const id of candidate) {
    if (known.has(id) && !seen.has(id)) {
      out.push(id);
      seen.add(id);
    }
  }
  for (const id of SECTION_IDS) if (!seen.has(id)) out.push(id);
  return out;
}

/** Parse the persisted JSON into a valid section order; unset/malformed/legacy JSON (not an array of
 *  strings) falls back to the default order — same defensive shape as `parseSections`. Pure. */
export function parseOrder(json: string | null): string[] {
  if (!json) return defaultOrder();
  try {
    const v = JSON.parse(json);
    if (!Array.isArray(v) || !v.every((x) => typeof x === "string")) return defaultOrder();
    return mergeOrder(v);
  } catch {
    return defaultOrder();
  }
}

/** Move `id` by one slot (`-1` up / `1` down); a no-op at either end or for an unknown id. Pure —
 *  returns a new array. */
export function moveBy(order: string[], id: string, dir: -1 | 1): string[] {
  const i = order.indexOf(id);
  const j = i + dir;
  if (i === -1 || j < 0 || j >= order.length) return order;
  const next = order.slice();
  [next[i], next[j]] = [next[j], next[i]];
  return next;
}

/** Move `id` all the way to the top or bottom. A no-op for an unknown id. Pure. */
export function moveToEnd(order: string[], id: string, end: "start" | "end"): string[] {
  if (!order.includes(id)) return order;
  const rest = order.filter((x) => x !== id);
  return end === "start" ? [id, ...rest] : [...rest, id];
}

/** Move `id` to sit immediately before/after `targetId` (drag-and-drop reorder). A no-op if either id
 *  is unknown, or they're the same id. Pure — returns a new array. */
export function moveNextTo(order: string[], id: string, targetId: string, before: boolean): string[] {
  if (id === targetId || !order.includes(id) || !order.includes(targetId)) return order;
  const rest = order.filter((x) => x !== id);
  const ti = rest.indexOf(targetId);
  const insertAt = before ? ti : ti + 1;
  rest.splice(insertAt, 0, id);
  return rest;
}

/** A `Record<id, cssOrder>` map for driving each section's CSS `order` (spaced by 10, so a value can
 *  slot in at `n-1`/`n+1` without renumbering the rest — see Sidebar.svelte's separators). Pure. */
export function orderStyleMap(order: string[]): Record<string, number> {
  const out: Record<string, number> = {};
  order.forEach((id, i) => (out[id] = i * 10));
  return out;
}

const store = writable<string[]>(parseOrder(lsGet(KEY)));
store.subscribe((s) => lsSet(KEY, JSON.stringify(s)));

/** Reactive, persisted section order. */
export const sidebarOrder = store;

/** Move a section up/down by one slot and persist. */
export function moveSection(id: string, dir: -1 | 1): void {
  store.update((o) => moveBy(o, id, dir));
}

/** Drop `id` next to `targetId` (drag-and-drop reorder) and persist. */
export function reorderSection(id: string, targetId: string, before: boolean): void {
  store.update((o) => moveNextTo(o, id, targetId, before));
}

/** Reset the order back to default and persist. */
export function resetSidebarOrder(): void {
  store.set(defaultOrder());
}
