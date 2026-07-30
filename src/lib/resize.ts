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

/**
 * CPE-1140 (review follow-up): fit BOTH persisted side-pane widths together on load, order-independently.
 *
 * The per-pane {@link maxSidePaneWidth} clamp is correct for a single drag (the other pane's width is
 * already settled), but applying it sequentially at load — clamp the sidebar using the *raw* persisted
 * right width, then clamp the right using the already-shrunk sidebar — makes whichever pane is clamped
 * first absorb the entire squeeze, so a perfectly valid persisted sidebar could be forced to its min just
 * because the right pane's persisted width is large (reachable now that the fixed maxes are gone).
 *
 * This shares the available room (`budget = windowWidth − dividers − midMin`) between the two panes and,
 * when the two persisted widths overflow it, trims each **proportionally to its slack above its own min**,
 * so neither is gratuitously collapsed. Each pane stays ≥ its own min; if the window is so narrow that
 * both mins + `midMin` don't fit, both land at their min and the middle pane's `minmax(midMin, 1fr)` grid
 * track is the one that overflows/scrolls (the ticket's narrow-window behaviour). Returns `[sidebar, right]`.
 */
export function fitSidePanes(
  sidebar: number,
  right: number,
  sidebarMin: number,
  rightMin: number,
  budget: number,
): [number, number] {
  const s = Math.max(sidebarMin, sidebar);
  const r = Math.max(rightMin, right);
  const overflow = s + r - budget;
  if (overflow <= 0) return [s, r];
  const sSlack = s - sidebarMin;
  const rSlack = r - rightMin;
  const totalSlack = sSlack + rSlack;
  if (totalSlack <= 0) return [s, r]; // both already at min — window too narrow; middle scrolls
  return [
    Math.max(sidebarMin, Math.round(s - overflow * (sSlack / totalSlack))),
    Math.max(rightMin, Math.round(r - overflow * (rSlack / totalSlack))),
  ];
}
