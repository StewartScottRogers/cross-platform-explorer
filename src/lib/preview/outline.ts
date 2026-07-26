/**
 * Pure helpers for the code-preview outline strip + breadcrumb + jump-to-symbol (CPE-1090, epic
 * CPE-724). Kept dependency-free and DOM-free so they're trivially unit-testable: the component that
 * measures a real `lineHeight` from `getComputedStyle` and reads/writes `scrollTop` lives in
 * `PreviewPane.svelte`; everything here is plain-number math + array scanning.
 */
import type { Symbol as CodeSymbol } from "../bindings.gen";

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
