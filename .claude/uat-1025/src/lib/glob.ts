/**
 * Tiny glob matcher for "Select by pattern" (CPE-360). Supports `*` (any run, including
 * empty) and `?` (exactly one character), case-insensitive; every other character is a
 * literal. Multiple **comma-separated** patterns match if ANY include matches (CPE-571);
 * a pattern prefixed with `!` is an **exclusion** (CPE-578), so `*.js, !*.min.js` selects
 * every `.js` except minified ones. With only exclusions, everything-except matches. Pure
 * and side-effect-free so it's unit-testable.
 */

/** Compile a glob to an anchored, case-insensitive RegExp. */
export function globToRegExp(pattern: string): RegExp {
  const escaped = pattern
    .replace(/[.+^${}()|[\]\\]/g, "\\$&") // escape regex specials (not * or ?)
    .replace(/\*/g, ".*")
    .replace(/\?/g, ".");
  return new RegExp(`^${escaped}$`, "i");
}

/** Whether `name` matches the glob `pattern` — ANY of several comma-separated include patterns, minus
 *  any `!`-prefixed exclusion. An empty/blank pattern (or all-blank list) matches nothing; a list of
 *  only exclusions matches everything except them. */
export function matchesGlob(name: string, pattern: string): boolean {
  const parts = pattern.split(",").map((p) => p.trim()).filter(Boolean);
  if (parts.length === 0) return false;
  const excludes = parts.filter((p) => p.startsWith("!")).map((p) => p.slice(1)).filter(Boolean);
  const includes = parts.filter((p) => !p.startsWith("!"));
  const included = includes.length === 0 || includes.some((p) => globToRegExp(p).test(name));
  const excluded = excludes.some((p) => globToRegExp(p).test(name));
  return included && !excluded;
}
