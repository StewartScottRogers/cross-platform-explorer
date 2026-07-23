// Frontend tag service (CPE-636, epic CPE-614): a reactive mirror of the backend tag store
// (load_tags / set_tags / tag_counts, CPE-635). The store maps an absolute path to its tags + a
// single colour label. The reducer-style helpers below are pure + DOM-free so they're unit-tested;
// the store tail just wires the `invoke` calls. Idle by default — an empty store costs nothing, so
// the plain explorer is unaffected until a path is actually tagged.

import { writable, get, type Readable } from "svelte/store";
import { invoke } from "./invoke";

/** One path's tags plus its single colour label (mirror of the Rust `TagEntry`). */
export interface TagEntry {
  tags: string[];
  label: string;
}

/** The whole store: absolute path → its entry (mirror of the Rust `TagStore`). */
export type TagStore = Record<string, TagEntry>;

/**
 * The fixed colour palette for the single per-path label. `""` (no label) maps to `""` so callers can
 * uniformly look a label up; every other key resolves to a hex colour. Pure data — no DOM.
 */
export const LABEL_COLORS: Record<string, string> = {
  "": "",
  red: "#e05252",
  orange: "#e0902e",
  yellow: "#e0c72e",
  green: "#4ca65a",
  blue: "#4a8fe0",
  purple: "#9a5ad0",
  grey: "#8a8f98",
};

/** The hex colour for a label, or `""` for the empty/unknown label. Pure. */
export function labelColor(label: string): string {
  return LABEL_COLORS[label] ?? "";
}

/** The entry for a path, or an empty entry if the path is untagged (or the store is empty/absent).
    Pure (never mutates the store). */
export function entryFor(store: TagStore, path: string): TagEntry {
  return store?.[path] ?? { tags: [], label: "" };
}

/** Whether `path` currently carries `tag`. Pure. */
export function hasTag(store: TagStore, path: string, tag: string): boolean {
  return entryFor(store, path).tags.includes(tag);
}

/** Every distinct tag across the store, sorted ascending. Pure. */
export function allTags(store: TagStore): string[] {
  const set = new Set<string>();
  for (const entry of Object.values(store)) {
    for (const tag of entry.tags) set.add(tag);
  }
  return [...set].sort();
}

const store = writable<TagStore>({});

/** Reactive tag store: absolute path → its {tags, label} (empty until something is tagged). */
export const tags: Readable<TagStore> = store;

let loaded = false;
/** Load the tag store from the backend once (idempotent). Call at app start. */
export async function initTags(): Promise<void> {
  if (loaded) return;
  loaded = true;
  const s = await invoke<TagStore>("load_tags");
  // The backend always returns a map; guard against a nullish response so the store is never null
  // (an empty map keeps the plain explorer untouched).
  store.set(s ?? {});
}

/** Replace one path's tags + label; the store is updated from the returned whole store. */
export async function setEntryTags(path: string, tags: string[], label: string): Promise<void> {
  const updated = await invoke<TagStore>("set_tags", { path, tags, label });
  store.set(updated ?? {});
}

/** Tag usage counts, most-used first (`[tag, count][]`), straight from the backend. */
export function tagCounts(): Promise<[string, number][]> {
  return invoke<[string, number][]>("tag_counts");
}

/** Re-key a path's tags after an in-app rename/move so they follow the file (CPE-652). No-op if untagged. */
export async function retagPath(from: string, to: string): Promise<void> {
  const updated = await invoke<TagStore>("retag_path", { from, to });
  store.set(updated ?? {});
}

/** Rename tag `old` → `next` across every path (CPE-653). An empty `next` deletes it. */
export async function renameTag(old: string, next: string): Promise<void> {
  const updated = await invoke<TagStore>("rename_tag", { old, newName: next });
  store.set(updated ?? {});
}

/** Remove a tag from every path (CPE-653). */
export async function deleteTag(tag: string): Promise<void> {
  const updated = await invoke<TagStore>("delete_tag", { tag });
  store.set(updated ?? {});
}

/** Merge an exported tag store (JSON) into the current one (CPE-654). */
export async function importTags(json: string): Promise<void> {
  const updated = await invoke<TagStore>("import_tags", { json });
  store.set(updated ?? {});
}

/** The current tag store serialized as pretty JSON, for export (CPE-654). */
export function exportTags(): string {
  return JSON.stringify(get(store), null, 2);
}
