// One-shot OS high-contrast accessibility signal (CPE-1546, epic CPE-1496).
//
// `queryOsHighContrast()` asks the Rust `is_high_contrast_active` command whether the OS's high-contrast /
// increased-contrast accessibility mode is ON, so `theme.ts`'s `resolveContrast(pref, osHighContrastActive)`
// can resolve a persisted `ContrastSetting === "system"` (CPE-1544/1545) to the real OS state instead of the
// hard-coded `false` default. Read ONCE at boot (main.ts) — there is no live subscription (deferred).
//
// Fail-open: any rejection (an older host without the command, a non-Tauri context, a backend error)
// resolves to `false`, so the app's default behaviour and the manual "High" override are preserved and this
// never throws. Mirrors `theme.ts`'s shape: a tiny, focused, easily-mockable helper.
import { invoke } from "./invoke";

/**
 * Query the OS high-contrast signal once. Resolves `true` when the OS reports high-contrast/increased-
 * contrast is active, else `false`. Never rejects: any error fails open to `false`.
 */
export async function queryOsHighContrast(): Promise<boolean> {
  try {
    return await invoke<boolean>("is_high_contrast_active");
  } catch {
    return false;
  }
}
