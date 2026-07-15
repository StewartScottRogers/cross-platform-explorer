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

/** Split a name into `[base, extension]`. A leading dot (dotfile like `.gitignore`) is NOT an
 *  extension. `ext` includes the dot (`".txt"`) or is empty. Pure. */
export function splitExt(name: string): [string, string] {
  const dot = name.lastIndexOf(".");
  return dot > 0 ? [name.slice(0, dot), name.slice(dot)] : [name, ""];
}

/** Flag intra-batch collisions: any target name produced more than once (case-insensitively). */
function markConflicts(items: RenameItem[]): RenameItem[] {
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

/** The case transforms offered by {@link planCase}. */
export type CaseMode = "lower" | "upper" | "title";

/** Title-case: capitalise the first letter of each word (runs of letters/digits), lowercase the rest. */
function toTitleCase(s: string): string {
  return s.toLowerCase().replace(/[\p{L}\p{N}]+/gu, (w) => w.charAt(0).toUpperCase() + w.slice(1));
}

/**
 * Change the case of each name's **base** (the extension is left untouched, so `README.TXT` →
 * lower → `readme.TXT`), by `mode` (CPE-427). A base that already matches is left unchanged.
 */
export function planCase(names: string[], mode: CaseMode): RenameItem[] {
  const transform = (base: string): string =>
    mode === "lower" ? base.toLowerCase() : mode === "upper" ? base.toUpperCase() : toTitleCase(base);
  const items: RenameItem[] = names.map((from) => {
    const [base, ext] = splitExt(from);
    const to = `${transform(base)}${ext}`;
    return { from, to, changed: to !== from, conflict: false };
  });
  return markConflicts(items);
}

/**
 * Sequentially number the names (CPE-426). `pattern` is the new base name with a run of `#` marking
 * where the number goes, zero-padded to the run's width (`photo-###`, start 1 → `photo-001.jpg`); a
 * pattern with no `#` gets the number appended. The original extension is preserved. An empty
 * pattern is a no-op. Numbers count from `start` in the given order.
 */
export function planNumber(names: string[], pattern: string, start: number): RenameItem[] {
  const items: RenameItem[] = names.map((from, i) => {
    if (!pattern) {
      return { from, to: from, changed: false, conflict: false };
    }
    const n = start + i;
    const [, ext] = splitExt(from);
    const run = pattern.match(/#+/);
    const base = run
      ? pattern.replace(/#+/, String(n).padStart(run[0].length, "0"))
      : `${pattern}${n}`;
    const to = `${base}${ext}`;
    return { from, to, changed: to !== from, conflict: false };
  });
  return markConflicts(items);
}

/**
 * Add a `prefix` and/or `suffix` to each name, keeping the extension last (CPE-424): the suffix
 * lands before the extension (`report.pdf` + suffix `-v2` → `report-v2.pdf`). Both empty ⇒ no-op.
 */
export function planAffix(names: string[], prefix: string, suffix: string): RenameItem[] {
  const items: RenameItem[] = names.map((from) => {
    if (!prefix && !suffix) {
      return { from, to: from, changed: false, conflict: false };
    }
    const [base, ext] = splitExt(from);
    const to = `${prefix}${base}${suffix}${ext}`;
    return { from, to, changed: to !== from, conflict: false };
  });
  return markConflicts(items);
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

  return markConflicts(items);
}
