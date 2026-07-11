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
export function matchesQuery(name: string, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (q === "") return true;

  const n = name.toLowerCase();

  if (q.includes("*") || q.includes("?")) {
    return globToRegExp(q).test(n);
  }
  return n.includes(q);
}

/** Convert a glob (already lowercased) to an anchored RegExp. */
function globToRegExp(glob: string): RegExp {
  // Escape everything that is special in a RegExp EXCEPT * and ?, which we
  // translate afterwards.
  const escaped = glob.replace(/[.+^${}()|[\]\\]/g, "\\$&");
  const pattern = "^" + escaped.replace(/\*/g, ".*").replace(/\?/g, ".") + "$";
  return new RegExp(pattern);
}
