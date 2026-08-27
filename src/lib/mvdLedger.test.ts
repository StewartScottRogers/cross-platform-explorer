// CPE-1922: the guard that makes the MVD total a *derived* number.
//
// Two halves, deliberately:
//   1. Against the REAL `.claude/qa-architecture/MANUAL-TEST-BURNDOWN.md` — recount its tables and fail
//      when the header sentence disagrees, naming BOTH numbers so the failure is diagnosable without
//      opening the file. This is the acceptance criterion.
//   2. Against hand-built malformed fixtures — because the way this ledger really broke was a stray
//      blank line between two rows silently detaching five of them, and a counter that returns a
//      smaller number on a malformed table is the same defect one level up. Every malformation below
//      must THROW, never under-count.
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import {
  parseBurndown,
  describeCounts,
  splitRow,
  fencedLines,
  TABLE_ANNOUNCEMENT,
  MvdLedgerError,
  STILL_MANUAL,
  STATUS_MARKERS,
} from "./mvdLedger";

const LEDGER_PATH = join(process.cwd(), ".claude", "qa-architecture", "MANUAL-TEST-BURNDOWN.md");
const source = readFileSync(LEDGER_PATH, "utf8");

describe("MANUAL-TEST-BURNDOWN.md — the MVD total is derived from its own tables", () => {
  it("parses without any malformation", () => {
    // A throw here is the point: it names the exact line and what is wrong with it.
    expect(() => parseBurndown(source)).not.toThrow();
  });

  it("the header sentence equals a fresh count of the tables", () => {
    const l = parseBurndown(source);
    const stated = `${l.header.primary} primary + ${l.header.supplementary} supplementary = ${l.header.total} total`;
    const counted = `${l.counted.primary} primary + ${l.counted.supplementary} supplementary = ${l.counted.total} total`;
    expect(
      counted,
      `MANUAL-TEST-BURNDOWN.md's header (line ${l.header.line}) claims ${stated}, but a fresh count of its ` +
        `own tables says ${counted}.\n` +
        `Derived: ${describeCounts(l)}.\n` +
        "Do NOT patch the header forward from its old value — that is the bug CPE-1922 fixed. Set it to " +
        "the counted number above. Rows that count as MVD are ⛰ manual, 🔧 in progress and 🟡 partial; " +
        "only ✅ automated leaves MVD.",
    ).toBe(stated);
  });

  it("every counted row's still-manual status is one the Legend defines", () => {
    const l = parseBurndown(source);
    for (const row of l.rows) {
      expect(Object.values(STATUS_MARKERS)).toContain(row.status);
    }
    // CPE-1932: a guard that measured almost nothing must go red, not green. If a future edit detaches
    // most of the ledger, the counts collapse — these floors catch that before the total does.
    expect(l.rows.length, "the ledger's counted tables have almost no rows — did an edit detach them?").toBeGreaterThan(15);
    expect(l.tables.filter((t) => t.kind === "supplementary").length).toBeGreaterThan(2);
  });

  // Review finding: the two floors above were too loose to notice a WHOLE TABLE vanishing. Indenting
  // one supplementary table by two spaces (legal GFM, rendered page byte-identical) dropped the total
  // 13 -> 10 while `rows.length` only went 24 -> 19 and the supplementary-table count only 5 -> 4 —
  // both floors still passed, and the test then instructed the next shift to write 10 into the header.
  // These two assertions are the floors that would have caught it.
  it("no table can go missing: every announced table was actually parsed", () => {
    // DERIVED, not hard-coded. Review round 3: `tables.length >= 8` goes slack the moment a ninth table
    // is added without bumping it — and a table can then vanish undetected, which is what made the
    // blockquoted-table variant durable. Counting the `<!-- mvd-table: -->` announcements instead means
    // adding a table raises the expected count automatically, and a table that stops being parsed while
    // its announcement remains reds immediately.
    const lines = source.split(/\r?\n/);
    const fenced = fencedLines(lines);
    const announced = lines.filter((line, i) => !fenced[i] && TABLE_ANNOUNCEMENT.test(line)).length;
    const l = parseBurndown(source);
    expect(
      l.tables.length,
      `${announced} tables are announced with an <!-- mvd-table: ... --> comment, but the parser built ` +
        `${l.tables.length}. A table disappeared from the parser's view — the rendered page can look ` +
        "unchanged while a whole table stops being counted. Find the table; do not delete the annotation.",
    ).toBe(announced);
    expect(announced, "the ledger announces almost no tables — did an edit gut the file?").toBeGreaterThan(4);
    for (const t of l.tables) {
      expect(t.dataRows, `the ${t.kind} table at line ${t.line} has no data rows`).toBeGreaterThan(0);
    }
  });

  it("every table-shaped line in the file is accounted for by some parsed table", () => {
    // Deliberately detected with a LOOSER matcher than the parser's own gate (`/^ {0,3}\|/`). If the
    // gate ever narrows again — the bug review round 2 caught — the loose count exceeds what the tables
    // account for and this reds, instead of the number quietly getting smaller.
    //
    // `[\s>]` and not `[\s]`: round 3 found the one variant neither floor caught. A NEW debt table
    // logged inside a blockquote renders as a 9th table on GitHub with its rows fully visible, while
    // `>` is not whitespace — so the loose matcher rejected the row exactly as the gate did, both
    // counts moved together, and three real ⛰ rows went uncounted with every check green. One
    // character fixes it. (Reusing the parser's own `fencedLines` rather than a second, simpler fence
    // model: a test that toggled on any fence line diverged on a ``` block containing a `~~~` line and
    // reported it as a table problem.)
    const lines = source.split(/\r?\n/);
    const fenced = fencedLines(lines);
    const looseRows = lines.filter((line, i) => !fenced[i] && /^[\s>]*\|/.test(line)).length;
    const l = parseBurndown(source);
    const accounted = l.tables.reduce((n, t) => n + 2 + t.dataRows, 0); // header + delimiter + rows
    expect(
      accounted,
      `${looseRows} lines in the ledger look like table rows, but the parsed tables only account for ` +
        `${accounted} of them. Some rows are outside every table — they are not being counted, and on ` +
        "GitHub they are probably not rendering as a table either. A blockquoted (`> |`) or " +
        "tab-indented table is the usual cause.",
    ).toBe(looseRows);
  });

  it("still-manual is exactly ⛰ + 🔧 + 🟡 (documented in the ledger's own Legend)", () => {
    const l = parseBurndown(source);
    const still = l.rows.filter((r) => STILL_MANUAL.has(r.status)).length;
    expect(still).toBe(l.counted.total);
    expect(l.counted.byStatus.automated).toBe(l.rows.length - still);
    // The counting rule must be written down where a QA-Architect reads it, not only here.
    expect(source).toContain("### How the total is counted (CPE-1922)");
  });

  it("keeps the header shape the CPE-1934 ratchet measurer reads", () => {
    // `scripts/ratchet-baselines.mjs`'s `manual-test-mvd` entry (PR #1052) reads this exact sentence out
    // of this exact file. If its shape changes, that measurer throws "no MVD total found".
    const m = /\*\*MVD \(still-manual surfaces\):[^*]*?=\s*(\d+)\s*total\*\*/.exec(source);
    expect(m, "the CPE-1934 ratchet measurer's regex no longer matches the header sentence").not.toBeNull();
    expect(Number(m![1])).toBe(parseBurndown(source).counted.total);
  });
});

