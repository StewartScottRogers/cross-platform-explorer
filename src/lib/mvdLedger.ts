// CPE-1922: the Manual Verification Debt (MVD) total in
// `.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md` used to be a **running number patched forward**
// by each shift (add what you logged, subtract what you automated) rather than a **count of the
// ledger's own tables**. It drifted, in both directions, for weeks: the header claimed 16 while the
// tables held 12.
//
// This module is the fix in the shape this repo believes in — **derive the number, do not restate
// it** (CPE-1933 on provenance claims, CPE-1932 on enumerating rather than recalling). It parses
// the ledger's tables and reports what they actually contain; `mvdLedger.test.ts` fails CI when the
// header disagrees with that.
//
// It is deliberately a **parser, not a grep**. The bug that motivates the strictness is real and
// recent: a stray blank line between two rows silently detached five rows from the Ledger table
// (they stopped rendering as a table at all), and it was found by *rendering* the page, not by
// reading it. A counter that quietly returns a smaller number when the table is malformed is the
// same defect one level up — so every malformation below is a **loud failure**, never a smaller
// count.

/** The four status markers the ledger's Legend defines. The key is the marker glyph as it appears
 *  in a Status cell; the value is the canonical status name. */
export const STATUS_MARKERS: Readonly<Record<string, MvdStatus>> = {
  "⛰": "manual", // ⛰ still needs human eyes
  "\u{1F527}": "in progress", // 🔧 automation ticket open
  "\u{1F7E1}": "partial", // 🟡 one sub-surface automated, the rest still human
  "✅": "automated", // ✅ retired, pinned by a CI/guard job
};

export type MvdStatus = "manual" | "in progress" | "partial" | "automated";

/** **The counting rule**, documented once here and in the ledger's own Legend so the two cannot
 *  drift apart (this ambiguity is half of how the old drift happened: "6 primary" counted these,
 *  "4 primary manual" did not).
 *
 *  A row is **MVD** when a human still has to look at something for the row's claim to hold. That
 *  is true of `⛰ manual` (nobody has started), of `🔧 in progress` (an automation ticket is open,
 *  but until it lands a human is still the test), and of `🟡 partial` (some sub-surface is pinned,
 *  the rest is not). Only `✅ automated` — the whole row pinned by a named CI/guard job — leaves
 *  MVD. Anything else would let a row claim credit for automation that does not exist yet. */
export const STILL_MANUAL: ReadonlySet<MvdStatus> = new Set<MvdStatus>(["manual", "in progress", "partial"]);

/** How a table participates in the count, declared by its `<!-- mvd-table: ... -->` annotation.
 *  - `primary` — the numbered Ledger; the "N primary" half of the header.
 *  - `supplementary` — a dated per-shift debt table; the "N supplementary" half.
 *  - `excluded` — a historical table whose Status cells predate the marker Legend. Counted by
 *    nobody, but still parsed and still required to be well-formed, and it must carry a reason. */
export type MvdTableKind = "primary" | "supplementary" | "excluded";

export interface MvdRow {
  /** The table this row belongs to. */
  kind: MvdTableKind;
  /** 1-based line number in the source file — so a failure names a line, not "somewhere". */
  line: number;
  /** First cell: the row's `#` (primary) or `Ticket(s)` (supplementary). */
  id: string;
  /** Canonical status from the row's single marker cell. */
  status: MvdStatus;
}

export interface MvdTable {
  kind: MvdTableKind;
  /** The annotation's trailing note (the reason, for `excluded`). */
  note: string;
  /** 1-based line of the table's header row. */
  line: number;
  columns: number;
  /** Data rows in the table, INCLUDING an `excluded` table's (whose rows are not in `rows`). A whole
   *  table going missing is easier to see here than in a total that merely got smaller. */
  dataRows: number;
  rows: MvdRow[];
}

export interface MvdHeaderTotals {
  primary: number;
  supplementary: number;
  total: number;
  /** 1-based line of the header sentence. */
  line: number;
  /** The header sentence exactly as written, for failure messages. */
  text: string;
}

