/**
 * Pure name-transform logic for batch rename (CPE-255). The UI collects a
 * find/replace and previews the result; the actual filesystem rename is applied
 * by the backend `move_exact` command. Keeping the transform pure makes it fully
 * unit-testable and keeps the dialog dumb.
 */

export interface RenameItem {
  /** Original file name. */
  from: string;
  /** Proposed new name after the transform. */
  to: string;
  /** True when `to` differs from `from`. */
  changed: boolean;
  /**
   * True when this item's `to` collides with another item's `to` in the same
   * batch (two files would end up with the same name). Applying is still safe —
   * the backend refuses to clobber — but the preview flags it up front.
   */
  conflict: boolean;
}

/** Escape a literal string for safe use inside a RegExp. */
function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/**
 * Replace every occurrence of `find` with `replace` across each name. An empty
 * `find` is a no-op (returns every item unchanged). Matching is
 * case-insensitive unless `caseSensitive` is set.
 */
export function planFindReplace(
  names: string[],
  find: string,
  replace: string,
  caseSensitive = false,
): RenameItem[] {
  const items: RenameItem[] = names.map((from) => {
    let to = from;
    if (find) {
      const re = new RegExp(escapeRegExp(find), caseSensitive ? "g" : "gi");
      to = from.replace(re, replace);
    }
    return { from, to, changed: to !== from, conflict: false };
  });

  // Flag intra-batch collisions: any target name produced more than once.
  const counts = new Map<string, number>();
  for (const it of items) {
    const key = it.to.toLowerCase();
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }
  for (const it of items) {
    if ((counts.get(it.to.toLowerCase()) ?? 0) > 1) it.conflict = true;
  }

  return items;
}
