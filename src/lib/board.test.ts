// CPE-521: Agent Board model — column grouping, ordering, counts, and move validation.
import { describe, it, expect } from "vitest";
import { groupByColumn, columnCounts, isValidMove, isColumn, ticketTask, groupByLane, laneFor, clampBoardSize, BOARD_MIN_W, BOARD_MIN_H, groupByEpic, todoDone, epicProgress, type Card } from "./board";

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

  it("ticketTask builds the injected agent task from the card (CPE-522)", () => {
    expect(ticketTask({ id: "CPE-42", title: "Fix the parser" })).toBe("Work on ticket CPE-42: Fix the parser");
    expect(ticketTask({ id: "CPE-7", title: "   " })).toBe("Work on ticket CPE-7."); // blank title
  });

  it("laneFor surfaces a Doing card tagged 'review' in the virtual Review lane (CPE-523)", () => {
    expect(laneFor(card("CPE-1", "Doing", { tags: ["review"] }))).toBe("Review");
    expect(laneFor(card("CPE-2", "Doing", { tags: ["ready"] }))).toBe("Doing"); // not review
    expect(laneFor(card("CPE-3", "Backlog", { tags: ["review"] }))).toBe("Backlog"); // review only counts in Doing
    expect(laneFor(card("CPE-4", "Done"))).toBe("Done");
  });

  it("groupByLane splits Doing into Doing + Review", () => {
    const cs = [
      card("CPE-1", "Doing", { tags: ["review"] }),
      card("CPE-2", "Doing"),
      card("CPE-3", "Done"),
    ];
    const g = groupByLane(cs);
    expect(g.Review.map((c) => c.id)).toEqual(["CPE-1"]);
    expect(g.Doing.map((c) => c.id)).toEqual(["CPE-2"]);
    expect(g.Done.map((c) => c.id)).toEqual(["CPE-3"]);
  });

  it("groupByEpic + todoDone organize an epic's tickets to-do (top) → done (bottom) (CPE-530)", () => {
    const cs = [
      card("CPE-10", "Backlog", { epic: "CPE-500" }),
      card("CPE-11", "Done", { epic: "CPE-500" }),
      card("CPE-12", "Doing", { epic: "CPE-500" }),
      card("CPE-20", "Backlog"), // no epic
    ];
    const g = groupByEpic(cs);
    expect(g["CPE-500"].map((c) => c.id).sort()).toEqual(["CPE-10", "CPE-11", "CPE-12"]);
    expect(g[""].map((c) => c.id)).toEqual(["CPE-20"]); // no-epic bucket
    const split = todoDone(g["CPE-500"]);
    expect(split.todo.map((c) => c.id)).toEqual(["CPE-10", "CPE-12"]); // non-Done, id-ordered
    expect(split.done.map((c) => c.id)).toEqual(["CPE-11"]);
    expect(epicProgress(cs, "CPE-500")).toEqual({ done: 1, total: 3 });
  });

  it("clampBoardSize enforces the min and the viewport max (CPE-529)", () => {
    // Within bounds → unchanged (rounded).
    expect(clampBoardSize(900, 600, 1920, 1080)).toEqual({ w: 900, h: 600 });
    // Below the minimum → clamped up.
    expect(clampBoardSize(100, 100, 1920, 1080)).toEqual({ w: BOARD_MIN_W, h: BOARD_MIN_H });
    // Above the viewport → clamped down to the viewport.
    expect(clampBoardSize(5000, 5000, 1200, 800)).toEqual({ w: 1200, h: 800 });
    // A tiny viewport never goes below the legible minimum.
    expect(clampBoardSize(500, 500, 300, 300)).toEqual({ w: BOARD_MIN_W, h: BOARD_MIN_H });
  });
});
