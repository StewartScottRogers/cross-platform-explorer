// CPE-521: Agent Board model — column grouping, ordering, counts, and move validation.
import { describe, it, expect } from "vitest";
import { groupByColumn, columnCounts, isValidMove, isColumn, type Card } from "./board";

function card(id: string, column: string, extra: Partial<Card> = {}): Card {
  return { id, title: `t ${id}`, ticket_type: "Feature", priority: "Medium", tags: [], column, ...extra };
}

const cards: Card[] = [
  card("CPE-100", "Backlog"),
  card("CPE-9", "Backlog"),
  card("CPE-3", "Doing"),
  card("CPE-2", "Done"),
  card("CPE-7", "Nonsense"), // unknown column → dropped
];

describe("board model (CPE-521)", () => {
  it("groups cards by column and orders each by numeric id", () => {
    const g = groupByColumn(cards);
    expect(g.Backlog.map((c) => c.id)).toEqual(["CPE-9", "CPE-100"]); // numeric, not lexical
    expect(g.Doing.map((c) => c.id)).toEqual(["CPE-3"]);
    expect(g.Done.map((c) => c.id)).toEqual(["CPE-2"]);
    expect(g.Blocked).toEqual([]);
  });

  it("drops cards with an unknown column", () => {
    const all = Object.values(groupByColumn(cards)).flat();
    expect(all.find((c) => c.id === "CPE-7")).toBeUndefined();
  });

  it("counts per column", () => {
    expect(columnCounts(cards)).toEqual({ Backlog: 2, Doing: 1, Blocked: 0, Deferred: 0, Done: 1 });
  });

  it("isColumn recognizes only the five workflow columns", () => {
    expect(isColumn("Backlog")).toBe(true);
    expect(isColumn("Doing")).toBe(true);
    expect(isColumn("Epics")).toBe(false);
    expect(isColumn("")).toBe(false);
  });

  it("isValidMove requires a known card, a valid column, and an actual change", () => {
    expect(isValidMove(cards, "CPE-3", "Done")).toBe(true);
    expect(isValidMove(cards, "CPE-3", "Doing")).toBe(false); // same column
    expect(isValidMove(cards, "CPE-3", "Nonsense")).toBe(false); // invalid column
    expect(isValidMove(cards, "CPE-999", "Done")).toBe(false); // unknown card
  });
});
