// CPE-1948 — the enumeration table in `docs/design/RATCHETS.md` is asserted, not maintained.
//
// CPE-1934 built the guard that makes a raised ratchet baseline loud, and wrote all twelve baselines
// into `docs/design/RATCHETS.md` with their current values as literals. Nothing tied those literals
// to the measured ones — so the document explaining why a stored number needs a guard was itself a
// stored number with no guard. It went stale within an hour of landing (`manual-test-mvd` said 16
// after CPE-1922 recounted it to 13), and a second row was found stale on the day CPE-1948 was picked
// up (`bidi-render-registry` said 1552 against a measured 1553, moved by PR #1056 with the ratchet
// job's own run predating the guard's landing).
//
// Two shapes were available and the repo uses both: DELETE the numbers and point at
// `node scripts/ratchet-baselines.mjs print`, or ASSERT them. Asserted, because the numbers are why
// anyone opens that page — the scale of a debt is what tells you whether an allowlist is a rounding
// error or a project, and a page without them is honest and useless. The cost is this file.
//
// Two rules this test is built around, both from CPE-1933:
//
//   Anchor on structure, never on prose (rule 2). The parser locates the table by its exact header
//   row and matches each `today` cell WHOLE. "The first number after the baseline's name" would
//   happily be satisfied by a sentence, by a row of the raise ledger further down the same document,
//   or by the leading digits of a cell whose parenthetical tail nobody checked — and the third of
//   those is not hypothetical, it is what that cell actually contained before this ticket.
//
//   Enumerate, don't recall (CPE-1932). The list of ratchets comes from `REGISTRY` at run time. The
//   document being wrong is this ticket's premise, so the document cannot also be the source of truth
//   for which ratchets exist; the id column is compared to the registry as an ordered whole, which
//   makes "a baseline was registered and never written down" a red too.

import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import {
  REGISTRY,
  LEDGER_PATH,
  parseEnumerationTable,
  parseLedger,
  evaluate,
  measureWorkingTree,
  isUnmeasurable,
} from "../../scripts/ratchet-baselines.mjs";

const ROOT = resolve(__dirname, "..", "..");
const doc = () => readFileSync(join(ROOT, LEDGER_PATH), "utf8");

/** A minimal document carrying just an enumeration table, for the parser's own red-proofs. */
function docWithTable(rows: string[]): string {
  return [
    "# Ratchets",
    "",
    "Some prose about `hex-files`, which stands at 999 today — a sentence, not a row.",
    "",
    "| id | file | what the number counts | today |",
    "|----|------|------------------------|-------|",
    ...rows,
    "",
    "Trailing prose.",
  ].join("\n");
}

describe("the enumeration table matches the measurer (CPE-1948)", () => {
  it("lists exactly the registered baselines, in the registry's order", () => {
    const inDoc = parseEnumerationTable(doc()).map((r) => r.id);
    const inRegistry = REGISTRY.map((b) => b.id);
    expect(
      inDoc,
      `the id column of ${LEDGER_PATH}'s enumeration table has drifted from REGISTRY in ` +
        `scripts/ratchet-baselines.mjs. Registered: ${inRegistry.join(", ")}. Documented: ${inDoc.join(", ")}. ` +
        `A baseline that is gated but undocumented is the same defect one level up — add the row.`,
    ).toEqual(inRegistry);
  });

  // THE assertion. Everything else in this file exists to stop it being satisfiable by accident.
  it("every row's `today` value equals what the measurer reports right now", () => {
    const measured = measureWorkingTree();
    const wrong: string[] = [];
    for (const row of parseEnumerationTable(doc())) {
      const v = measured[row.id];
      if (isUnmeasurable(v)) {
        wrong.push(`${LEDGER_PATH}:${row.line} ${row.id}: could not be measured at all — ${v.failed}`);
        continue;
      }
      if (v !== row.value) {
        wrong.push(`${LEDGER_PATH}:${row.line} ${row.id}: the table says ${row.value}, the measurer reports ${v}`);
      }
    }
    expect(
      wrong,
      `${LEDGER_PATH}'s enumeration table disagrees with scripts/ratchet-baselines.mjs:\n  ${wrong.join("\n  ")}\n` +
        `Run \`node scripts/ratchet-baselines.mjs print\` and correct the table. If a value went UP, that is a ` +
        `raise and needs a raise-ledger row as well (see the same document).`,
    ).toEqual([]);
  });

  it("every row points at the file the registry says it does", () => {
    const byId = new Map(REGISTRY.map((b) => [b.id, b]));
    for (const row of parseEnumerationTable(doc())) {
      expect(row.file, `${LEDGER_PATH}:${row.line} ${row.id} names the wrong file`).toBe(byId.get(row.id)?.file);
    }
  });

  // The "not gated" marker is a claim about the registry, so it is derived from the registry rather
  // than trusted. It is also the row that went stale first, precisely because nothing gated it — being
  // ungated is a reason to watch the documented value harder, not a reason to stop.
  it("the not-gated marker is derived from `unenforced`, in both directions", () => {
    const byId = new Map(REGISTRY.map((b) => [b.id, b]));
    for (const row of parseEnumerationTable(doc())) {
      expect(
        row.gated,
        `${LEDGER_PATH}:${row.line} ${row.id} is marked ${row.gated ? "gated" : "**enumerated, not gated**"} in the ` +
          `table, but REGISTRY says unenforced=${byId.get(row.id)?.unenforced === true}`,
      ).toBe(byId.get(row.id)?.unenforced !== true);
    }
    // Non-vacuity for the direction above: at least one row must actually carry the marker, or the
    // "in both directions" claim is only ever exercised one way.
    expect(parseEnumerationTable(doc()).filter((r) => !r.gated).length).toBeGreaterThan(0);
  });
});

