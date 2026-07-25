// Frontend tag service (CPE-636, epic CPE-614): a reactive mirror of the backend tag store
// (load_tags / set_tags / tag_counts, CPE-635). The store maps an absolute path to its tags + a
// single colour label. The reducer-style helpers below are pure + DOM-free so they're unit-tested;
// the store tail just wires the `invoke` calls. Idle by default — an empty store costs nothing, so
// the plain explorer is unaffected until a path is actually tagged.

import { writable, get, type Readable } from "svelte/store";
import { commands } from "./bindings.gen"; // typed client (CPE-964)
import { unwrap } from "./invoke";

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
  const s = unwrap(await commands.loadTags()) as TagStore;
  // The backend always returns a map; guard against a nullish response so the store is never null
  // (an empty map keeps the plain explorer untouched).
  store.set(s ?? {});
}

/** Replace one path's tags + label; the store is updated from the returned whole store. */
export async function setEntryTags(path: string, tags: string[], label: string): Promise<void> {
  const updated = unwrap(await commands.setTags(path, tags, label)) as TagStore;
  store.set(updated ?? {});
}

// --- Native tag sync (CPE-828, epic CPE-717) -------------------------------------------------------
// Sync a path's tags with the OS-native store (macOS Finder tags / Windows NTFS ADS / Linux xattr). A
// filesystem that can't hold native metadata degrades to a silent no-op in the backend.

/** The OS-native tag store's display name (e.g. "NTFS alternate data streams", "Finder tags"). */
export function nativeTagStoreName(): Promise<string> {
  return commands.nativeTagsName();
}

/** Pull a path's native tags into the store (non-destructive union); the store is updated from the
 *  returned whole store, so any open editor can re-seed from it. */
export async function pullNativeTags(path: string): Promise<void> {
  const updated = unwrap(await commands.nativeTagsPull(path)) as TagStore;
  store.set(updated ?? {});
}

/** Push a path's current tags out to native file metadata (the internal store is authoritative). */
export async function pushNativeTags(path: string): Promise<void> {
  await commands.nativeTagsPush(path).then(unwrap);
}

/** Tag usage counts, most-used first (`[tag, count][]`), straight from the backend. */
export function tagCounts(): Promise<[string, number][]> {
  return commands.tagCounts().then(unwrap);
}

/** Re-key a path's tags after an in-app rename/move so they follow the file (CPE-652). No-op if untagged. */
export async function retagPath(from: string, to: string): Promise<void> {
  const updated = unwrap(await commands.retagPath(from, to)) as TagStore;
  store.set(updated ?? {});
}

/** Rename tag `old` → `next` across every path (CPE-653). An empty `next` deletes it. */
export async function renameTag(old: string, next: string): Promise<void> {
  const updated = unwrap(await commands.renameTag(old, next)) as TagStore;
  store.set(updated ?? {});
}

/** Remove a tag from every path (CPE-653). */
export async function deleteTag(tag: string): Promise<void> {
  const updated = unwrap(await commands.deleteTag(tag)) as TagStore;
  store.set(updated ?? {});
}

/** Merge an exported tag store (JSON) into the current one (CPE-654). */
export async function importTags(json: string): Promise<void> {
  const updated = unwrap(await commands.importTags(json)) as TagStore;
  store.set(updated ?? {});
}

/** The current tag store serialized as pretty JSON, for export (CPE-654). */
export function exportTags(): string {
  return JSON.stringify(get(store), null, 2);
}
