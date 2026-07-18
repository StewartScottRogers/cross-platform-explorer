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

/** Every tag with the number of paths carrying it, most-used first then alphabetical (mirrors the
 *  backend `tag_counts`). Drives the sidebar Tags section. Pure. */
export function tagCounts(store: TagStore): [string, number][] {
  const counts = new Map<string, number>();
  for (const entry of Object.values(store)) {
    for (const t of entry.tags) counts.set(t, (counts.get(t) ?? 0) + 1);
  }
  return [...counts.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]));
}
