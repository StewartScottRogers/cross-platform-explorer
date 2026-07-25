// Windowing math for the virtualized file list (CPE-690, epic CPE-688): given the scroll position and
// geometry, compute which item indices to render (a visible window + overscan) and the top/bottom spacer
// heights that preserve the full scroll height. Pure + dependency-free so it's unit-tested here; the
// FileList render integration (keyboard nav, selection, scroll-into-view, DnD with windowed rows) is the
// separate, GUI-verified part. Handles both the list views (columns = 1) and the icon/gallery grids
// (columns = N) via fixed row height per view.

/** The slice of items to render plus the spacer heights that keep the scrollbar honest. */
export interface WindowRange {
  /** First item index to render (inclusive), row-aligned. */
  start: number;
  /** One past the last item index to render (exclusive). */
  end: number;
  /** Pixels of empty space above the rendered slice (a spacer div's height). */
  padTop: number;
  /** Pixels of empty space below the rendered slice. */
  padBottom: number;
}

/**
 * Compute the render window for a fixed-row-height virtual list/grid.
 *
 * @param scrollTop      current scroll offset in px
 * @param viewportHeight visible height of the scroll container in px
 * @param rowHeight      height of one row in px (a row holds `columns` items)
 * @param itemCount      total number of items
 * @param columns        items per row (1 for list/details, N for icon/gallery grids)
 * @param overscanRows   extra rows to render above and below the viewport (smoother scroll)
 */
export function windowRange(
  scrollTop: number,
  viewportHeight: number,
  rowHeight: number,
  itemCount: number,
  columns = 1,
  overscanRows = 3,
): WindowRange {
  const cols = Math.max(1, Math.floor(columns));
  if (rowHeight <= 0 || itemCount <= 0 || viewportHeight <= 0) {
    return { start: 0, end: 0, padTop: 0, padBottom: 0 };
  }

  const rows = Math.ceil(itemCount / cols);
  const clampedScroll = Math.max(0, scrollTop);
  // Clamp to the last row so a scroll offset past the content end (defensive) still yields the bottom
  // window rather than a start index beyond the list.
  const firstRow = Math.min(Math.floor(clampedScroll / rowHeight), Math.max(0, rows - 1));
  const visibleRows = Math.ceil(viewportHeight / rowHeight);

  const startRow = Math.max(0, firstRow - overscanRows);
  const endRow = Math.min(rows, firstRow + visibleRows + overscanRows);

  const start = startRow * cols;
  const end = Math.min(itemCount, endRow * cols);
  const padTop = startRow * rowHeight;
  const padBottom = (rows - endRow) * rowHeight;

  return { start, end, padTop, padBottom };
}

/** The total scrollable height of the list — for the scroll container's inner size. Pure. */
export function totalHeight(rowHeight: number, itemCount: number, columns = 1): number {
  const cols = Math.max(1, Math.floor(columns));
  if (rowHeight <= 0 || itemCount <= 0) return 0;
  return Math.ceil(itemCount / cols) * rowHeight;
}

/**
 * The scrollTop needed to bring item `index` into view (window-aware scroll-into-view, CPE-690): with
 * virtualization an off-screen row isn't in the DOM, so `element.scrollIntoView` can't be used — the
 * container's scrollTop is computed instead. Returns the current `scrollTop` when the item is already
 * fully visible; otherwise scrolls minimally, aligning the item to the top (if it's above the window) or
 * the bottom (if below). Pure.
 */
export function ensureVisibleOffset(
  index: number,
  scrollTop: number,
  viewportHeight: number,
  rowHeight: number,
  itemCount: number,
  columns = 1,
): number {
  const cols = Math.max(1, Math.floor(columns));
  if (rowHeight <= 0 || itemCount <= 0 || viewportHeight <= 0) return Math.max(0, scrollTop);
  const clampedIndex = Math.max(0, Math.min(index, itemCount - 1));
  const row = Math.floor(clampedIndex / cols);
  const itemTop = row * rowHeight;
  const itemBottom = itemTop + rowHeight;
  if (itemTop < scrollTop) return itemTop; // above the window → align to top
  if (itemBottom > scrollTop + viewportHeight) return itemBottom - viewportHeight; // below → align to bottom
  return scrollTop; // already fully visible
}
