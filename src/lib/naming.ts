/**
 * Compute a name that does not collide with any existing name, matching how
 * Windows Explorer / macOS Finder auto-number new items:
 *
 *   "New folder"  ->  "New folder"            (if free)
 *                 ->  "New folder (2)"         (if "New folder" is taken)
 *                 ->  "New folder (3)"         (and so on)
 *
 * The lowest free number is used, so gaps are filled. Matching is
 * case-insensitive because the Windows and macOS filesystems are.
 */
export function uniqueName(base: string, existing: Iterable<string>): string {
  const taken = new Set<string>();
  for (const name of existing) taken.add(name.toLowerCase());

  if (!taken.has(base.toLowerCase())) return base;

  for (let i = 2; ; i++) {
    const candidate = `${base} (${i})`;
    if (!taken.has(candidate.toLowerCase())) return candidate;
  }
}
