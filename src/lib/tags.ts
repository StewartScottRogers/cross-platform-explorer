// Frontend tag service (CPE-636, epic CPE-614): a reactive mirror of the backend tag store
// (load_tags / set_tags / tag_counts, CPE-635). The store maps an absolute path to its tags + a
// single colour label. The reducer-style helpers below are pure + DOM-free so they're unit-tested;
// the store tail just wires the `invoke` calls. Idle by default — an empty store costs nothing, so
// the plain explorer is unaffected until a path is actually tagged.

import { writable, get, type Readable } from "svelte/store";
import { commands } from "./bindings.gen"; // typed client (CPE-964)
import { unwrap } from "./invoke";
import { canonicalPath } from "./paths";

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

const EMPTY_ENTRY: TagEntry = { tags: [], label: "" };

/** The entry for a path, or an empty entry if the path is untagged (or the store is empty/absent).
 *  Pure (never mutates the store).
 *
 *  Falls back to a canonicalPath scan (CPE-1737 round 2) ONLY when the exact key misses: `set_tags` has
 *  no `require_local`, so a remote folder is genuinely taggable, and its listing row's `path`
 *  legitimately carries a trailing '/' that a lookup from a different route (the toolbar's "tag this
 *  folder", the sidebar) would not reproduce. The store's OWN keys are left exactly as the backend
 *  returned them — this only widens what a lookup will MATCH, it never rewrites what gets sent to
 *  `setTags`/stored as a key, so a LOCAL Windows path's separators are never at risk of the
 *  backslash-corruption `canonicalPath` would cause if used to build a literal value (see
 *  settings.ts's comment on the same class of bug). The store is small (opt-in — only tagged paths
 *  appear at all), so the O(n) fallback scan costs nothing in practice. */
export function entryFor(store: TagStore, path: string): TagEntry {
  if (!store) return EMPTY_ENTRY;
  const exact = store[path];
  if (exact) return exact;
  const key = canonicalPath(path);
  for (const k of Object.keys(store)) {
    if (canonicalPath(k) === key) return store[k];
  }
  return EMPTY_ENTRY;
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

/** The backend key `setEntryTags` should WRITE under for `path` (CPE-1737 round 3): reuses an existing
 *  entry's key when one already matches — exactly, or canonically — so editing tags on a remote folder
 *  reached from a listing row (trailing '/') updates the SAME entry a prior tagging via a different
 *  route (toolbar/sidebar, un-slashed) already created, instead of forking a second key beside it.
 *  `entryFor`'s canonical-scan fallback fixes LOOKUP, but a naive `setTags(path, ...)` would still write
 *  a NEW key every time the caller's spelling differs — leaving two entries for one real folder, so
 *  `allTags`/`tagCounts` double-count it and reaching the folder by the OTHER spelling shows the STALE
 *  entry unchanged. Falls back to `path` itself (unrewritten) when nothing already matches — the
 *  first-time-tagging case. Pure. */
export function keyToWrite(store: TagStore, path: string): string {
  if (!store) return path;
  if (path in store) return path;
  const key = withoutTrailingSeparator(path);
  for (const k of Object.keys(store)) {
    if (withoutTrailingSeparator(k) === key) return k;
  }
  return path;
}

/** Strip ONE trailing FORWARD SLASH only, keeping a bare root (`/`, `C:/`) intact. Deliberately NOT
 *  `canonicalPath`: this is the only place in the codebase where a normalised form selects the key a
 *  write LANDS on, so it must collapse the trailing separator and nothing else. `canonicalPath` also
 *  rewrites `\` to `/`, and on Linux/macOS a backslash is a legal filename character — so a file
 *  literally named `a\b` and the path `a/b` are two unrelated files that canonicalise to one key, and
 *  the write would land on whichever the store happened to hold (found by the round-3 review, which
 *  reproduced it). Elsewhere the same collision only mis-toggles a favourite, which is recoverable;
 *  here it would silently overwrite another entry's tags. The trailing slash is the ONLY spelling
 *  difference CPE-1737 introduces, so it is the only one worth collapsing.
 *
 *  CPE-1754: a trailing BACKSLASH used to be stripped here too, but on Linux/macOS a backslash is a
 *  legal filename character — so a path whose FINAL character is a backslash (e.g. a POSIX file
 *  literally named `x\`) is a distinct real path, not a separator, and stripping it collapsed that path
 *  onto its sibling `x`, sending a tag write to the wrong entry (overwriting it). Windows never needed
 *  the backslash case here: a real Windows path like `C:\Users\me\docs` has no trailing separator to
 *  strip, and the bare drive root `C:\` never reaches this branch (its last char `\` no longer matches,
 *  so it now returns unchanged — same outcome the old regex guard gave it for `C:/`). So narrowing to
 *  forward-slash-only loses no behaviour this function's one caller (`keyToWrite`) needs. */
function withoutTrailingSeparator(p: string): string {
  if (p.length < 2) return p;
  if (p[p.length - 1] !== "/") return p;
  const rest = p.slice(0, -1);
  // A bare drive root ("C:/") or a POSIX root ("/") keeps its separator — dropping it changes meaning.
  if (/^[A-Za-z]:$/.test(rest)) return p;
  return rest;
}

/** Replace one path's tags + label; the store is updated from the returned whole store. Sends
 *  `keyToWrite`'s answer, never a value rewritten through `canonicalPath` directly (see `entryFor`'s doc
 *  for why storing/sending the canonicalised form itself is unsafe for a local path). */
export async function setEntryTags(path: string, tags: string[], label: string): Promise<void> {
  const key = keyToWrite(get(store), path);
  const updated = unwrap(await commands.setTags(key, tags, label)) as TagStore;
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
