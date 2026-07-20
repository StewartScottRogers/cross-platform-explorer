// Pure line-level text diff (CPE-779, epic CPE-722). Classic LCS: split both texts into lines, find the
// longest common subsequence, and emit an in-order `same`/`del`/`add` sequence — the compute the codebase
// lacked (diff.ts only *parses* unified diffs + does an intra-line prefix/suffix diff). No DOM/IO —
// unit-tested — so the text-compare view (CPE-779) is a thin render. O(n·m); callers cap the input size.

export type LineOp = "same" | "add" | "del";

export interface LineDiffRow {
  op: LineOp;
  text: string;
}

export interface LineDiffResult {
  rows: LineDiffRow[];
  added: number;
  removed: number;
}

/** Diff `oldText` against `newText` line-by-line. `del` = only in old, `add` = only in new, `same` = both. */
export function lineDiff(oldText: string, newText: string): LineDiffResult {
  const a = oldText.length ? oldText.split("\n") : [];
  const b = newText.length ? newText.split("\n") : [];
  const n = a.length;
  const m = b.length;

  // LCS length table: dp[i][j] = LCS length of a[i..] and b[j..].
  const dp: number[][] = Array.from({ length: n + 1 }, () => new Array<number>(m + 1).fill(0));
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      dp[i][j] = a[i] === b[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
    }
  }

  const rows: LineDiffRow[] = [];
  let added = 0;
  let removed = 0;
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (a[i] === b[j]) {
      rows.push({ op: "same", text: a[i] });
      i++;
      j++;
    } else if (dp[i + 1][j] >= dp[i][j + 1]) {
      rows.push({ op: "del", text: a[i] });
      removed++;
      i++;
    } else {
      rows.push({ op: "add", text: b[j] });
      added++;
      j++;
    }
  }
  while (i < n) {
    rows.push({ op: "del", text: a[i++] });
    removed++;
  }
  while (j < m) {
    rows.push({ op: "add", text: b[j++] });
    added++;
  }
  return { rows, added, removed };
}
