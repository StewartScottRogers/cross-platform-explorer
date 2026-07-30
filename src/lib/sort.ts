import type { DirEntry, SortKey, SortDir } from "./types";
import { typeName } from "./filetypes";
import type { CellValue } from "./bindings.gen";

/**
 * Numeric-aware, case-insensitive collator so that embedded numbers compare by
 * value ("file2" < "file10") the way Windows Explorer / macOS Finder order names.
 */
const collator = new Intl.Collator(undefined, {
  numeric: true,
  sensitivity: "base",
});

/** Natural-order comparison of two names ("file2" before "file10"). */
export function compareNames(a: string, b: string): number {
  return collator.compare(a, b);
}

/**
 * Compare two entries for display order. Directories always sort before files;
 * within each group the chosen key decides, with a natural-name tiebreaker for
 * the type/size keys. `dir` flips the result for descending order (the
 * directories-first rule is intentionally NOT flipped).
 *
 * `typeNameOf` resolves an entry's type-key string; it defaults to `typeName`, but `sortEntries`
 * passes a precomputed-per-entry resolver so a whole sort doesn't recompute `typeName` on every one of
 * its O(n log n) comparisons (CPE-694).
 */
export function compareEntries(
  a: DirEntry,
  b: DirEntry,
  key: SortKey,
  dir: SortDir,
  foldersFirst = true,
  typeNameOf: (e: DirEntry) => string = typeName,
  sizeOf: (e: DirEntry) => number = (e) => e.size,
): number {
  // Directories float to the top unless the user opted into mixed sorting (CPE-359).
  if (foldersFirst && a.is_dir !== b.is_dir) return a.is_dir ? -1 : 1;

  let cmp = 0;
  switch (key) {
    case "name":
      cmp = compareNames(a.name, b.name);
      break;
    case "modified":
      // Files that share a timestamp (copied/extracted together) fall back to natural name order,
      // matching the type/size keys — otherwise they'd appear in arbitrary backend order (CPE-612).
      cmp = ((a.modified ?? 0) - (b.modified ?? 0)) || compareNames(a.name, b.name);
      break;
    case "type":
      cmp = collator.compare(typeNameOf(a), typeNameOf(b)) || compareNames(a.name, b.name);
      break;
    case "size":
      // `sizeOf` lets the recursive-folder-size mode (CPE-750) sort folders by their computed subtree
      // size; by default it's each entry's own byte size. A pending folder (size not yet computed) resolves
      // to a sentinel via `sizeOf`, so those cluster and fall back to name order — stable while filling.
      cmp = sizeOf(a) - sizeOf(b) || compareNames(a.name, b.name);
      break;
  }

  return dir === "asc" ? cmp : -cmp;
}

/** Return a new array of entries sorted for display. Does not mutate the input.
    `foldersFirst` (default true) floats directories above files (CPE-359).

    Decorate-sort-undecorate for the type key (CPE-688 perf): `typeName` is computed once per entry
    (O(n)) into a cache, so the O(n log n) comparisons read the cached string instead of re-deriving it
    — and re-allocating a template string — on every comparison. Other keys don't touch the cache. */
export function sortEntries(
  entries: DirEntry[],
  key: SortKey,
  dir: SortDir,
  foldersFirst = true,
  sizeOf?: (e: DirEntry) => number,
): DirEntry[] {
  if (key === "type") {
    const typeCache = new Map<DirEntry, string>();
    for (const e of entries) typeCache.set(e, typeName(e));
    const typeNameOf = (e: DirEntry) => typeCache.get(e) ?? typeName(e);
    return [...entries].sort((a, b) => compareEntries(a, b, key, dir, foldersFirst, typeNameOf));
  }
  // For size sort, `sizeOf` (when given) lets folders compare by recursive subtree size (CPE-750).
  if (key === "size" && sizeOf) {
    return [...entries].sort((a, b) => compareEntries(a, b, key, dir, foldersFirst, typeName, sizeOf));
  }
  return [...entries].sort((a, b) => compareEntries(a, b, key, dir, foldersFirst));
}

// ── Metadata-column sort (CPE-1146, epic CPE-707) ────────────────────────────────────────────────
// Type-aware ordering over CPE-1145's typed `CellValue`, NOT the pre-formatted display string (a
// lexical sort on "1920×1080" vs "640×480" would put the bigger image first — wrong). `Empty` (no
// value for that row — unreadable, or the wrong file kind for the column) always sorts last,
// regardless of asc/desc, so rows the column doesn't apply to don't jump to the top on a descending
// sort; direction only reorders the rows that actually have a value.

/** Compare two typed cell values for sort. Mismatched variants (shouldn't happen — a column always
 *  extracts one `CellValue` shape) compare equal rather than throwing. */
export function compareCellValues(a: CellValue, b: CellValue): number {
  const aEmpty = a === "Empty";
  const bEmpty = b === "Empty";
  if (aEmpty && bEmpty) return 0;
  if (aEmpty) return 1;
  if (bEmpty) return -1;
  if ("Text" in a && "Text" in b) return a.Text.localeCompare(b.Text, undefined, { sensitivity: "base" });
  if ("Int" in a && "Int" in b) return a.Int - b.Int;
  if ("Float" in a && "Float" in b) return a.Float - b.Float;
  if ("Bytes" in a && "Bytes" in b) return a.Bytes - b.Bytes;
  if ("Dimensions" in a && "Dimensions" in b) {
    const areaA = a.Dimensions.w * a.Dimensions.h;
    const areaB = b.Dimensions.w * b.Dimensions.h;
    return areaA - areaB || a.Dimensions.w - b.Dimensions.w;
  }
  return 0;
}

/** Sort entries by an active metadata column's typed value, `cellOf` resolving each path to its
 *  (possibly not-yet-fetched → `"Empty"`) cell. Mirrors `sortEntries`'s folders-first grouping + a
 *  natural-name tiebreaker so equal/empty values fall back to a stable, familiar order. */
export function sortByMetaColumn(
  entries: DirEntry[],
  cellOf: (path: string) => CellValue,
  dir: SortDir,
  foldersFirst = true,
): DirEntry[] {
  return [...entries].sort((a, b) => {
    if (foldersFirst && a.is_dir !== b.is_dir) return a.is_dir ? -1 : 1;
    const av = cellOf(a.path);
    const bv = cellOf(b.path);
    const aEmpty = av === "Empty";
    const bEmpty = bv === "Empty";
    // Empty-last is a GROUPING decision, like folders-first above — it must not flip with `dir`, or a
    // descending sort would put every not-yet-fetched/unsupported row at the TOP (compareCellValues'
    // own Empty ordering would otherwise get inverted along with everything else below).
    if (aEmpty !== bEmpty) return aEmpty ? 1 : -1;
    const cmp = compareCellValues(av, bv) || compareNames(a.name, b.name);
    return dir === "asc" ? cmp : -cmp;
  });
}
