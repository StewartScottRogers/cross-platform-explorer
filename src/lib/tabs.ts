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

/** Tab ids that survive "Close others" — only the target (CPE-357). */
export function keepOnly(ids: number[], id: number): number[] {
  return ids.filter((x) => x === id);
}

/** Tab ids that survive "Close tabs to the right" — the target and everything left of it.
    An unknown id keeps all (a no-op). */
export function keepThroughRight(ids: number[], id: number): number[] {
  const i = ids.indexOf(id);
  return i < 0 ? ids.slice() : ids.slice(0, i + 1);
}
