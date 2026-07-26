/**
 * Pure helpers for the code-preview outline strip + breadcrumb + jump-to-symbol (CPE-1090, epic
 * CPE-724). Kept dependency-free and DOM-free so they're trivially unit-testable: the component that
 * measures a real `lineHeight` from `getComputedStyle` and reads/writes `scrollTop` lives in
 * `PreviewPane.svelte`; everything here is plain-number math + array scanning.
 */
import type { Symbol as CodeSymbol, FoldRange } from "../bindings.gen";

/** Fallback line height (px) when the measured value is missing/invalid — matches the preview's
 *  12px monospace font at a ~1.5 line-height, rounded to a sane constant. */
export const FALLBACK_LINE_HEIGHT = 18;

/**
 * Resolve a usable line-height in px from a CSS `line-height` computed value, which can be `"normal"`
 * (keyword, no px), an empty string (no layout yet, e.g. in tests), `"0px"`, or `NaN` after parsing.
 * Division-safe: never returns `0`, negative, or `NaN` — falls back to `fontSize * 1.4`, and if that is
 * also unusable, to {@link FALLBACK_LINE_HEIGHT}.
 */
export function resolveLineHeight(lineHeightCss: string, fontSizePx: number): number {
  const parsed = parseFloat(lineHeightCss);
  if (Number.isFinite(parsed) && parsed > 0) return parsed;
  const fromFont = fontSizePx * 1.4;
  if (Number.isFinite(fromFont) && fromFont > 0) return fromFont;
  return FALLBACK_LINE_HEIGHT;
}

/**
 * The scrollTop (px) that puts 1-based `line` at the top of the scroll container, given a uniform
 * `lineHeight`. Division/NaN-safe: a non-finite or non-positive `lineHeight` is treated as
 * {@link FALLBACK_LINE_HEIGHT} instead of ever producing `NaN`/`Infinity`. `line < 1` is clamped to `1`
 * (top of file). The result is NOT clamped to a container's `scrollHeight` here — callers with access to
 * the real DOM element clamp against its live `scrollHeight`.
 */
export function lineToScrollTop(line: number, lineHeight: number): number {
  const safeLineHeight = Number.isFinite(lineHeight) && lineHeight > 0 ? lineHeight : FALLBACK_LINE_HEIGHT;
  const safeLine = Number.isFinite(line) && line > 1 ? line : 1;
  return (safeLine - 1) * safeLineHeight;
}

/**
 * The 1-based top-visible line for a given `scrollTop`, using the same uniform-line-height math as
 * {@link lineToScrollTop} (its inverse). Division-safe for the same reasons.
 */
export function scrollTopToLine(scrollTop: number, lineHeight: number): number {
  const safeLineHeight = Number.isFinite(lineHeight) && lineHeight > 0 ? lineHeight : FALLBACK_LINE_HEIGHT;
  const safeScrollTop = Number.isFinite(scrollTop) && scrollTop > 0 ? scrollTop : 0;
  return Math.round(safeScrollTop / safeLineHeight) + 1;
}

/**
 * The enclosing symbol for the top-visible line: the last symbol (in source order) whose `line <=
 * topLine`, or `null` when `outline` is empty or `topLine` is above every symbol's line (e.g. scrolled to
 * the very top of a file whose first symbol starts a few lines in). `outline` is assumed sorted by line
 * (as `code_intel`/`code_outline::outline` produce it) but this scans defensively rather than assuming
 * it, so an out-of-order list still yields a sane answer instead of undefined behaviour.
 */
export function enclosingSymbol(outline: CodeSymbol[], topLine: number): CodeSymbol | null {
  let best: CodeSymbol | null = null;
  for (const sym of outline) {
    if (sym.line <= topLine && (best === null || sym.line > best.line)) best = sym;
  }
  return best;
}

// ---- fold-aware jump/breadcrumb corrections (CPE-1095) ----
// A collapsed fold removes its interior lines' `.cl-row` elements from the DOM entirely (see
// PreviewPane's `{#if !hiddenLines.has(...)}`), so the uniform-line-height math in
// `lineToScrollTop`/`scrollTopToLine` drifts by the collapsed range's length once any fold above the
// point of interest is collapsed. The helpers below correct for that: either by counting past the hidden
// lines (used as a fallback), or — preferred, since it's robust to wrap too — by resolving the actual
// rendered row's position and only falling back to the count-based math when that row isn't rendered.

/**
 * How many lines strictly above `line` are currently hidden by a collapsed fold. Used to convert a real
 * source line into its position among *rendered* rows (`line - countHiddenAbove(line, hiddenLines)`).
 * Division-safe by construction (pure counting, no math that can produce NaN/Infinity).
 */
export function countHiddenAbove(line: number, hiddenLines: ReadonlySet<number>): number {
  let count = 0;
  for (const hidden of hiddenLines) {
    if (hidden < line) count++;
  }
  return count;
}

/**
 * The 1-based start line of the collapsed fold that currently hides `line`, or `null` when `line` is not
 * hidden (no fold above it is collapsed, or it isn't inside any fold's range). Used by `jumpToSymbol` to
 * expand the right fold before scrolling to a target line that's currently folded away.
 */
export function foldToExpand(
  line: number,
  foldCollapsed: ReadonlySet<number>,
  foldByStart: ReadonlyMap<number, FoldRange>,
): number | null {
  for (const start of foldCollapsed) {
    const f = foldByStart.get(start);
    if (f && line > f.start_line && line <= f.end_line) return start;
  }
  return null;
}

/**
 * Map a 1-based position among *rendered* (non-hidden) rows back to its real 1-based source line number —
 * the inverse of `countHiddenAbove`. Used by `updateBreadcrumb` to turn the scroll-position estimate (which
 * only knows "the Nth rendered row") into the actual source line, so the breadcrumb stays correct while a
 * fold above the viewport is collapsed. Index/division-safe: a non-positive `totalLines` returns `1`, and a
 * `visibleRow` past the last rendered row clamps to `totalLines` instead of running off the end.
 */
export function lineAtVisibleRow(visibleRow: number, totalLines: number, hiddenLines: ReadonlySet<number>): number {
  if (!Number.isFinite(totalLines) || totalLines <= 0) return 1;
  const safeRow = Number.isFinite(visibleRow) && visibleRow > 1 ? visibleRow : 1;
  let count = 0;
  for (let line = 1; line <= totalLines; line++) {
    if (hiddenLines.has(line)) continue;
    count++;
    if (count >= safeRow) return line;
  }
  return totalLines;
}

/** Minimal shape `rowScrollTarget` needs from a DOM rect — lets tests pass plain objects. */
export interface RectLike {
  top: number;
}

/**
 * The scrollTop that puts a target row at the top of `container`, given the row's own
 * `getBoundingClientRect()` (or `null` when that row isn't currently rendered — e.g. it's still inside a
 * collapsed fold the caller didn't expand). When `row` is available this is exact regardless of folds
 * above it or word-wrap, since the row's real rendered position already reflects any hidden rows removed
 * from the flow. When it's not, `fallback` (typically the corrected line-height math) is returned as-is.
 */
export function rowScrollTarget(
  row: RectLike | null,
  containerRect: RectLike,
  containerScrollTop: number,
  fallback: number,
): number {
  if (!row) return fallback;
  return row.top - containerRect.top + containerScrollTop;
}
