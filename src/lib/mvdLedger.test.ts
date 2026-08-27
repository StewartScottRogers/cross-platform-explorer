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
import { parseBurndown, describeCounts, splitRow, MvdLedgerError, STILL_MANUAL, STATUS_MARKERS } from "./mvdLedger";

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
