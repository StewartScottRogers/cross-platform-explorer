/**
 * Theme runtime (CPE-1535, foundation slice of epic CPE-1492 "light/dark theme"): resolves the
 * persisted `theme` setting to a concrete theme and stamps it onto
 * `document.documentElement.dataset.theme` so CPE-1534's `:root[data-theme="light"]` CSS layer has
 * something to select on.
 *
 * Today only "light" exists — "system" resolves to "light" unconditionally, since there is no real
 * dark palette yet (that's CPE-1493). This module exists so CPE-1493 has exactly ONE place to extend
 * (resolveTheme) when real dark values land: swap the unconditional "light" for a
 * `matchMedia("(prefers-color-scheme: dark)")` check, and everything downstream (applyTheme, the CSS
 * selector, the Settings UI in CPE-1536) keeps working unchanged.
 *
 * Kept pure/synchronous and framework-free so it's trivially unit-testable.
 */
import type { ThemeSetting } from "./types";

/** The concrete theme a `ThemeSetting` resolves to. Only "light" is real today. */
export type ResolvedTheme = "light";

/**
 * Resolve a persisted theme preference to the concrete theme to apply. Both "system" and "light"
 * resolve to "light" for now — there is no dark palette to resolve "system" against yet.
 */
export function resolveTheme(pref: ThemeSetting): ResolvedTheme {
  return "light";
}

/**
 * Apply a theme preference to the document by setting `documentElement.dataset.theme` to its
 * resolved value. Harmless before CPE-1534's CSS lands (an unrecognised attribute has no visual
 * effect); once merged, `:root[data-theme="light"]` selectors take effect.
 */
export function applyTheme(pref: ThemeSetting): void {
  document.documentElement.dataset.theme = resolveTheme(pref);
}
