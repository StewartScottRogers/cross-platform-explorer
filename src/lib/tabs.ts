/**
 * Tab helpers (CPE-356). Pure logic for the recently-closed-tab stack that powers
 * Ctrl+Shift+T, kept side-effect-free so it's unit-testable; App.svelte owns the tabs.
 */

/**
 * Push a just-closed tab's folder path onto the recently-closed stack (most recent last),
 * bounded to `cap` so it can't grow without limit. Returns a new array.
 */
export function pushClosedTab(stack: string[], path: string, cap = 10): string[] {
  return [...stack, path].slice(-cap);
}
