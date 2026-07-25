/**
 * Match a file/folder name against a search query.
 *
 * - A plain query (no wildcards) matches as a case-insensitive substring, the
 *   long-standing behaviour.
 * - A query containing `*` or `?` is treated as a glob and matched against the
 *   WHOLE name (anchored), like Windows Explorer: `*` matches any run of
 *   characters, `?` matches exactly one. Regex metacharacters are literal.
 * - A query containing a brace group `{a,b,c}` (matched braces with a top-level
 *   comma) is a glob too, where the group matches any one of its comma-separated
 *   alternatives — `*.{jpg,png}` matches `photo.jpg` and `photo.png` (CPE-697).
 *   `*`/`?` work inside a group; nested groups expand recursively. An unmatched
 *   brace, or a `{…}` with no top-level comma, is treated literally.
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

  if (isGlob(q)) {
    const re = globToRegExp(q);
    return (name) => re.test(name.toLowerCase());
  }
  return (name) => name.toLowerCase().includes(q);
}

/** Single-shot match — convenience over {@link makeMatcher} for callers testing one name. */
export function matchesQuery(name: string, query: string): boolean {
  return makeMatcher(query)(name);
}

/**
 * Whether `q` should be matched as a glob rather than a plain substring: it contains `*`/`?`, or a real
 * brace group. A lone/unmatched `{` (no matching `}` or no top-level comma) is *not* a glob — it stays a
 * literal-substring query, preserving the pre-CPE-697 behaviour for names that merely contain a brace.
 */
function isGlob(q: string): boolean {
  if (q.includes("*") || q.includes("?")) return true;
  for (let i = 0; i < q.length; i++) {
    if (q[i] === "{" && findBraceGroup(q, i) !== null) return true;
  }
  return false;
}

/** Convert a glob (already lowercased) to an anchored RegExp, expanding brace groups. */
function globToRegExp(glob: string): RegExp {
  return new RegExp("^" + globBody(glob) + "$");
}

/**
 * Translate a glob fragment into an (unanchored) RegExp body. `*`→`.*`, `?`→`.`, a matched `{a,b}`
 * group with a top-level comma → `(?:a|b)` (each alternative translated recursively so `*`/`?` and
 * nested groups work), everything else — including a literal/unmatched brace and any comma outside a
 * group — escaped literally.
 */
function globBody(glob: string): string {
  let out = "";
  let i = 0;
  while (i < glob.length) {
    const ch = glob[i];
    if (ch === "*") {
      out += ".*";
      i += 1;
    } else if (ch === "?") {
      out += ".";
      i += 1;
    } else if (ch === "{") {
      const group = findBraceGroup(glob, i);
      if (group !== null) {
        out += "(?:" + group.alts.map(globBody).join("|") + ")";
        i = group.end + 1;
      } else {
        out += "\\{";
        i += 1;
      }
    } else {
      // Escape every RegExp metacharacter (including `}` and `,`, which are only special inside a group,
      // handled above) so it matches literally.
      out += ch.replace(/[.+^${}()|[\]\\,]/g, "\\$&");
      i += 1;
    }
  }
  return out;
}

/**
 * If a real brace group opens at `open` (glob[open] === "{"), return its top-level comma-separated
 * alternatives and the index of the matching `}`. Returns null when the `{` is unmatched or the group
 * has no top-level comma (a comma-less `{x}` is a literal, matching bash brace-expansion semantics).
 * Depth-aware, so nested `{…}` are kept intact within an alternative.
 */
function findBraceGroup(glob: string, open: number): { alts: string[]; end: number } | null {
  if (glob[open] !== "{") return null;
  const alts: string[] = [];
  let depth = 0;
  let start = open + 1;
  let sawTopComma = false;
  for (let i = open; i < glob.length; i++) {
    const ch = glob[i];
    if (ch === "{") {
      depth += 1;
    } else if (ch === "}") {
      depth -= 1;
      if (depth === 0) {
        alts.push(glob.slice(start, i));
        return sawTopComma ? { alts, end: i } : null;
      }
    } else if (ch === "," && depth === 1) {
      alts.push(glob.slice(start, i));
      start = i + 1;
      sawTopComma = true;
    }
  }
  return null; // unmatched `{`
}
