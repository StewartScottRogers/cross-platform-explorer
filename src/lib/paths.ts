/**
 * Shared path-canonicalisation helper (CPE-1737 round 2). Every remote directory's wire `path` now
 * legitimately carries a trailing `/` (S3's own spelling of "this is a prefix, not an object" — see
 * `crates/vfs/src/connect.rs`'s `join_remote`), so a same-named file and folder never collide as two
 * rows in ONE listing. But that same folder is also reachable with NO trailing slash from every other
 * route: Up (`parentDir` strips it), a breadcrumb segment, a typed address, a favourite/pin/tag added
 * before this fix shipped, the sidebar tree, session restore. Anything that treats a path as a stable
 * IDENTITY (a key into a persisted store, a cache slot, a history entry, the sidebar's "current folder"
 * highlight) must compare/store the CANONICAL form, or the two spellings read as two different folders.
 *
 * `canonicalPath` is for that identity use ONLY — never for the literal value handed to a backend
 * command. A listing row's own `entry.path` must stay exactly as the backend built it (trailing slash
 * included for a directory) so Svelte's keyed `{#each}` keeps the object row and the prefix row
 * distinct; canonicalising it away there would reintroduce the very collision CPE-1737 fixed, just one
 * level removed (e.g. inside `DropStackPanel.svelte`'s own keyed each if two colliding rows were both
 * canonicalised down to the same key).
 */

/**
 * The canonical form of `p`: backslashes normalise to forward slashes and exactly one trailing
 * separator is stripped — but ONLY when `p` actually had one.
 *
 * **It does NOT agree with [`treePrefixPath`] (the sidebar's `norm`) everywhere, and must not.** This
 * function's own docs used to claim it "mirrors `Sidebar.svelte`'s local `norm()` … so every
 * path-keyed consumer agrees with the sidebar's notion of 'the same folder'", and that claim was
 * false when it was written: `norm` strips trailing separators unconditionally, so `norm("/") === ""`
 * and `norm("C:/") === "C:"`, while this function deliberately preserves both. The *code* on both
 * sides is deliberate — see [`treePrefixPath`] for why the sidebar needs the root to collapse — but
 * the claim of agreement was not. `paths.test.ts` now derives the real relationship from the two
 * functions themselves (they agree on every non-root input and differ exactly at the roots), so the
 * next divergence reds instead of sitting in a comment. CPE-1950.
 *
 * A bare Windows drive letter + colon ("C:", no separator at all) is left EXACTLY as given, never
 * rewritten into a root: it means something different to the OS than its root ("C:\"/"C:/") — the
 * process's current *working* directory on that drive, not the root — so inventing a trailing separator
 * for it would itself corrupt the value (CPE-1737 round 3: an earlier version of this function did
 * exactly that, promoting a bare "C:" into "C:/" because the drive-root guard fired unconditionally,
 * whenever it was already stripping a REAL trailing separator). An input that genuinely IS a root
 * ("C:\" or "C:/") keeps its one trailing separator rather than collapsing down to the bare form —
 * still guarding against a value that would misnavigate if it were ever used as a literal path again
 * (e.g. a favourited drive root).
 */
export function canonicalPath(p: string): string {
  const slashed = p.replace(/\\/g, "/");
  // A bare root (or empty string) has no trailing separator to strip in the first place — collapsing
  // "/" down to "" would conflate "the root" with "no path", a different thing everywhere else in this
  // app treats them as (e.g. an empty `currentPath` means Home, not the filesystem root).
  if (slashed === "/" || slashed === "") return slashed;
  // Nothing to strip: a bare "C:" (or any path with no trailing separator) is returned untouched,
  // rather than letting the drive-root check below invent a separator that was never there.
  if (!slashed.endsWith("/")) return slashed;
  const stripped = slashed.replace(/\/+$/, "");
  return /^[A-Za-z]:$/.test(stripped) ? `${stripped}/` : stripped;
}

/** Whether `a` and `b` name the same folder once trailing-slash/separator spelling is ignored. */
export function samePath(a: string, b: string): boolean {
  return canonicalPath(a) === canonicalPath(b);
}

/**
 * The sidebar tree's normalisation: separators unified and EVERY trailing separator stripped,
 * unconditionally — so `treePrefixPath("/") === ""` and `treePrefixPath("C:/") === "C:"`.
 *
 * This is the single definition of what was `Sidebar.svelte`'s local `norm` (CPE-1950 moved it here;
 * it is the only caller today). It is NOT interchangeable with [`canonicalPath`], and the difference
 * is load-bearing rather than an oversight: the sidebar's `isAncestorOrSelf` asks
 * `b === a || b.startsWith(a + "/")`, and that idiom only works if the root normalises to the empty
 * string. Fed `canonicalPath`'s `"/"` instead, the POSIX root would test `"/home".startsWith("//")`
 * and no root would ever be an ancestor of anything — the tree would stop revealing.
 *
 * Use [`canonicalPath`] for IDENTITY (a key into a persisted store, a cache slot, a history entry);
 * use this one only for the sidebar's prefix arithmetic. `paths.test.ts` pins both the agreement on
 * every non-root input and the deliberate divergence at the roots.
 */
export function treePrefixPath(p: string): string {
  return p.replace(/\\/g, "/").replace(/\/+$/, "");
}
