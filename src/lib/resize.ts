/** Clamp a panel width to a safe [min, max] range. */
export function clampWidth(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

/**
 * The width divider (6px) used between panes in the main three-pane grid. Shared by the inline
 * `grid-template-columns` strings and the dynamic side-pane clamp below so the two never drift
 * apart (CPE-1140).
 */
export const PANE_DIVIDER_W = 6;

/**
 * CPE-1140: the side panes (left sidebar, right preview) have no fixed maximum anymore — they can
 * grow arbitrarily wide, but widening one must never squeeze the middle (file-list) pane below its
 * own minimum. This computes the largest width a side pane may take right now: whatever is left of
 * the window after the OTHER side pane, the grid dividers, and the middle pane's minimum are all
 * accounted for.
 *
 * Never returns less than `min` — if the window is too narrow to give the middle its full minimum
 * even with both side panes at their floor, the side pane simply holds its own minimum and the
 * middle pane (via its own `minmax(midMin, 1fr)` grid track) is the one that overflows/scrolls, per
 * the ticket's "narrow-window" behaviour (the middle's column visibility wins, not the side pane).
 */
export function maxSidePaneWidth(
  windowWidth: number,
  otherPanesWidth: number,
  dividerWidth: number,
  dividerCount: number,
  midMin: number,
  min: number,
): number {
  const available = windowWidth - otherPanesWidth - dividerWidth * dividerCount - midMin;
  return Math.max(min, available);
}