export interface MvdLedger {
  tables: MvdTable[];
  /** Every counted row (primary + supplementary), in file order. */
  rows: MvdRow[];
  /** Derived counts — what the tables actually say. */
  counted: {
    primary: number;
    supplementary: number;
    total: number;
    /** Per-status breakdown of the counted still-manual rows. */
    byStatus: Record<MvdStatus, number>;
  };
  /** What the header sentence asserts. */
  header: MvdHeaderTotals;
}

/** Thrown for every malformation. Carries the 1-based line so a red names a place. */
export class MvdLedgerError extends Error {
  readonly line: number;
  constructor(message: string, line: number) {
    super(line > 0 ? `${message} (line ${line})` : message);
    this.name = "MvdLedgerError";
    this.line = line;
  }
}

const ANNOTATION = /^<!--\s*mvd-table:\s*(primary|supplementary|excluded)\s*(.*?)\s*-->$/;

/** `**MVD (still-manual surfaces): P primary + S supplementary = T total**`.
 *
 *  Kept byte-compatible with the `manual-test-mvd` measurer in `scripts/ratchet-baselines.mjs`
 *  (CPE-1934), which reads the same sentence out of this same file: it matches
 *  `\*\*MVD \(still-manual surfaces\):[^*]*?=\s*(\d+)\s*total\*\*`, so nothing between the colon
 *  and `= T total**` may contain an asterisk. Changing this shape breaks that measurer. */
const HEADER = /\*\*MVD \(still-manual surfaces\): (\d+) primary \+ (\d+) supplementary = (\d+) total\*\*/;

/** A GFM table delimiter row: `|---|:--:|---|`. */
const DELIMITER_CELL = /^:?-{3,}:?$/;

/**
 * A line GFM will render as a table row.
 *
 * **Up to three leading spaces, and no more.** This is not pedantry — it was a real blind spot found in
 * review. Gating on `startsWith("|")` made the parser skip an indented table entirely: indenting the
 * 2026-08-10 supplementary table by two spaces left the rendered page **byte-identical** (GFM allows the
 * indent) while the parser's total silently fell 13 → 10, and the test then told the next shift to write
 * 10 into the header. A guard that launders an under-count as verified is worse than no guard, and it is
 * reachable by an ordinary edit — nesting a debt table under a bullet, which this file's style invites.
 *
 * Four or more spaces is an **indented code block** in GFM, not a table, so it is deliberately NOT
 * matched here — but it is not ignored either: `INDENTED_CODE_ROW` below reds on it, because a row a
 * human meant as a table row and GFM renders as code is the same silent-loss failure wearing a hat.
 */
const TABLE_ROW = /^ {0,3}\|/;

/** A pipe row indented far enough that GFM renders it as code, not as a table row. */
const INDENTED_CODE_ROW = /^ {4,}\|/;

