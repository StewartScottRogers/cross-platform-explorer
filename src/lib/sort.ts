import type { DirEntry, SortKey, SortDir } from "./types";
import { typeName } from "./filetypes";

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
 */
export function compareEntries(
  a: DirEntry,
  b: DirEntry,
  key: SortKey,
  dir: SortDir,
  foldersFirst = true,
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
      cmp = collator.compare(typeName(a), typeName(b)) || compareNames(a.name, b.name);
      break;
    case "size":
      cmp = a.size - b.size || compareNames(a.name, b.name);
      break;
  }

  return dir === "asc" ? cmp : -cmp;
}

/** Return a new array of entries sorted for display. Does not mutate the input.
    `foldersFirst` (default true) floats directories above files (CPE-359). */
export function sortEntries(
  entries: DirEntry[],
  key: SortKey,
  dir: SortDir,
  foldersFirst = true,
): DirEntry[] {
  return [...entries].sort((a, b) => compareEntries(a, b, key, dir, foldersFirst));
}