describe("the parser anchors on structure, not prose (CPE-1933 rule 2)", () => {
  it("does not read a number out of a surrounding paragraph", () => {
    // `docWithTable` plants "`hex-files`, which stands at 999 today" above the table.
    const rows = parseEnumerationTable(docWithTable(["| `hex-files` | `a/b.ts` | counts things | 85 |"]));
    expect(rows).toHaveLength(1);
    expect(rows[0].value).toBe(85);
  });

  it("REFUSES a `today` cell with a trailing parenthetical rather than reading its leading digits", () => {
    // Verbatim the cell this document carried before CPE-1948. A "first number in the cell" scanner
    // reads 14 and passes while `(13 → 14 on 2026-08-27, CPE-1946)` goes unasserted forever.
    expect(() =>
      parseEnumerationTable(
        docWithTable([
          "| `manual-test-mvd` | `a/b.md` | surfaces | 14 — **enumerated, not gated** (13 → 14 on 2026-08-27, CPE-1946) |",
        ]),
      ),
    ).toThrow(/must be exactly a number/);
  });

  it("refuses any other decoration of the value cell", () => {
    for (const cell of ["~85~", "85 (was 84)", "about 85", "85 files", "85 — not gated", "8 5"]) {
      expect(() =>
        parseEnumerationTable(docWithTable([`| \`hex-files\` | \`a/b.ts\` | counts things | ${cell} |`])),
        `the value cell ${JSON.stringify(cell)} was accepted`,
      ).toThrow(/today/);
    }
  });

  it("refuses an id or file cell that is not exactly one backticked token", () => {
    expect(() => parseEnumerationTable(docWithTable(["| hex-files | `a/b.ts` | counts | 85 |"]))).toThrow(/id cell/);
    expect(() => parseEnumerationTable(docWithTable(["| `hex-files` | a/b.ts | counts | 85 |"]))).toThrow(/file cell/);
  });

  it("refuses a second enumeration table rather than picking one", () => {
    const two = `${docWithTable(["| `a` | `x` | c | 1 |"])}\n${docWithTable(["| `a` | `x` | c | 2 |"])}`;
    expect(() => parseEnumerationTable(two)).toThrow(/exactly ONE enumeration table/);
  });

  // A parser that returns [] for a document it could not find the table in makes every assertion above
  // vacuously true — the CPE-1932 zero-enumeration false green, in miniature.
  it("throws rather than returning zero rows when the table is missing or empty", () => {
    expect(() => parseEnumerationTable("# Ratchets\n\nNo table here. hex-files is 85.\n")).toThrow(/exactly ONE/);
    expect(() => parseEnumerationTable(docWithTable([]))).toThrow(/no rows/);
    expect(() =>
      parseEnumerationTable("| id | file | what the number counts | today |\n| `a` | `x` | c | 1 |\n"),
    ).toThrow(/separator row/);
  });
});

describe("the raise-ledger mechanism is untouched (CPE-1934)", () => {
  const enumRow = "| `demo` | `src/x.ts` | counts things | 11 |";
  const ledgerRow = "| `demo` | 10 → 11 | CPE-1948 | a declared raise |";
  const both = [
    docWithTable([enumRow]),
    "",
    "## Raise ledger",
    "",
    "| baseline | from → to | ticket | why this raise is right |",
    "|----------|-----------|--------|-------------------------|",
    ledgerRow,
    "",
  ].join("\n");

  it("the two tables do not read each other", () => {
    expect(parseEnumerationTable(both).map((r) => r.id)).toEqual(["demo"]);
    expect(parseEnumerationTable(both)[0].value).toBe(11);
    const ledger = parseLedger(both);
    expect(ledger).toHaveLength(1);
    expect(ledger[0]).toMatchObject({ id: "demo", from: 10, to: 11, ticket: "CPE-1948" });
  });

  it("no row of the live enumeration table can be read as a licence", () => {
    const md = doc();
    const enumerated = parseEnumerationTable(md);
    expect(enumerated.length).toBeGreaterThan(0);
    // The enumeration table's own lines, fed to the ledger parser in isolation. If a `today` value
    // could ever be read as a `from → to` movement, that row would authorise a raise it knows nothing
    // about — the two tables live in one file and the ledger's rows are what make a raise legal.
    const enumerationLines = md
      .split(/\r?\n/)
      .filter((_, i) => enumerated.some((r) => r.line === i + 1))
      .join("\n");
    expect(enumerationLines.split("\n")).toHaveLength(enumerated.length);
    expect(parseLedger(enumerationLines)).toEqual([]);
  });

  // End-to-end at the level that matters: a declared raise, in a document carrying BOTH tables, still
  // passes; the same document without the new row still fails. CPE-1948 must not have made the licence
  // mechanism harder to satisfy or easier to fake.
  it("a raise declared by a new row in the combined document is still authorised", () => {
    const fake = [{ id: "demo", file: "src/x.ts", what: "demo", measure: () => 0 }];
    const declared = parseLedger(both);
    expect(evaluate(fake as never, { demo: 10 }, { demo: 11 }, declared, []).ok).toBe(true);
    // ...and the row is spent: present at the base too, it authorises nothing.
    expect(evaluate(fake as never, { demo: 10 }, { demo: 11 }, declared, declared).ok).toBe(false);
  });
});
