/** Content-search types + pure helpers (CPE-417), matching the `search_file_contents` backend. */

export interface ContentMatch {
  path: string;
  line_number: number;
  line: string;
}

export interface ContentSearchResult {
  matches: ContentMatch[];
  files_scanned: number;
  truncated: boolean;
}

export interface FileGroup {
  path: string;
  matches: ContentMatch[];
}

/** Group matches by file, preserving first-seen order — the shape the results list renders. Pure. */
export function groupMatches(matches: ContentMatch[]): FileGroup[] {
  const order: string[] = [];
  const by = new Map<string, ContentMatch[]>();
  for (const m of matches) {
    let bucket = by.get(m.path);
    if (!bucket) {
      bucket = [];
      by.set(m.path, bucket);
      order.push(m.path);
    }
    bucket.push(m);
  }
  return order.map((path) => ({ path, matches: by.get(path)! }));
}

/** The base name of a path, cross-platform (handles `\` and `/`). Pure. */
export function baseName(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  const parts = trimmed.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

/** The parent directory of a path, cross-platform. Empty if there is none. Pure. */
export function parentDir(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  const cut = Math.max(trimmed.lastIndexOf("/"), trimmed.lastIndexOf("\\"));
  return cut > 0 ? trimmed.slice(0, cut) : "";
}
