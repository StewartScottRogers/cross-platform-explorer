/**
 * Tiny glob matcher for "Select by pattern" (CPE-360). Supports `*` (any run, including
 * empty) and `?` (exactly one character), case-insensitive; every other character is a
 * literal. Multiple **comma-separated** patterns match if ANY one matches, so e.g.
 * `*.jpg, *.png` selects both (CPE-571). Pure and side-effect-free so it's unit-testable.
 */

/** Compile a glob to an anchored, case-insensitive RegExp. */
export function globToRegExp(pattern: string): RegExp {
  const escaped = pattern
    .replace(/[.+^${}()|[\]\\]/g, "\\$&") // escape regex specials (not * or ?)
    .replace(/\*/g, ".*")
    .replace(/\?/g, ".");
  return new RegExp(`^${escaped}$`, "i");
}

/** Whether `name` matches the glob `pattern` — or ANY of several comma-separated patterns. An
 *  empty/blank pattern (or all-blank list) matches nothing. */
export function matchesGlob(name: string, pattern: string): boolean {
  const parts = pattern.split(",").map((p) => p.trim()).filter(Boolean);
  if (parts.length === 0) return false;
  return parts.some((p) => globToRegExp(p).test(name));
}
