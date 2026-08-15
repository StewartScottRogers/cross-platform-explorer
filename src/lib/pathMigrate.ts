/**
 * Shared path-migration helper for the frontend's path-keyed `localStorage`/settings.json stores —
 * favourites, spotlight frecency (`cpe.spotlightFrecency`), and recents/recent-folders (CPE-1224). Each
 * store is a flat `{ path: string, ... }[]`, and none of them followed an in-app rename/move: their
 * entries orphaned at the old path (a favourited folder lost its star after rename; frecency/recents kept
 * dead paths). This is the frontend analog of CPE-1222's backend `tag_store_rename_subtree` fix — the
 * boundary rule is mirrored exactly (exact match, or a real path-separator boundary — `/a/b` matches
 * `/a/b` and everything under it but never a mere prefix sibling like `/a/bc`), checked against both
 * `/` and `\` since a store may hold paths from either OS convention. Pure + DOM-free so it's unit-tested
 * directly; callers (App.svelte's rename/move handlers) own persisting the migrated list back out.
 */

/**
 * `p`'s new location after `from` → `to`, or `null` if `p` is unaffected. Exact match re-keys like a
 * direct rename; anything under `from` (a real separator boundary, `/` or `\`) is re-keyed by swapping
 * the `from` prefix for `to`, leaving the remainder untouched — the subtree-aware superset a folder
 * rename/move needs. A no-op (`null`) when `from`/`to` are equal or `from` is empty.
 */
export function migratedPath(p: string, from: string, to: string): string | null {
  if (from === to || from === "") return null;
  if (p === from) return to;
  // Strip a trailing separator before building the "under this folder" prefixes below (CPE-1737 round
  // 2). A remote directory's own path can now legitimately carry a trailing '/' (S3's own spelling of
  // "this is a prefix"); without this, `from`/`to` already ending in '/' built a DOUBLED separator
  // ("…/sub//") that no real descendant path ever starts with, so a slashed folder's rename/move
  // migrated the folder itself but silently left every descendant behind, un-rekeyed.
  const fromBase = from.replace(/[\\/]+$/, "");
  const toBase = to.replace(/[\\/]+$/, "");
  if (fromBase === "") return null;
  const underSlash = `${fromBase}/`;
  const underBack = `${fromBase}\\`;
  if (p.startsWith(underSlash)) return `${toBase}/${p.slice(underSlash.length)}`;
  if (p.startsWith(underBack)) return `${toBase}\\${p.slice(underBack.length)}`;
  return null;
}

/**
 * Re-key every entry in `list` whose `path` is `from` or lives under it (subtree), swapping in `to`.
 * Pure — returns a new array only when something actually changed, so an unaffected list (the common
 * case: renaming a file/folder that's in none of these stores) is returned unchanged by reference.
 */
export function migratePathList<T extends { path: string }>(
  list: T[],
  from: string,
  to: string,
): T[] {
  let changed = false;
  const next = list.map((item) => {
    const np = migratedPath(item.path, from, to);
    if (np === null) return item;
    changed = true;
    return { ...item, path: np };
  });
  return changed ? next : list;
}
