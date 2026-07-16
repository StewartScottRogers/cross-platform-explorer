// Agent Board model (CPE-521) — the pure, testable core of the Kanban view. Cards come from the
// `board_cards` backend command (CPE-520); the column vocabulary + grouping live here so the Svelte
// component stays thin and the logic is unit-tested headlessly.

export const BOARD_COLUMNS = ["Backlog", "Doing", "Blocked", "Deferred", "Done"] as const;
export type Column = (typeof BOARD_COLUMNS)[number];

export interface Card {
  id: string;
  title: string;
  ticket_type: string;
  priority: string;
  tags: string[];
  epic?: string | null;
  sprint?: string | null;
  column: string;
}

export function isColumn(s: string): s is Column {
  return (BOARD_COLUMNS as readonly string[]).includes(s);
}

/** Numeric part of a CPE id, for stable ordering (CPE-9 before CPE-100). */
function idNum(id: string): number {
  const m = /(\d+)/.exec(id);
  return m ? Number(m[1]) : 0;
}

/** Group cards into their columns, each column ordered by id. Unknown columns are dropped. */
export function groupByColumn(cards: Card[]): Record<Column, Card[]> {
  const out = {} as Record<Column, Card[]>;
  for (const col of BOARD_COLUMNS) out[col] = [];
  for (const c of cards) {
    if (isColumn(c.column)) out[c.column].push(c);
  }
  for (const col of BOARD_COLUMNS) out[col].sort((a, b) => idNum(a.id) - idNum(b.id));
  return out;
}

/** The board's display lanes — the folder columns plus a virtual **Review** lane between Doing and
    Done (CPE-523). Review is not a folder: it's Doing-cards carrying the `review` tag. */
export const BOARD_LANES = ["Backlog", "Doing", "Review", "Blocked", "Deferred", "Done"] as const;
export type Lane = (typeof BOARD_LANES)[number];

/** The lane a card displays in: a Doing card tagged `review` shows in Review; otherwise its column. */
export function laneFor(card: Card): Lane {
  if (card.column === "Doing" && card.tags.includes("review")) return "Review";
  return isColumn(card.column) ? (card.column as Lane) : "Backlog";
}

/** Group cards into display lanes (incl. the virtual Review lane), each ordered by id. */
export function groupByLane(cards: Card[]): Record<Lane, Card[]> {
  const out = {} as Record<Lane, Card[]>;
  for (const l of BOARD_LANES) out[l] = [];
  for (const c of cards) out[laneFor(c)].push(c);
  for (const l of BOARD_LANES) out[l].sort((a, b) => idNum(a.id) - idNum(b.id));
  return out;
}

/** Per-column counts, for the column headers. */
export function columnCounts(cards: Card[]): Record<Column, number> {
  const g = groupByColumn(cards);
  const out = {} as Record<Column, number>;
  for (const col of BOARD_COLUMNS) out[col] = g[col].length;
  return out;
}

/** A move is meaningful only when the card exists and would land in a *different*, valid column. */
export function isValidMove(cards: Card[], id: string, to: string): boolean {
  if (!isColumn(to)) return false;
  const card = cards.find((c) => c.id === id);
  return !!card && card.column !== to;
}

/** The task string injected into a dispatched agent session (CPE-522): names the ticket it should
    work, reusing the CPE-313 explorer→console task hand-off. */
export function ticketTask(card: Pick<Card, "id" | "title">): string {
  const title = card.title.trim();
  return title ? `Work on ticket ${card.id}: ${title}` : `Work on ticket ${card.id}.`;
}
