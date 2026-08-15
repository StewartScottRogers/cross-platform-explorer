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
 * separator is stripped. Mirrors `Sidebar.svelte`'s pre-existing local `norm()` (that idiom already
 * existed for parent/child tree comparisons) — factored out here so every path-keyed consumer in the
 * app agrees with the sidebar's own notion of "the same folder".
 *
 * One case is deliberately NOT stripped: a bare Windows drive letter + colon ("C:") means something
 * different from its root ("C:\") to the OS — the process's current *working* directory on that drive,
 * not the root — so a drive root keeps one trailing separator rather than canonicalising into a value
 * that would misnavigate if it were ever used as a literal path again (e.g. a favourited drive root).
 */
export function canonicalPath(p: string): string {
  const slashed = p.replace(/\\/g, "/");
  // A bare root (or empty string) has no trailing separator to strip in the first place — collapsing
  // "/" down to "" would conflate "the root" with "no path", a different thing everywhere else in this
  // app treats them as (e.g. an empty `currentPath` means Home, not the filesystem root).
  if (slashed === "/" || slashed === "") return slashed;
  const stripped = slashed.replace(/\/+$/, "");
  return /^[A-Za-z]:$/.test(stripped) ? `${stripped}/` : stripped;
}

/** Whether `a` and `b` name the same folder once trailing-slash/separator spelling is ignored. */
export function samePath(a: string, b: string): boolean {
  return canonicalPath(a) === canonicalPath(b);
}
