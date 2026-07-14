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
