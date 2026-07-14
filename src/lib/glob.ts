/**
 * Tiny glob matcher for "Select by pattern" (CPE-360). Supports `*` (any run, including
 * empty) and `?` (exactly one character), case-insensitive; every other character is a
 * literal. Pure and side-effect-free so it's unit-testable.
 */

/** Compile a glob to an anchored, case-insensitive RegExp. */
export function globToRegExp(pattern: string): RegExp {
  const escaped = pattern
    .replace(/[.+^${}()|[\]\\]/g, "\\$&") // escape regex specials (not * or ?)
    .replace(/\*/g, ".*")
    .replace(/\?/g, ".");
  return new RegExp(`^${escaped}$`, "i");
}

/** Whether `name` matches the glob `pattern`. An empty/blank pattern matches nothing. */
export function matchesGlob(name: string, pattern: string): boolean {
  if (!pattern.trim()) return false;
  return globToRegExp(pattern).test(name);
}
