// Integrated workbench — unified-diff parser (CPE-526). Turns `git diff` text into a structured model
// (files → hunks → typed lines) the Diff view renders. Pure + unit-tested; tolerant of malformed input
// (returns what it can, never throws).

export type DiffLineKind = "add" | "del" | "context";
export interface DiffLine {
  kind: DiffLineKind;
  text: string;
  /** 1-based line number in the OLD file (present on `del` + `context`). */
  oldLine?: number;
  /** 1-based line number in the NEW file (present on `add` + `context`). */
  newLine?: number;
}
export interface Hunk {
  header: string; // the @@ … @@ line
  lines: DiffLine[];
}
export interface DiffFile {
  oldPath: string;
  newPath: string;
  binary: boolean;
  hunks: Hunk[];
}

/** Strip a `a/` / `b/` prefix (and normalize `/dev/null`) from a diff path token. */
function cleanPath(p: string): string {
  const t = p.trim().replace(/\t.*$/, ""); // git may append a tab + timestamp
  if (t === "/dev/null") return "/dev/null";
  return t.replace(/^[ab]\//, "");
}

/** Parse unified-diff (git) text into files/hunks/typed lines. Never throws. */
export function parseDiff(text: string): DiffFile[] {
  const files: DiffFile[] = [];
  let cur: DiffFile | null = null;
  let hunk: Hunk | null = null;
  let oldNo = 0; // running line number in the old file, from the @@ header
  let newNo = 0; // running line number in the new file

  for (const line of text.split(/\r?\n/)) {
    if (line.startsWith("diff --git")) {
      const m = /^diff --git (\S+) (\S+)$/.exec(line);
      cur = { oldPath: m ? cleanPath(m[1]) : "", newPath: m ? cleanPath(m[2]) : "", binary: false, hunks: [] };
      files.push(cur);
      hunk = null;
      continue;
    }
    if (!cur) continue;
    if (line.startsWith("Binary files") || line.startsWith("GIT binary patch")) {
      cur.binary = true;
      continue;
    }
    if (line.startsWith("--- ")) {
      cur.oldPath = cleanPath(line.slice(4));
      continue;
    }
    if (line.startsWith("+++ ")) {
      cur.newPath = cleanPath(line.slice(4));
      continue;
    }
    if (line.startsWith("@@")) {
      hunk = { header: line, lines: [] };
      cur.hunks.push(hunk);
      // `@@ -oldStart[,oldCount] +newStart[,newCount] @@` seeds the line counters.
      const m = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/.exec(line);
      oldNo = m ? Number(m[1]) : 0;
      newNo = m ? Number(m[2]) : 0;
      continue;
    }
    if (!hunk) continue; // index/mode/similarity lines between the header and first hunk
    if (line.startsWith("+")) hunk.lines.push({ kind: "add", text: line.slice(1), newLine: newNo++ });
    else if (line.startsWith("-")) hunk.lines.push({ kind: "del", text: line.slice(1), oldLine: oldNo++ });
    else if (line.startsWith(" ")) hunk.lines.push({ kind: "context", text: line.slice(1), oldLine: oldNo++, newLine: newNo++ });
    // a `\ No newline at end of file` marker and blank trailing lines are ignored.
  }
  return files;
}

/** Added / removed line totals across a parsed diff, for a summary. */
export function diffStats(files: DiffFile[]): { added: number; removed: number; files: number } {
  let added = 0;
  let removed = 0;
  for (const f of files) {
    for (const h of f.hunks) {
      for (const l of h.lines) {
        if (l.kind === "add") added++;
        else if (l.kind === "del") removed++;
      }
    }
  }
  return { added, removed, files: files.length };
}

/** A short label for a file's change: its new path, or `old → new` on a rename, or the deleted path. */
export function fileLabel(f: DiffFile): string {
  if (f.newPath === "/dev/null") return `${f.oldPath} (deleted)`;
  if (f.oldPath === "/dev/null") return `${f.newPath} (new)`;
  if (f.oldPath && f.newPath && f.oldPath !== f.newPath) return `${f.oldPath} → ${f.newPath}`;
  return f.newPath || f.oldPath;
}
