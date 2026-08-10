/**
 * Theme runtime (CPE-1535 foundation slice + CPE-1540 resolution, epic CPE-1492 "light/dark theme";
 * widened by CPE-1544 with an orthogonal contrast axis, epic CPE-1496 "high contrast"): resolves the
 * persisted `theme` (and, orthogonally, `contrast`) setting to a concrete theme and stamps it onto
 * `document.documentElement.dataset.theme` so CPE-1534's `:root[data-theme="light"]` (CPE-1539's
 * `:root[data-theme="dark"]`, and CPE-1543's `:root[data-theme="hc-light"/"hc-dark"]`) CSS layers have
 * something to select on.
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
import type { ThemeSetting, ContrastSetting } from "./types";

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
 * Resolve a persisted contrast preference to whether the high-contrast variant should apply.
 * `"high"` is an unconditional manual override (`true`); `"off"` is an unconditional override
 * (`false`); `"system"` follows the OS high-contrast signal — the `osHighContrastActive` argument.
 * CPE-1546 supplies that argument for real: `main.ts` reads the OS state once at boot via
 * `highContrastSignal.ts`'s `queryOsHighContrast()` (backed by the `is_high_contrast_active` Rust
 * command) and passes it through here, so a persisted `"system"` contrast preference resolves to the
 * actual OS accessibility state. The argument still defaults to `false` so every call site that doesn't
 * pass it (and any older/non-Tauri context where the read fails open) behaves exactly as before. Mirrors
 * how `resolveTheme`'s `"system"` branch is structured — one pure, synchronous resolution function per axis.
 */
export function resolveContrast(pref: ContrastSetting, osHighContrastActive = false): boolean {
  if (pref === "high") return true;
  if (pref === "off") return false;
  return osHighContrastActive;
}

/**
 * Apply a theme preference (and, orthogonally, a contrast preference) to the document by setting
 * `documentElement.dataset.theme` to the composed resolved value. The base theme comes from
 * `resolveTheme`; when `resolveContrast` says high-contrast is active, the stamped value becomes
 * `` `hc-${base}` `` (e.g. `"hc-dark"`) for CPE-1543's `:root[data-theme="hc-..."]` CSS layer to select
 * on — otherwise it's the base value unchanged, exactly as before this ticket.
 *
 * `contrastPref` and `osHighContrastActive` are optional and default to `"off"`/`false`, which
 * reproduces the pre-CPE-1544 one-argument behaviour exactly (`dataset.theme` is always the bare
 * `"light"`/`"dark"` base) — so every existing call site (`main.ts`'s `applyTheme(loadTheme())`,
 * `SettingsDialog.svelte`'s `applyTheme(v)`) keeps compiling and behaving identically without passing
 * the new arguments. Callers that DO want the contrast axis applied pass `loadContrast()` (and, once
 * CPE-1546 lands, the OS signal) as the extra arguments.
 */
export function applyTheme(
  themePref: ThemeSetting,
  contrastPref: ContrastSetting = "off",
  osHighContrastActive = false,
): void {
  const base = resolveTheme(themePref);
  const hc = resolveContrast(contrastPref, osHighContrastActive);
  document.documentElement.dataset.theme = hc ? `hc-${base}` : base;
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
