// Grid-aware arrow-key navigation (CPE-769, epic CPE-688). In the icons/gallery grid views the selection
// lead should move in 2-D — ArrowDown/Up by a whole row (± the column count), ArrowLeft/Right by one tile —
// instead of the 1-D ±1 the list/details views use. This is the pure index-delta math; App feeds the delta
// to `moveLead`, which clamps to [0, count-1]. Kept dependency-free so it's unit-tested here.

/** Arrow keys this helper understands. Any other key yields a 0 delta (no move). */
export type ArrowKey = "ArrowUp" | "ArrowDown" | "ArrowLeft" | "ArrowRight";

/**
 * The index delta for an arrow key given the live column count.
 *
 * - `cols <= 1` (list / details, single column): Down = +1, Up = -1, Left/Right = 0 (no horizontal move).
 * - `cols > 1`  (icon / gallery grid): Down = +cols, Up = -cols, Right = +1, Left = -1.
 *
 * Left/Right wrapping across row boundaries (e.g. Right on a row's last tile → next row's first) and
 * clamping at the ends are left to `moveLead`, which bounds the resulting index. A non-arrow key → 0.
 */
export function arrowDelta(key: string, cols: number): number {
  const c = Math.max(1, Math.floor(cols) || 1);
  switch (key) {
    case "ArrowDown":
      return c;
    case "ArrowUp":
      return -c;
    case "ArrowRight":
      return c > 1 ? 1 : 0;
    case "ArrowLeft":
      return c > 1 ? -1 : 0;
    default:
      return 0;
  }
}
