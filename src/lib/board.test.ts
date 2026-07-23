// CPE-521: Agent Board model — column grouping, ordering, counts, and move validation.
import { describe, it, expect } from "vitest";
import { groupByColumn, columnCounts, isValidMove, isColumn, ticketTask, groupByLane, laneFor, clampBoardSize, BOARD_MIN_W, BOARD_MIN_H, groupByEpic, todoDone, epicProgress, doneWithArchived, filterCards, epicColumn, groupEpicsByColumn, archivedEpics, filterEpics, type Card, type Epic } from "./board";

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

describe("board card filter (CPE-555)", () => {
  const rows: Card[] = [
    card("CPE-100", "Backlog", { title: "Add parser", tags: ["ready"], priority: "High" }),
    card("CPE-9", "Doing", { title: "Fix cursor bug", ticket_type: "Bug", tags: ["ui"] }),
    card("CPE-2", "Done", { title: "Docs pass", tags: ["docs"] }),
  ];

  it("returns every card for a blank query", () => {
    expect(filterCards(rows, "").length).toBe(3);
    expect(filterCards(rows, "   ").length).toBe(3);
  });

  it("matches id, title, tag, type, and priority (case-insensitive substring)", () => {
    expect(filterCards(rows, "cpe-9").map((c) => c.id)).toEqual(["CPE-9"]); // id
    expect(filterCards(rows, "parser").map((c) => c.id)).toEqual(["CPE-100"]); // title
    expect(filterCards(rows, "DOCS").map((c) => c.id)).toEqual(["CPE-2"]); // tag, case-insensitive
    expect(filterCards(rows, "bug").map((c) => c.id).sort()).toEqual(["CPE-9"]); // type Bug + "bug" in title both → CPE-9 only
    expect(filterCards(rows, "high").map((c) => c.id)).toEqual(["CPE-100"]); // priority
  });

  it("returns nothing when no card matches", () => {
    expect(filterCards(rows, "zzz-nomatch")).toEqual([]);
  });
});

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

  it("doneWithArchived includes archived only when toggled on (CPE-531)", () => {
    const recent = [card("CPE-50", "Done"), card("CPE-40", "Done")];
    const archived = [card("CPE-9", "Done"), card("CPE-3", "Done")];
    // Off → recent only, id-ordered.
    expect(doneWithArchived(recent, archived, false).map((c) => c.id)).toEqual(["CPE-40", "CPE-50"]);
    // On → recent + archived, id-ordered together.
    expect(doneWithArchived(recent, archived, true).map((c) => c.id)).toEqual(["CPE-3", "CPE-9", "CPE-40", "CPE-50"]);
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

describe("epics-as-kanban (CPE-922)", () => {
  const ep = (id: string, status: string): Epic => ({ id, title: `Epic ${id}`, status, tags: ["epic"] });

  it("maps epic status onto Backlog/Doing/Done columns", () => {
    expect(epicColumn("Proposed")).toBe("Backlog");
    expect(epicColumn("")).toBe("Backlog");
    expect(epicColumn("whatever")).toBe("Backlog");
    expect(epicColumn("In Progress")).toBe("Doing");
    expect(epicColumn("active")).toBe("Doing");
    expect(epicColumn("Done")).toBe("Done");
    expect(epicColumn("CLOSED")).toBe("Done");
  });

  it("groups epics into columns, id-ordered", () => {
    const g = groupEpicsByColumn([ep("CPE-100", "Proposed"), ep("CPE-9", "Proposed"), ep("CPE-3", "In Progress"), ep("CPE-2", "Done")]);
    expect(g.Backlog.map((e) => e.id)).toEqual(["CPE-9", "CPE-100"]); // numeric order, not lexical
    expect(g.Doing.map((e) => e.id)).toEqual(["CPE-3"]);
    expect(g.Done.map((e) => e.id)).toEqual(["CPE-2"]);
  });

  it("derives archived epics from epic-tagged Done cards, dropping non-epics", () => {
    const archived: Card[] = [
      card("CPE-50", "Done", { tags: ["epic"] }),
      card("CPE-51", "Done", { tags: ["ready"] }), // not an epic → dropped
      card("CPE-40", "Done", { tags: ["epic", "big-design"] }),
    ];
    const a = archivedEpics(archived);
    expect(a.map((e) => e.id)).toEqual(["CPE-40", "CPE-50"]); // id-ordered, epic-tagged only
    expect(a.every((e) => e.status === "Done")).toBe(true);
  });

  it("filters epics by id, title, or tag", () => {
    const es = [ep("CPE-1", "Proposed"), { id: "CPE-2", title: "Thumbnail pipeline", status: "Done", tags: ["epic"] }];
    expect(filterEpics(es, "thumbnail").map((e) => e.id)).toEqual(["CPE-2"]);
    expect(filterEpics(es, "cpe-1").map((e) => e.id)).toEqual(["CPE-1"]);
    expect(filterEpics(es, "").length).toBe(2);
  });

  it("filterCards also matches the epic field (drill-down)", () => {
    const rows: Card[] = [card("CPE-11", "Doing", { epic: "CPE-503" }), card("CPE-12", "Backlog", { epic: "CPE-999" })];
    expect(filterCards(rows, "CPE-503").map((c) => c.id)).toEqual(["CPE-11"]);
  });
});
