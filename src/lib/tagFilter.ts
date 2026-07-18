// Filter helpers for the tags feature (CPE-639, epic CPE-614). Pure + DOM-free so the "show only
// files tagged X" view and the sidebar tag counts are unit-tested independently of any component.

import type { TagStore } from "./tags";

/** Keep only entries whose path carries `tag` (per the store). A blank tag returns everything. Pure. */
export function filterEntriesByTag<T extends { path: string }>(entries: T[], store: TagStore, tag: string): T[] {
  if (!tag) return entries;
  return entries.filter((e) => store[e.path]?.tags.includes(tag) ?? false);
}

/** Whether any entry in `entries` carries `tag` — used to hide an empty tag filter. Pure. */
export function anyEntryHasTag<T extends { path: string }>(entries: T[], store: TagStore, tag: string): boolean {
  return entries.some((e) => store[e.path]?.tags.includes(tag) ?? false);
}