// ---------------------------------------------------------------------------------------------
// Fixtures. Small, complete ledgers — the parser is pure, so every failure mode is drivable.
// ---------------------------------------------------------------------------------------------

const HEAD = (p: number, s: number, t: number) =>
  `# fixture\n\n**MVD (still-manual surfaces): ${p} primary + ${s} supplementary = ${t} total**\n\n`;

const PRIMARY = [
  "<!-- mvd-table: primary -->",
  "",
  "| # | Aspect | Status | Ticket |",
  "|---|--------|--------|--------|",
  "| 1 | tray | ⛰ manual | CPE-1 |",
  "| 2 | updater | 🟡 partial | CPE-2 |",
  "| 3 | visual | 🔧 in progress | CPE-3 |",
  "| 4 | backend | ✅ automated | CPE-4 |",
].join("\n");

const SUPP = [
  "<!-- mvd-table: supplementary -->",
  "",
  "| Ticket | Surface | Status | Logged |",
  "|--------|---------|--------|--------|",
  "| CPE-9 | status bar | ⛰ manual | 2026-08-20 |",
  "| CPE-8 | trash | ✅ automated | 2026-08-20 |",
].join("\n");

const GOOD = HEAD(3, 1, 4) + PRIMARY + "\n\n" + SUPP + "\n";

