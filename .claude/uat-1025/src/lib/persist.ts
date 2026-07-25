// Tiny localStorage helpers that never throw — localStorage can be unavailable (SSR, sandboxed
// webview, private mode). Consolidates the per-component `try { localStorage… } catch {}` pattern used
// for UI-state persistence (board/home/workbench/search/preview view prefs).

/** Read a raw string, or `null` if absent/unavailable. */
export function lsGet(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

/** Write a raw string; a no-op if storage is unavailable. */
export function lsSet(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    /* ignore */
  }
}

/** Read a boolean stored as `"1"`/`"0"`; missing/unavailable → `fallback`. */
export function lsBool(key: string, fallback: boolean): boolean {
  const v = lsGet(key);
  return v === null ? fallback : v === "1";
}
