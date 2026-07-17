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

/** One run of a result line: matched text (rendered highlighted) or plain text. */
export interface Segment {
  text: string;
  match: boolean;
}

/** Split `line` into matched / unmatched [`Segment`]s for `query` (case-insensitive unless
 *  `caseSensitive`) — non-overlapping substrings, left to right — so the results list can `<mark>` the
 *  hits (CPE-557). A blank query yields the whole line as one unmatched segment. Pure. */
export function highlightSegments(line: string, query: string, caseSensitive = false): Segment[] {
  if (!query) return [{ text: line, match: false }];
  const hay = caseSensitive ? line : line.toLowerCase();
  const needle = caseSensitive ? query : query.toLowerCase();
  const out: Segment[] = [];
  let i = 0;
  while (i < line.length) {
    const idx = hay.indexOf(needle, i);
    if (idx === -1) {
      out.push({ text: line.slice(i), match: false });
      break;
    }
    if (idx > i) out.push({ text: line.slice(i, idx), match: false });
    out.push({ text: line.slice(idx, idx + needle.length), match: true });
    i = idx + needle.length;
  }
  return out;
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