describe("parseBurndown — the happy path", () => {
  it("counts ⛰, 🔧 and 🟡 as MVD and ✅ as retired", () => {
    const l = parseBurndown(GOOD);
    expect(l.counted).toMatchObject({
      primary: 3,
      supplementary: 1,
      total: 4,
      byStatus: { manual: 2, "in progress": 1, partial: 1, automated: 2 },
    });
    expect(describeCounts(l)).toContain("3 primary + 1 supplementary = 4 total");
  });

  // Review finding, BLOCKING. `startsWith("|")` made an indented table invisible: GFM allows a table
  // row up to three leading spaces, so the rendered page was byte-identical while the parser silently
  // stopped counting a whole table — and the test then coached the next shift to write the smaller
  // number into the header. A guard that launders an under-count as verified is worse than no guard.
  describe("a table indented the way GFM allows is still counted", () => {
    for (const indent of ["", " ", "  ", "   "]) {
      it(`${indent.length} leading space(s) — counted identically`, () => {
        const nudged = SUPP.split("\n")
          .map((l) => (l.startsWith("|") ? indent + l : l))
          .join("\n");
        const l = parseBurndown(HEAD(3, 1, 4) + PRIMARY + "\n\n" + nudged + "\n");
        expect(l.counted.total).toBe(4);
        expect(l.tables.length).toBe(2);
        expect(l.tables[1].dataRows).toBe(2);
      });
    }

    it("FOUR leading spaces is a GFM code block, not a table — and it REDS rather than vanishing", () => {
      const buried = SUPP.split("\n")
        .map((l) => (l.startsWith("|") ? "    " + l : l))
        .join("\n");
      const src = HEAD(3, 1, 4) + PRIMARY + "\n\n" + buried + "\n";
      // The pre-fix behaviour was to skip it in silence and report 3 instead of 4.
      expect(() => parseBurndown(src)).toThrow(/indented four or more spaces is an indented CODE BLOCK/);
    });
  });

  // Review round 3, the variant neither floor caught. Note the shape: it is not an EXISTING table being
  // lost (the announced-vs-parsed floor catches that) — it is a NEW debt table that is never gained.
  // GitHub renders it as a real table with its rows visible, so a human reviewing the ledger sees three
  // ⛰ rows that the total does not include. Nothing about the file looks wrong.
  describe("a debt table logged inside a blockquote is not silently uncounted", () => {
    const QUOTED = [
      "> <!-- mvd-table: supplementary -->",
      ">",
      "> | Ticket | Surface | Status | Logged |",
      "> |--------|---------|--------|--------|",
      "> | CPE-7 | one | ⛰ manual | 2026-08-27 |",
      "> | CPE-6 | two | ⛰ manual | 2026-08-27 |",
      "> | CPE-5 | three | ⛰ manual | 2026-08-27 |",
    ].join("\n");
    const withQuoted = HEAD(3, 1, 4) + PRIMARY + "\n\n" + SUPP + "\n\n" + QUOTED + "\n";

    it("the parser does not count it (blockquoted rows are not this ledger's format)", () => {
      // Documenting the real behaviour: the three rows are simply not seen. That is the danger.
      expect(parseBurndown(withQuoted).counted.total).toBe(4);
    });

    it("but the loose matcher DOES see them, so the accounted-for floor reds", () => {
      const lines = withQuoted.split("\n");
      const fenced = fencedLines(lines);
      const loose = lines.filter((line, i) => !fenced[i] && /^[\s>]*\|/.test(line)).length;
      const accounted = parseBurndown(withQuoted).tables.reduce((n, t) => n + 2 + t.dataRows, 0);
      expect(loose, "the one-character `[\\s>]` fix is what makes these rows visible to the floor").toBe(accounted + 5);
      expect(accounted).not.toBe(loose);
      // and with the pre-fix whitespace-only matcher, both counts moved together and it passed:
      const preFix = lines.filter((line, i) => !fenced[i] && /^\s*\|/.test(line)).length;
      expect(preFix).toBe(accounted);
    });

    it("and the announced-vs-parsed floor reds too", () => {
      const lines = withQuoted.split("\n");
      const fenced = fencedLines(lines);
      const announced = lines.filter((line, i) => !fenced[i] && TABLE_ANNOUNCEMENT.test(line)).length;
      expect(announced).toBe(3);
      expect(parseBurndown(withQuoted).tables.length).toBe(2);
    });
  });

  it("a four-backtick fence is not closed by a three-backtick line", () => {
    // CommonMark: the closer must use the same character AND be at least as long. Comparing only the
    // first character closed the block early and would have counted a table GFM renders as code — an
    // over-count rather than a silent loss, but wrong either way.
    const lines = ["````", "```", "| # | Aspect | Status | Ticket |", "````", "after"];
    expect(fencedLines(lines)).toEqual([true, true, true, true, false]);
    // and the same-length case still closes
    expect(fencedLines(["```", "| x |", "```", "after"])).toEqual([true, true, true, false]);
    // a ``` block containing a ~~~ line stays open — the divergence that made a second fence model
    // in the test report a fence problem as a table problem
    expect(fencedLines(["```", "~~~", "| x |", "```", "after"])).toEqual([true, true, true, true, false]);
  });

  it("a pipe row inside a fenced code block is text, not a table", () => {
    // Latent foot-gun the review flagged: this ledger now documents its own table format, so a future
    // shift quoting an example row inside a fence is likely. It must not red as "not annotated".
    const fenced = "```\n| # | Aspect | Status | Ticket |\n|---|---|---|---|\n| 1 | example | ⛰ manual | CPE-0 |\n```\n";
    const l = parseBurndown(HEAD(3, 1, 4) + PRIMARY + "\n\n" + fenced + "\n" + SUPP + "\n");
    expect(l.counted.total).toBe(4);
    expect(l.tables.length).toBe(2);
  });

  it("reports rows with their real line numbers", () => {
    const l = parseBurndown(GOOD);
    expect(l.rows[0]).toMatchObject({ id: "1", status: "manual", kind: "primary" });
    expect(GOOD.split("\n")[l.rows[0].line - 1]).toContain("| 1 | tray |");
  });
});

