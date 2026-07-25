/**
 * Pure helpers for the duplicate-cleanup UI (CPE-428). Deleting duplicates is destructive, so the
 * guard rails live here where they're unit-testable: never let a whole group be removed (always keep
 * at least one copy), and offer a safe default selection (every copy except the first).
 */

export interface DupGroupLite {
  paths: string[];
}

/** The redundant copies to pre-select for removal: every path except the first of each group. */
export function redundantPaths(groups: DupGroupLite[]): string[] {
  return groups.flatMap((g) => g.paths.slice(1));
}

/** True only if every group would still retain at least one (unselected) copy — the safety guard. */
export function keepsOnePerGroup(groups: DupGroupLite[], selected: Set<string>): boolean {
  return groups.every((g) => g.paths.some((p) => !selected.has(p)));
}

/** Drop `removed` paths from the groups, and drop any group that no longer has a duplicate (<2). */
export function pruneGroups<T extends DupGroupLite>(groups: T[], removed: Set<string>): T[] {
  return groups
    .map((g) => ({ ...g, paths: g.paths.filter((p) => !removed.has(p)) }))
    .filter((g) => g.paths.length > 1);
}
