/**
 * Find the index of the item to jump to for type-ahead find.
 *
 * Searches `names` cyclically for the first one that starts with `prefix`
 * (case-insensitive):
 *   - `advance` true  → start the search AFTER `from` (used for single-letter
 *     buffers, so repeating a letter cycles through matches).
 *   - `advance` false → start the search AT `from` (used while accumulating a
 *     longer prefix, so the current item stays selected if it still matches).
 *
 * Returns -1 when there is no match (or the prefix/list is empty).
 */
export function firstMatchIndex(
  names: string[],
  prefix: string,
  from: number,
  advance: boolean,
): number {
  const p = prefix.toLowerCase();
  const n = names.length;
  if (p === "" || n === 0) return -1;

  const base = advance ? from + 1 : from;
  for (let k = 0; k < n; k++) {
    const i = (((base + k) % n) + n) % n;
    if (names[i].toLowerCase().startsWith(p)) return i;
  }
  return -1;
}