describe("splitRow — GFM cell splitting", () => {
  it("splits on unescaped pipes", () => {
    expect(splitRow("| a | b | c |")).toEqual(["a", "b", "c"]);
  });

  it("does NOT split on an escaped pipe — the ledger really contains one", () => {
    // A real CPE-1708 row carries `grep -rl 'status-bar\|\.sb-'`. A naive split reads it as one column
    // too many, which is precisely the kind of miscount this module exists to stop.
    expect(splitRow("| a | grep 'x\\|y' | c |")).toEqual(["a", "grep 'x\\|y'", "c"]);
  });

  it("rejects a row that is not delimited by leading AND trailing pipes", () => {
    expect(() => splitRow("| a | b")).toThrow(/leading AND a trailing pipe/);
  });
});

describe("parseBurndown — a malformed table FAILS LOUDLY, it never counts smaller", () => {
  it("a blank line between two rows (the bug that really happened)", () => {
    // Detaching rows 3 and 4 would silently drop 1 MVD row from the count if the parser just grepped.
    const split = PRIMARY.replace("| 3 | visual", "\n| 3 | visual");
    const src = HEAD(3, 1, 4) + split + "\n\n" + SUPP + "\n";
    expect(() => parseBurndown(src)).toThrowError(MvdLedgerError);
    expect(() => parseBurndown(src)).toThrow(/not annotated/);
    // and the number it would have produced is smaller — proving the throw is load-bearing
    expect(() => parseBurndown(src)).toThrow(/SPLIT a table in two/);
  });

  it("a row wrapped onto a second line (a missing trailing pipe)", () => {
    const wrapped = PRIMARY.replace("| 3 | visual | 🔧 in progress | CPE-3 |", "| 3 | visual | 🔧 in\nprogress | CPE-3 |");
    const src = HEAD(3, 1, 4) + wrapped + "\n\n" + SUPP + "\n";
    expect(() => parseBurndown(src)).toThrow(MvdLedgerError);
  });

  it("a missing pipe inside a row (wrong cell count)", () => {
    const bad = PRIMARY.replace("| 1 | tray | ⛰ manual | CPE-1 |", "| 1 | tray ⛰ manual | CPE-1 |");
    const src = HEAD(3, 1, 4) + bad + "\n\n" + SUPP + "\n";
    expect(() => parseBurndown(src)).toThrow(/3 cells but the table header has 4/);
  });

  it("a stray UNescaped pipe inside a cell (one column too many)", () => {
    const bad = PRIMARY.replace("| 1 | tray | ⛰ manual | CPE-1 |", "| 1 | grep 'a|b' | ⛰ manual | CPE-1 |");
    const src = HEAD(3, 1, 4) + bad + "\n\n" + SUPP + "\n";
    expect(() => parseBurndown(src)).toThrow(/5 cells but the table header has 4/);
  });

  it("a table with no GFM delimiter row — it would render as a paragraph of pipes", () => {
    const bad = PRIMARY.replace("|---|--------|--------|--------|\n", "");
    const src = HEAD(3, 1, 4) + bad + "\n\n" + SUPP + "\n";
    expect(() => parseBurndown(src)).toThrow(/must be a GFM delimiter row/);
  });

  it("an unannotated table cannot arrive uncounted", () => {
    const src = HEAD(3, 1, 4) + PRIMARY + "\n\n" + SUPP.replace("<!-- mvd-table: supplementary -->\n\n", "") + "\n";
    expect(() => parseBurndown(src)).toThrow(/not annotated/);
  });

  it("an `excluded` table must state a reason", () => {
    const src = HEAD(3, 1, 4) + PRIMARY + "\n\n" + SUPP.replace("supplementary -->", "excluded -->") + "\n";
    expect(() => parseBurndown(src)).toThrow(/must state its reason/);
  });

  it("a row with no status marker at all", () => {
    const bad = PRIMARY.replace("⛰ manual", "manual");
    const src = HEAD(3, 1, 4) + bad + "\n\n" + SUPP + "\n";
    expect(() => parseBurndown(src)).toThrow(/no status marker/);
  });

  it("a row with markers in two different cells — which one is the status?", () => {
    const bad = PRIMARY.replace("| 1 | tray | ⛰ manual | CPE-1 |", "| 1 | tray ✅ | ⛰ manual | CPE-1 |");
    const src = HEAD(3, 1, 4) + bad + "\n\n" + SUPP + "\n";
    expect(() => parseBurndown(src)).toThrow(/markers in 2 different cells/);
  });

  it("a Status cell carrying two markers at once", () => {
    const bad = PRIMARY.replace("⛰ manual |", "⛰ manual / ✅ automated |");
    const src = HEAD(3, 1, 4) + bad + "\n\n" + SUPP + "\n";
    expect(() => parseBurndown(src)).toThrow(/2 different markers/);
  });

  it("no primary table at all", () => {
    const src = HEAD(0, 1, 1) + SUPP + "\n";
    expect(() => parseBurndown(src)).toThrow(/no `primary` table found/);
  });

  it("no supplementary table at all", () => {
    const src = HEAD(3, 0, 3) + PRIMARY + "\n";
    expect(() => parseBurndown(src)).toThrow(/no `supplementary` table found/);
  });

  it("a missing header sentence", () => {
    expect(() => parseBurndown("# fixture\n\n" + PRIMARY + "\n\n" + SUPP + "\n")).toThrow(/no MVD header sentence/);
  });

  it("the header stated twice — restating it IS the drift", () => {
    const src = HEAD(3, 1, 4) + "\n**MVD (still-manual surfaces): 3 primary + 1 supplementary = 4 total**\n\n" + PRIMARY + "\n\n" + SUPP + "\n";
    expect(() => parseBurndown(src)).toThrow(/appears 2 times/);
  });

  it("a header whose own halves do not add up", () => {
    const src = HEAD(3, 1, 9) + PRIMARY + "\n\n" + SUPP + "\n";
    expect(() => parseBurndown(src)).toThrow(/does not add up/);
  });
});

describe("the derived count moves when a row's marker moves", () => {
  it("flipping a ⛰ row to ✅ lowers the total by one", () => {
    const flipped = parseBurndown(HEAD(3, 1, 4) + PRIMARY.replace("| 1 | tray | ⛰ manual |", "| 1 | tray | ✅ automated |") + "\n\n" + SUPP + "\n");
    expect(flipped.counted.total).toBe(3);
    expect(flipped.counted.primary).toBe(2);
  });

  it("flipping a ✅ row back to ⛰ raises it by one", () => {
    const raised = parseBurndown(HEAD(3, 1, 4) + PRIMARY.replace("| 4 | backend | ✅ automated |", "| 4 | backend | ⛰ manual |") + "\n\n" + SUPP + "\n");
    expect(raised.counted.total).toBe(5);
  });

  it("a 🟡 partial row counts as debt, not as automated", () => {
    const l = parseBurndown(GOOD);
    expect(l.counted.byStatus.partial).toBe(1);
    expect(STILL_MANUAL.has("partial")).toBe(true);
    expect(STILL_MANUAL.has("automated")).toBe(false);
  });
});
