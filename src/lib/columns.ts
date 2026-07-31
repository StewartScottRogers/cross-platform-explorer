/**
 * Details-view column widths (CPE-350). Pure helpers for the resizable file-list columns:
 * the grid template the header and rows share, and a clamped single-column resize. Kept
 * side-effect-free so they're unit-tested; FileList wires the pointer drag to them.
 */

/** The four sizable columns, in order: Name, Date modified, Type, Size. */
export const COLUMN_DEFAULTS = [320, 150, 120, 90];
/** Per-column minimum widths (px) so a column can't be dragged to nothing. */
export const COLUMN_MINS = [120, 90, 80, 60];
/** A generous per-column maximum so a drag can't produce an absurd width. */
export const COLUMN_MAX = 1200;
/** The leftmost (Name) column's own minimum — the absolute floor the middle file-list pane must
 *  never drop below, even on its own (CPE-1140, requirement #3). */
export const NAME_COL_MIN = COLUMN_MINS[0];
/** Horizontal chrome the details-view grid adds around the columns: `.columns` in app.css pads
 *  `12px 10px` (22px) and `.rows` pads `6px 0` — the wider of the two, so the derived middle-pane
 *  minimum leaves every column fully visible instead of flush against the pane edge. */
const FILELIST_CHROME = 22;
/** The middle (file-list) pane's minimum width (CPE-1140): the sum of every visible details-view
 *  column's minimum width plus the pane's own chrome, floored by the Name column's own minimum so
 *  the middle is never narrower than its leftmost column. */
export const MID_MIN = Math.max(
  COLUMN_MINS.reduce((sum, w) => sum + w, 0) + FILELIST_CHROME,
  NAME_COL_MIN,
);

/**
 * The CSS grid template for the four columns plus a trailing `1fr` spacer that soaks up
 * surplus width (keeping columns left-packed). Header and rows use the SAME template so
 * they stay aligned.
 */
export function columnsTemplate(widths: number[]): string {
  return widths.map((w) => `${Math.round(w)}px`).join(" ") + " 1fr";
}

/**
 * Return a new widths array with column `i` set to `px`, clamped to its min/max. Invalid
 * indices are returned unchanged. Does not mutate the input.
 */
export function resizeColumnTo(
  widths: number[],
  i: number,
  px: number,
  mins: number[] = COLUMN_MINS,
  max: number = COLUMN_MAX,
): number[] {
  if (i < 0 || i >= widths.length) return widths;
  const min = mins[i] ?? 40;
  const clamped = Math.max(min, Math.min(max, Math.round(px)));
  const next = widths.slice();
  next[i] = clamped;
  return next;
}

/**
 * Left offsets (px) of each column boundary, for positioning resize handles. `padLeft` is
 * the grid's left padding. Offset `k` is the right edge of column `k`.
 */
export function boundaryOffsets(widths: number[], padLeft: number): number[] {
  const out: number[] = [];
  let acc = padLeft;
  for (const w of widths) {
    acc += w;
    out.push(acc);
  }
  return out;
}

// ── Dynamic metadata columns (CPE-1146, epic CPE-707) ────────────────────────────────────────────
// Generalises the fixed 4 (Name/Date/Type/Size) with N user-picked metadata columns (Dimensions,
// Duration, Pages, …) appended after them. The built-in constants above are UNCHANGED — a folder with
// no active metadata columns renders byte-for-byte as before ("none active" is the default, per the
// ticket's persistence design) — these are purely additive helpers for the active set.

/** One active metadata column: its stable id (matches `AvailableColumn.id`, CPE-1145) + its own
 *  resizable width. Order in the array IS display order. */
export interface ActiveMetaColumn {
  id: string;
  width: number;
}

/** Default width (px) a newly-added metadata column starts at. */
export const META_COL_DEFAULT_WIDTH = 110;
/** Minimum width (px) for any metadata column — narrower than the built-ins since values like
 *  "1920×1080" or "3:45" are short. */
export const META_COL_MIN = 70;

/** The full per-column minimum-widths array for the 4 built-ins plus `metaCount` active metadata
 *  columns, for `resizeColumnTo`'s `mins` parameter over the combined widths array. */
export function fullMins(metaCount: number): number[] {
  return COLUMN_MINS.concat(new Array(Math.max(0, metaCount)).fill(META_COL_MIN) as number[]);
}

/** Append `id` to the active set at `META_COL_DEFAULT_WIDTH`, unless it's already active (no-op —
 *  a column can't be added twice). Does not mutate the input. */
export function addMetaColumn(active: ActiveMetaColumn[], id: string): ActiveMetaColumn[] {
  if (active.some((c) => c.id === id)) return active;
  return [...active, { id, width: META_COL_DEFAULT_WIDTH }];
}

/** Drop `id` from the active set (no-op if absent). Does not mutate the input. */
export function removeMetaColumn(active: ActiveMetaColumn[], id: string): ActiveMetaColumn[] {
  return active.filter((c) => c.id !== id);
}

/** Move `id` one slot earlier (`dir: -1`) or later (`dir: 1`) in display order; a no-op at either
 *  end or if `id` isn't active. Does not mutate the input. */
export function moveMetaColumn(active: ActiveMetaColumn[], id: string, dir: -1 | 1): ActiveMetaColumn[] {
  const i = active.findIndex((c) => c.id === id);
  const j = i + dir;
  if (i < 0 || j < 0 || j >= active.length) return active;
  const next = active.slice();
  [next[i], next[j]] = [next[j], next[i]];
  return next;
}

/** Re-clamp every active column's width into `[META_COL_MIN, COLUMN_MAX]` — called when restoring a
 *  persisted set (CPE-1140-style load guard) so a stale/corrupt width can't paint a broken layout. */
export function clampMetaWidths(active: ActiveMetaColumn[]): ActiveMetaColumn[] {
  return active.map((c) => ({
    ...c,
    width: Math.max(META_COL_MIN, Math.min(COLUMN_MAX, Math.round(c.width))),
  }));
}

/**
 * The "applies to all files" sentinel (CPE-1166): an `AvailableColumn` whose `extensions` list is
 * **empty** applies to every file, not one media family — the magic-byte detectors (true type, type
 * mismatch, text encoding, line endings) are file-agnostic. The column picker uses this to decide
 * whether an extension gate is relevant: an applies-to-all column must never be greyed out.
 * Mirrors `MetaColumn::applies_to_all` in the Rust `column_extract` module.
 */
export function appliesToAllFiles(extensions: string[]): boolean {
  return extensions.length === 0;
}

/**
 * Whether a column with the given `extensions` is applicable to a file of the given lowercase `ext`.
 * An applies-to-all column (empty `extensions`, see {@link appliesToAllFiles}) matches every file; an
 * extension-scoped column matches only when its list contains `ext`. Callers pass `ext` already
 * lowercased and without a leading dot.
 */
export function columnAppliesTo(extensions: string[], ext: string): boolean {
  return appliesToAllFiles(extensions) || extensions.includes(ext);
}
