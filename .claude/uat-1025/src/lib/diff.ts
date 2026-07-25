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

/** One run of a modified line: unchanged text, or the changed span (highlighted). */
export interface InlineSeg {
  text: string;
  changed: boolean;
}

/**
 * Approximate **intra-line diff** for a modified line pair (a `del` immediately followed by an `add`):
 * the common leading prefix + trailing suffix are unchanged, the middle differs. Returns segment lists
 * for the old and new text so the diff view can highlight exactly what changed within the line (CPE-570).
 * Cheap + deterministic (prefix/suffix scan, not a full LCS) — good enough for line-level edits.
 */
export function inlineDiff(oldText: string, newText: string): { old: InlineSeg[]; new: InlineSeg[] } {
  let start = 0;
  const max = Math.min(oldText.length, newText.length);
  while (start < max && oldText[start] === newText[start]) start++;
  let endOld = oldText.length;
  let endNew = newText.length;
  while (endOld > start && endNew > start && oldText[endOld - 1] === newText[endNew - 1]) {
    endOld--;
    endNew--;
  }
  const seg = (full: string, s: number, e: number): InlineSeg[] => {
    const out: InlineSeg[] = [];
    if (s > 0) out.push({ text: full.slice(0, s), changed: false });
    if (e > s) out.push({ text: full.slice(s, e), changed: true });
    if (e < full.length) out.push({ text: full.slice(e), changed: false });
    return out.length ? out : [{ text: full, changed: false }];
  };
  return { old: seg(oldText, start, endOld), new: seg(newText, start, endNew) };
}

/** A diff line ready to render, with optional intra-line highlight segments (set on a modified pair). */
export interface RenderLine extends DiffLine {
  segs?: InlineSeg[];
}

/** Annotate a hunk's lines with intra-line highlight segments: each `del` immediately followed by an
 *  `add` is treated as a modified line and both get [`InlineSeg`]s from [`inlineDiff`] (CPE-570). Other
 *  lines pass through unchanged. Pure. */
export function annotateInline(lines: DiffLine[]): RenderLine[] {
  const out: RenderLine[] = lines.map((l) => ({ ...l }));
  for (let i = 0; i < out.length - 1; i++) {
    if (out[i].kind === "del" && out[i + 1].kind === "add") {
      const d = inlineDiff(out[i].text, out[i + 1].text);
      out[i].segs = d.old;
      out[i + 1].segs = d.new;
      i++; // consume the pair
    }
  }
  return out;
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

/** Added / removed line totals for a single file — for a per-file `+N −M` badge (CPE-567). */
export function fileStats(f: DiffFile): { added: number; removed: number } {
  const { added, removed } = diffStats([f]);
  return { added, removed };
}

/** Reconstruct a single file's unified-diff text from the parsed model — for a "copy this file's diff"
 *  action (CPE-572). Faithful for viewing/sharing (omits optional index/mode lines). */
export function toPatch(f: DiffFile): string {
  const old = f.oldPath || "/dev/null";
  const nw = f.newPath || "/dev/null";
  const lines: string[] = [
    `diff --git a/${f.oldPath || old} b/${f.newPath || nw}`,
    `--- ${old === "/dev/null" ? "/dev/null" : "a/" + old}`,
    `+++ ${nw === "/dev/null" ? "/dev/null" : "b/" + nw}`,
  ];
  for (const h of f.hunks) {
    lines.push(h.header);
    for (const l of h.lines) {
      lines.push((l.kind === "add" ? "+" : l.kind === "del" ? "-" : " ") + l.text);
    }
  }
  return lines.join("\n") + "\n";
}

/** A short label for a file's change: its new path, or `old → new` on a rename, or the deleted path. */
export function fileLabel(f: DiffFile): string {
  if (f.newPath === "/dev/null") return `${f.oldPath} (deleted)`;
  if (f.oldPath === "/dev/null") return `${f.newPath} (new)`;
  if (f.oldPath && f.newPath && f.oldPath !== f.newPath) return `${f.oldPath} → ${f.newPath}`;
  return f.newPath || f.oldPath;
}