/** Opening/closing fence of a fenced code block (``` or ~~~), with GFM's own 0–3 space indent. */
const FENCE = /^ {0,3}(`{3,}|~{3,})/;

/**
 * Which lines sit inside a fenced code block. Those lines are not markdown at all, so a `|` there is
 * text. Without this, a future shift quoting an example table row inside a fence — likely, now that this
 * ledger documents its own table format — would red with a confusing "table is not annotated".
 *
 * **Exported so the guard test reuses this exact model rather than keeping a second, simpler one.** A
 * test that toggled on any fence line diverged from this one on a ``` block containing a `~~~` line, and
 * reported the divergence as a *table* problem — a guard reddening a legal file, with a message pointing
 * at the wrong thing. One model, not two.
 *
 * Matches CommonMark on the two rules that bite: the closer must use the **same character** as the
 * opener, and be **at least as long**. A four-backtick fence is therefore not closed by a three-backtick
 * line. (Comparing only the first character closed such a block early, which would count a table GFM
 * renders as code — an over-count rather than a silent loss, but wrong either way. A sibling PR hit the
 * same class of bug on the same day.)
 */
export function fencedLines(lines: string[]): boolean[] {
  const inFence = new Array<boolean>(lines.length).fill(false);
  let openChar: string | null = null;
  let openLen = 0;
  for (let i = 0; i < lines.length; i++) {
    const m = FENCE.exec(lines[i]);
    if (openChar === null) {
      if (m) {
        openChar = m[1][0];
        openLen = m[1].length;
        inFence[i] = true;
      }
    } else {
      inFence[i] = true;
      if (m && m[1][0] === openChar && m[1].length >= openLen) openChar = null;
    }
  }
  return inFence;
}

/**
 * A line that ANNOUNCES a table — anchored at the start of its line, which is what distinguishes a real
 * annotation from this ledger's own prose describing the format inside backticks.
 *
 * The guard test compares the number of these against the number of tables the parser actually built.
 * That equality is the floor that a hard-coded `tables.length >= N` cannot be: `>= 8` goes slack the
 * moment a ninth table is added without bumping it, and a table can then vanish undetected — which is
 * exactly what made the blockquoted-table variant durable in review.
 *
 * `[\s>]` and not `[\s]` for the same reason as the guard test's loose row matcher: a table announced
 * from inside a blockquote must still be *seen* to be announced, or the floor moves in lockstep with the
 * thing it is supposed to catch and passes.
 */
export const TABLE_ANNOUNCEMENT = /^[\s>]*<!--\s*mvd-table:/;

/**
 * Split one table line into cells the way GFM does: on **unescaped** pipes only.
 *
 * `\|` inside a cell is an escaped pipe and does NOT start a new cell — the ledger really does
 * contain one (`grep -rl 'status-bar\|\.sb-'` in a CPE-1708 row), and a naive `line.split("|")`
 * reads that row as having one column too many. That miscount is exactly the failure mode this
 * module exists to prevent, so the splitter has to be right rather than nearly right.
 */
export function splitRow(line: string): string[] {
  const cells: string[] = [];
  let cell = "";
  for (let i = 0; i < line.length; i++) {
    const ch = line[i];
    if (ch === "\\" && i + 1 < line.length) {
      cell += ch + line[i + 1];
      i++;
      continue;
    }
    if (ch === "|") {
      cells.push(cell);
      cell = "";
      continue;
    }
    cell += ch;
  }
  cells.push(cell);
  // A GFM row is written with a leading and a trailing pipe here, so the first and last pieces are
  // the empty strings outside them. Anything else means the row is not delimited as written.
  if (cells.length < 3 || cells[0].trim() !== "" || cells[cells.length - 1].trim() !== "") {
    throw new MvdLedgerError(
      `table row is not delimited by a leading AND a trailing pipe: ${line.slice(0, 80)}`,
      0,
    );
  }
  return cells.slice(1, -1).map((c) => c.trim());
}

/** The markers present in a cell, deduplicated, in a stable order. */
function markersIn(cell: string): string[] {
  return Object.keys(STATUS_MARKERS).filter((m) => cell.includes(m));
}

/**
 * Parse the burndown ledger. Throws `MvdLedgerError` on **any** malformation rather than returning
 * a count computed from whatever happened to parse.
 */
export function parseBurndown(src: string): MvdLedger {
  const lines = src.split(/\r?\n/);

  // --- the header sentence -------------------------------------------------------------------
  const headerHits: MvdHeaderTotals[] = [];
  lines.forEach((text, i) => {
    const m = HEADER.exec(text);
    if (m) {
      headerHits.push({
        primary: Number(m[1]),
        supplementary: Number(m[2]),
        total: Number(m[3]),
        line: i + 1,
        text: m[0],
      });
    }
  });
  if (headerHits.length === 0) {
    throw new MvdLedgerError(
      "no MVD header sentence found — it must read exactly " +
        "`**MVD (still-manual surfaces): P primary + S supplementary = T total**`",
      0,
    );
  }
  if (headerHits.length > 1) {
    throw new MvdLedgerError(
      `the MVD header sentence appears ${headerHits.length} times (lines ${headerHits
        .map((h) => h.line)
        .join(", ")}) — exactly one line may state the total, or restating it is the drift again`,
      headerHits[1].line,
    );
  }
  const header = headerHits[0];
  if (header.primary + header.supplementary !== header.total) {
    throw new MvdLedgerError(
      `the header does not add up: ${header.primary} primary + ${header.supplementary} ` +
        `supplementary = ${header.primary + header.supplementary}, not ${header.total}`,
      header.line,
    );
  }

  // --- the tables ----------------------------------------------------------------------------
  const inFence = fencedLines(lines);
  const tables: MvdTable[] = [];
  for (let i = 0; i < lines.length; i++) {
    if (inFence[i]) continue;
    if (INDENTED_CODE_ROW.test(lines[i])) {
      throw new MvdLedgerError(
        "a table row indented four or more spaces is an indented CODE BLOCK in GFM, not a table row — " +
          "it will not render as part of any table. Outdent it to at most three spaces, or put it in a " +
          "fenced block if it really is sample text. (Silently skipping it is how a whole table could " +
          "vanish from the count while the page still looked right.)",
        i + 1,
      );
    }
    if (!TABLE_ROW.test(lines[i])) continue;

    // Maximal run of consecutive table rows. GFM allows each one 0–3 leading spaces, so the run is
    // matched on `TABLE_ROW` and every line is trimmed before it is split into cells.
    const start = i;
    let end = i;
    while (end + 1 < lines.length && !inFence[end + 1] && TABLE_ROW.test(lines[end + 1])) end++;
    i = end;

    const block = lines.slice(start, end + 1).map((l) => l.trim());
    const at = (n: number) => start + n + 1; // 1-based line of block row n

    // The annotation must be the nearest preceding non-blank line. That single rule is what makes a
    // *split* table impossible to miss: a blank line inside a table leaves the second half with a
    // table row as its nearest preceding non-blank line, which is not an annotation, so it reds.
    let p = start - 1;
    while (p >= 0 && lines[p].trim() === "") p--;
    // The annotation may wrap over several lines (the `excluded` ones carry a paragraph of reason), so
    // fold an HTML comment back into one line before matching.
    let annText = p >= 0 ? lines[p].trim() : "";
    if (p >= 0 && annText.endsWith("-->") && !annText.startsWith("<!--")) {
      let q = p;
      while (q > 0 && !lines[q].includes("<!--")) q--;
      if (lines[q].includes("<!--")) {
        annText = lines
          .slice(q, p + 1)
          .map((l) => l.trim())
          .join(" ")
          .replace(/\s+/g, " ");
      }
    }
    const ann = p >= 0 ? ANNOTATION.exec(annText) : null;
    if (!ann) {
      const prev = p >= 0 ? lines[p].trim().slice(0, 60) : "start of file";
      throw new MvdLedgerError(
        "table is not annotated — every table in this ledger must be preceded by " +
          "`<!-- mvd-table: primary|supplementary|excluded ... -->`. A table row appearing here " +
          "usually means a blank line or a non-pipe line SPLIT a table in two, which is how five " +
          `Ledger rows once stopped rendering. Nearest preceding non-blank line: "${prev}"`,
        at(0),
      );
    }
    const kind = ann[1] as MvdTableKind;
    const note = ann[2].replace(/^[\s—-]+/, "").trim();
    if (kind === "excluded" && note === "") {
      throw new MvdLedgerError("an `excluded` table must state its reason in the annotation", at(0));
    }

    // Well-formedness: header row, delimiter row, at least one data row, uniform column count.
    if (block.length < 3) {
      throw new MvdLedgerError(
        `table has ${block.length} line(s) — it needs a header row, a delimiter row and at least one data row`,
        at(0),
      );
    }
    let columns: number;
    try {
      columns = splitRow(block[0]).length;
    } catch (e) {
      throw new MvdLedgerError((e as Error).message.replace(/ \(line 0\)$/, ""), at(0));
    }
    const delim = splitRow(block[1]);
    if (delim.length !== columns || !delim.every((c) => DELIMITER_CELL.test(c))) {
      throw new MvdLedgerError(
        "the line under a table header must be a GFM delimiter row (`|---|---|`) with one cell per " +
          `column (${columns}) — without it the block renders as a paragraph of pipes, not a table`,
        at(1),
      );
    }

    const rows: MvdRow[] = [];
    for (let n = 2; n < block.length; n++) {
      let cells: string[];
      try {
        cells = splitRow(block[n]);
      } catch (e) {
        throw new MvdLedgerError((e as Error).message.replace(/ \(line 0\)$/, ""), at(n));
      }
      if (cells.length !== columns) {
        throw new MvdLedgerError(
          `row has ${cells.length} cells but the table header has ${columns} — a missing pipe, an ` +
            "extra one, or a literal `|` that needs escaping as `\\|`",
          at(n),
        );
      }
      if (kind === "excluded") continue;

      const marked = cells.map((c, idx) => ({ idx, cell: c, markers: markersIn(c) })).filter((c) => c.markers.length > 0);
      if (marked.length === 0) {
        throw new MvdLedgerError(
          "row has no status marker in any cell — every counted row must carry exactly one of " +
            `${Object.keys(STATUS_MARKERS).join(" ")} so its status is readable rather than inferred`,
          at(n),
        );
      }
      if (marked.length > 1) {
        throw new MvdLedgerError(
          `row carries status markers in ${marked.length} different cells (columns ` +
            `${marked.map((m) => m.idx).join(", ")}) — the Status cell must be the only one, or which ` +
            "cell is the status becomes a guess",
          at(n),
        );
      }
      if (marked[0].markers.length > 1) {
        throw new MvdLedgerError(
          `the Status cell carries ${marked[0].markers.length} different markers ` +
            `(${marked[0].markers.join(" ")}) — a row has exactly one status`,
          at(n),
        );
      }
      rows.push({ kind, line: at(n), id: cells[0], status: STATUS_MARKERS[marked[0].markers[0]] });
    }

    tables.push({ kind, note, line: at(0), columns, dataRows: block.length - 2, rows });
  }

  // --- CPE-1932: a guard that measured nothing must go red, not green ------------------------
  if (!tables.some((t) => t.kind === "primary")) {
    throw new MvdLedgerError("no `primary` table found — the numbered Ledger must be annotated as `primary`", 0);
  }
  if (tables.filter((t) => t.kind === "primary").length > 1) {
    throw new MvdLedgerError("more than one `primary` table — there is exactly one numbered Ledger", 0);
  }
  if (!tables.some((t) => t.kind === "supplementary")) {
    throw new MvdLedgerError("no `supplementary` table found — the per-shift debt tables must be annotated", 0);
  }

  const counted = tables.flatMap((t) => t.rows);
  const still = counted.filter((r) => STILL_MANUAL.has(r.status));
  const byStatus: Record<MvdStatus, number> = { manual: 0, "in progress": 0, partial: 0, automated: 0 };
  for (const r of counted) byStatus[r.status]++;

  const primary = still.filter((r) => r.kind === "primary").length;
  const supplementary = still.filter((r) => r.kind === "supplementary").length;

  return {
    tables,
    rows: counted,
    counted: { primary, supplementary, total: primary + supplementary, byStatus },
    header,
  };
}

/** A one-line human summary of the derived counts, used in failure messages and by the ledger's own
 *  "how this number is produced" note. */
export function describeCounts(l: MvdLedger): string {
  const s = l.counted.byStatus;
  return (
    `${l.counted.primary} primary + ${l.counted.supplementary} supplementary = ${l.counted.total} total ` +
    `(${s.manual} manual, ${s["in progress"]} in progress, ${s.partial} partial; ${s.automated} rows retired)`
  );
}
