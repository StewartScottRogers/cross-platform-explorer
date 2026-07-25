/** Filename-search types + a pure sort (CPE-603), matching the `find_files_by_name` backend. */

export interface NameMatch {
  path: string;
  name: string;
  is_dir: boolean;
}

export interface NameSearchResult {
  matches: NameMatch[];
  dirs_scanned: number;
  truncated: boolean;
}

/**
 * Order hits for display: folders before files, then case-insensitively by name, then by full path
 * (so same-named files are grouped by location). Pure + stable — returns a new array.
 */
export function sortNameMatches(matches: NameMatch[]): NameMatch[] {
  return [...matches].sort((a, b) => {
    if (a.is_dir !== b.is_dir) return a.is_dir ? -1 : 1;
    const n = a.name.toLowerCase().localeCompare(b.name.toLowerCase());
    return n !== 0 ? n : a.path.toLowerCase().localeCompare(b.path.toLowerCase());
  });
}
