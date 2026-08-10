/**
 * Theme runtime (CPE-1535 foundation slice + CPE-1540 resolution, epic CPE-1492 "light/dark theme"):
 * resolves the persisted `theme` setting to a concrete theme and stamps it onto
 * `document.documentElement.dataset.theme` so CPE-1534's `:root[data-theme="light"]` (and CPE-1539's
 * `:root[data-theme="dark"]`) CSS layers have something to select on.
 *
 * "system" resolves live against the OS light/dark signal via
 * `window.matchMedia("(prefers-color-scheme: dark)")` — chosen over Tauri's
 * `getCurrentWindow().theme()`/`onThemeChanged` because the webview already reflects the OS signal
 * through `prefers-color-scheme` on Windows/macOS/Linux with no Tauri API and no new
 * `capabilities/default.json` permission, and it sidesteps the Linux `onThemeChanged` flakiness the
 * epic brief calls out (Tauri #9427). One code path, no Rust surface, fully mockable in vitest/jsdom
 * by stubbing `window.matchMedia`.
 *
 * Kept pure/synchronous and framework-free so it's trivially unit-testable.
 */
import type { ThemeSetting } from "./types";

/** The concrete theme a `ThemeSetting` resolves to. */
export type ResolvedTheme = "light" | "dark";

const DARK_QUERY = "(prefers-color-scheme: dark)";

/**
 * Resolve a persisted theme preference to the concrete theme to apply. "light"/"dark" are
 * unconditional overrides; "system" checks `window.matchMedia("(prefers-color-scheme: dark)")` and
 * resolves to "dark" if it matches, else "light". Guarded for non-browser/older test environments
 * where `window` or `matchMedia` is unavailable — falls back to "light".
 */
export function resolveTheme(pref: ThemeSetting): ResolvedTheme {
  if (pref === "light" || pref === "dark") return pref;
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") return "light";
  return window.matchMedia(DARK_QUERY).matches ? "dark" : "light";
}

/**
 * Apply a theme preference to the document by setting `documentElement.dataset.theme` to its
 * resolved value. Harmless before CPE-1534/CPE-1539's CSS lands (an unrecognised attribute has no
 * visual effect); once merged, the matching `:root[data-theme="..."]` selector takes effect.
 */
export function applyTheme(pref: ThemeSetting): void {
  document.documentElement.dataset.theme = resolveTheme(pref);
}

/**
 * Subscribe to live OS light/dark flips via `matchMedia`'s `change` event, invoking `onChange` each
 * time the OS preference changes. Returns an unsubscribe function. Guarded the same way as
 * `resolveTheme`: a no-op unsubscribe is returned when `matchMedia` is unavailable.
 *
 * Callers typically pass `() => applyTheme(loadTheme())` — safe to wire unconditionally, since
 * `resolveTheme` only reacts to the OS signal when the persisted pref is "system"; a "light"/"dark"
 * pref is unaffected by the callback firing.
 */
export function watchSystemTheme(onChange: () => void): () => void {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
    return () => {};
  }
  const mql = window.matchMedia(DARK_QUERY);
  const listener = () => onChange();
  mql.addEventListener("change", listener);
  return () => mql.removeEventListener("change", listener);
}
