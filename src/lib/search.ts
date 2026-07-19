/**
 * Match a file/folder name against a search query.
 *
 * - A plain query (no wildcards) matches as a case-insensitive substring, the
 *   long-standing behaviour.
 * - A query containing `*` or `?` is treated as a glob and matched against the
 *   WHOLE name (anchored), like Windows Explorer: `*` matches any run of
 *   characters, `?` matches exactly one. Regex metacharacters are literal.
 *
 * An empty/whitespace query matches everything (callers usually gate on this).
 */

/** A compiled predicate that tests a single name against a fixed query. */
export type Matcher = (name: string) => boolean;

/**
 * Compile `query` **once** into a reusable predicate. Filtering a folder calls the matcher per entry, so
 * the query normalization and — crucially — the glob→RegExp compilation must happen here, not per entry:
 * otherwise a glob search recompiles the RegExp N times per keystroke (CPE-695, epic CPE-688).
 */
export function makeMatcher(query: string): Matcher {
  const q = query.trim().toLowerCase();
  if (q === "") return () => true;

  if (q.includes("*") || q.includes("?")) {
    const re = globToRegExp(q);
    return (name) => re.test(name.toLowerCase());
  }
  return (name) => name.toLowerCase().includes(q);
}

/** Single-shot match — convenience over {@link makeMatcher} for callers testing one name. */
export function matchesQuery(name: string, query: string): boolean {
  return makeMatcher(query)(name);
}

/** Convert a glob (already lowercased) to an anchored RegExp. */
function globToRegExp(glob: string): RegExp {
  // Escape everything that is special in a RegExp EXCEPT * and ?, which we
  // translate afterwards.
  const escaped = glob.replace(/[.+^${}()|[\]\\]/g, "\\$&");
  const pattern = "^" + escaped.replace(/\*/g, ".*").replace(/\?/g, ".") + "$";
  return new RegExp(pattern);
}
