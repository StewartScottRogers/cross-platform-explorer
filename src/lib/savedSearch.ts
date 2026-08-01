// Pure saved-query model + evaluator (CPE-986, epic CPE-978 "Smart folders & saved searches"). A saved
// search is a serialisable, named bundle of `Condition`s (reused from CPE-774) combined with all/any, so a
// smart folder is a thin listing filtered through `evaluateSavedSearch`. No DOM/IO — unit-tested here — so
// the store (persistence) and the editor UI are thin wrappers over this. Mirrors the idioms of the
// neighbouring rule engines (colorRules / watchRules / selectMatch): one Condition matcher, tolerant parse.

import type { DirEntry } from "./types";
import type { TreeNode } from "./bindings.gen";
import { matchesCondition, isValidCondition, type Condition } from "./colorRules";

/**
 * A serialisable named query. `conditions` are combined with `match`:
 * - `"all"` — an entry must satisfy every condition (AND); an empty condition list matches everything.
 * - `"any"` — an entry must satisfy at least one condition (OR); an empty condition list matches nothing.
 *
 * `root` (CPE-1229) is the folder the search was captured from — there is no whole-computer index
 * (CPE-703's index engine is a separate, not-yet-built epic), so a structured saved search evaluates
 * recursively from this one captured folder rather than "everywhere" the way the tag-only smart folders
 * do. Optional/omittable so older-shaped persisted data (and the CPE-986/1228 fixtures that predate it)
 * still parses; a missing root falls back to the current folder at open time (see App.svelte).
 */
export interface SavedSearch {
  id: string;
  name: string;
  conditions: Condition[];
  match: "all" | "any";
  root?: string;
}

/** Resolve the folder a structured search should evaluate from: its captured `root` (CPE-1229) if set,
    else `fallback` (the folder open at the time it's opened) — the same fallback a search saved before
    `root` existed (or whose captured folder no longer resolves) needs. Pure — the one source of truth
    so the open-evaluator (`App.svelte`'s `loadStructuredSearchEntries`) and the live-refresh watch scope
    (CPE-1230's `smartFolderLiveRefresh.ts`) can't drift apart on which folder is "the" root. */
export function resolveSavedSearchRoot(search: SavedSearch, fallback: string): string {
  return search.root && search.root.trim() ? search.root : fallback;
}

/** Whether `entry` satisfies the saved search, combining its conditions with all/any. Pure. */
export function matchesSavedSearch(entry: DirEntry, search: SavedSearch, now: number): boolean {
  const { conditions, match } = search;
  if (match === "all") {
    // Vacuous truth: an "all" search with no conditions matches everything (an unfiltered smart folder).
    return conditions.every((c) => matchesCondition(entry, c, now));
  }
  // "any" with no conditions matches nothing — there is no condition to satisfy.
  return conditions.some((c) => matchesCondition(entry, c, now));
}

/**
 * The entries that satisfy the saved search, combining its conditions via all/any through the existing
 * `matchesCondition`. Returns the matching `DirEntry`s (order preserved), not indices — smart folders
 * consume a filtered listing. Pure.
 */
export function evaluateSavedSearch(entries: DirEntry[], search: SavedSearch, now: number): DirEntry[] {
  return entries.filter((e) => matchesSavedSearch(e, search, now));
}

/** Serialise a saved search to JSON for persistence. */
export function serializeSavedSearch(search: SavedSearch): string {
  return JSON.stringify(search);
}

/**
 * Structural guard for a persisted saved search — validates each field (not just presence), reusing
 * `isValidCondition` so a corrupted/hand-edited condition is caught here rather than throwing later in
 * `matchesCondition`. Requires a non-blank `name`. Pure.
 */
function isValidSavedSearch(x: unknown): x is SavedSearch {
  if (!x || typeof x !== "object") return false;
  const o = x as Record<string, unknown>;
  return (
    typeof o.id === "string" &&
    typeof o.name === "string" &&
    o.name.trim() !== "" &&
    (o.match === "all" || o.match === "any") &&
    Array.isArray(o.conditions) &&
    o.conditions.every(isValidCondition) &&
    (o.root === undefined || typeof o.root === "string")
  );
}

/**
 * Parse a persisted saved search. Tolerant: bad JSON / wrong shape / missing or blank name / an invalid
 * condition → `null`. Never throws. Pure.
 */
export function parseSavedSearch(json: string): SavedSearch | null {
  try {
    const raw = JSON.parse(json);
    return isValidSavedSearch(raw) ? raw : null;
  } catch {
    return null;
  }
}

/** Join a scanned-tree root with a child name, matching whichever separator `root` already uses (`\`
    for a Windows-style root, `/` otherwise) so a flattened path is a real, navigable OS path rather than
    a mix of separators. Not exported — an internal detail of {@link flattenTree}. */
function joinScanPath(root: string, name: string): string {
  const sep = root.includes("\\") ? "\\" : "/";
  return root.endsWith(sep) ? root + name : root + sep + name;
}

/** Lowercased extension without the dot, mirroring the backend `DirEntry.extension` convention — empty
    for a folder or a name with no extension. Not exported — an internal detail of {@link flattenTree}. */
function extensionOf(name: string, isDir: boolean): string {
  if (isDir) return "";
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
}

/**
 * Flatten a `scanTree` result (CPE-779, `commands.scanTree`) into a `DirEntry[]` so a structured saved
 * search can run through the same `evaluateSavedSearch`/`matchesCondition` as any other listing (CPE-1229
 * open-evaluator — no parallel matcher). There's no whole-computer index yet, so this is how a structured
 * search "cuts across the tree": recursively, from the one folder it was captured under. `modified`/`size`
 * come straight off the scan; `hidden`/`is_symlink` default false (`scan_tree` never follows symlinks and
 * doesn't report the hidden attribute — neither is read by `matchesCondition`, so this is a safe, inert
 * default rather than a guess). Pure — order mirrors the tree's own (dir-then-file-as-scanned) order.
 */
export function flattenTree(nodes: TreeNode[], root: string): DirEntry[] {
  const out: DirEntry[] = [];
  const walk = (list: TreeNode[], parent: string) => {
    for (const n of list) {
      const path = joinScanPath(parent, n.name);
      out.push({
        name: n.name,
        path,
        is_dir: n.isDir,
        size: n.size ?? 0,
        modified: n.modified ?? null,
        extension: extensionOf(n.name, n.isDir),
        hidden: false,
        is_symlink: false,
      });
      if (n.isDir && n.children) walk(n.children, path);
    }
  };
  walk(nodes, root);
  return out;
}
