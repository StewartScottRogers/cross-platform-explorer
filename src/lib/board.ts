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

/** Filter cards by a free-text query — case-insensitive substring across id, title, any tag, type, or
 *  priority. A blank query returns every card unchanged (CPE-555). Pure, so the board stays thin. */
export function filterCards(cards: Card[], query: string): Card[] {
  const q = query.trim().toLowerCase();
  if (!q) return cards;
  return cards.filter(
    (c) =>
      c.id.toLowerCase().includes(q) ||
      c.title.toLowerCase().includes(q) ||
      c.ticket_type.toLowerCase().includes(q) ||
      c.priority.toLowerCase().includes(q) ||
      (c.epic ?? "").toLowerCase().includes(q) ||
      c.tags.some((t) => t.toLowerCase().includes(q)),
  );
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

// --- Epic-organized view (CPE-530): group by epic, split to-do (top) vs done (bottom). ------------
export interface Epic {
  id: string;
  title: string;
  status: string;
  tags: string[];
}

/** The synthetic key for tickets with no epic. */
export const NO_EPIC = "";

/** Group cards by their `epic:` id ("" = no epic). */
export function groupByEpic(cards: Card[]): Record<string, Card[]> {
  const out: Record<string, Card[]> = {};
  for (const c of cards) {
    const k = c.epic || NO_EPIC;
    (out[k] ||= []).push(c);
  }
  return out;
}

/** Split a set of cards into to-do (any non-Done column) and done, each id-ordered. To-do shows on
    top, done on the bottom — the epic view's ordering. */
export function todoDone(cards: Card[]): { todo: Card[]; done: Card[] } {
  const byId = (a: Card, b: Card) => idNum(a.id) - idNum(b.id);
  return {
    todo: cards.filter((c) => c.column !== "Done").sort(byId),
    done: cards.filter((c) => c.column === "Done").sort(byId),
  };
}

/** Per-epic progress `{ done, total }` computed from the cards attached to that epic id. */
export function epicProgress(cards: Card[], epicId: string): { done: number; total: number } {
  const mine = cards.filter((c) => (c.epic || NO_EPIC) === epicId);
  return { done: mine.filter((c) => c.column === "Done").length, total: mine.length };
}

// --- Epics-as-kanban (CPE-922): lay epics out across ticket-style columns instead of a list+detail, so
// the Epics view reads like the tickets board. Epics flow Proposed → In Progress → Done. -------------

/** The Epics view's columns — the epic lifecycle mapped onto the tickets board's Backlog/Doing/Done. */
export const EPIC_COLUMNS = ["Backlog", "Doing", "Done"] as const;
export type EpicColumn = (typeof EPIC_COLUMNS)[number];

/** Map an epic's `status:` frontmatter to its board column. `In Progress`/`Active` → Doing,
 *  `Done`/`Closed` → Done; anything else (Proposed, blank, unknown) is not-yet-started → Backlog. */
export function epicColumn(status: string): EpicColumn {
  const s = status.trim().toLowerCase();
  if (s === "done" || s === "closed" || s === "complete") return "Done";
  if (s === "in progress" || s === "active" || s === "doing") return "Doing";
  return "Backlog";
}

/** Group epics into the Epics-view columns, each id-ordered (CPE-9 before CPE-100). */
export function groupEpicsByColumn(epics: Epic[]): Record<EpicColumn, Epic[]> {
  const out = { Backlog: [], Doing: [], Done: [] } as Record<EpicColumn, Epic[]>;
  for (const e of epics) out[epicColumn(e.status)].push(e);
  for (const c of EPIC_COLUMNS) out[c].sort((a, b) => idNum(a.id) - idNum(b.id));
  return out;
}

/** The epics hiding in the archive: `epic`-tagged cards from the dated `Done/**` subfolders (which come
 *  back as {@link Card}s, not epics), mapped to {@link Epic}s for the Done column's archive toggle. */
export function archivedEpics(archived: Card[]): Epic[] {
  return archived
    .filter((c) => c.tags.includes("epic"))
    .map((c) => ({ id: c.id, title: c.title, status: "Done", tags: c.tags }))
    .sort((a, b) => idNum(a.id) - idNum(b.id));
}

/** Filter epics by a free-text query — id, title, or any tag (case-insensitive). Blank → all. */
export function filterEpics(epics: Epic[], query: string): Epic[] {
  const q = query.trim().toLowerCase();
  if (!q) return epics;
  return epics.filter(
    (e) =>
      e.id.toLowerCase().includes(q) ||
      e.title.toLowerCase().includes(q) ||
      e.tags.some((t) => t.toLowerCase().includes(q)),
  );
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

// --- Panel size (CPE-529): a resizable board panel, persisted, clamped to a legible minimum. -------
export const BOARD_MIN_W = 640;
export const BOARD_MIN_H = 420;
const SIZE_KEY = "cpe.board.size";

/** Clamp a requested panel size to [min, viewport]. Pure. */
export function clampBoardSize(w: number, h: number, vw: number, vh: number): { w: number; h: number } {
  const maxW = Math.max(BOARD_MIN_W, vw);
  const maxH = Math.max(BOARD_MIN_H, vh);
  return {
    w: Math.round(Math.min(Math.max(w, BOARD_MIN_W), maxW)),
    h: Math.round(Math.min(Math.max(h, BOARD_MIN_H), maxH)),
  };
}

/** The saved board panel size, or null if none/garbage. */
export function loadBoardSize(): { w: number; h: number } | null {
  try {
    const raw = localStorage.getItem(SIZE_KEY);
    if (!raw) return null;
    const o = JSON.parse(raw);
    return typeof o?.w === "number" && typeof o?.h === "number" ? { w: o.w, h: o.h } : null;
  } catch {
    return null;
  }
}

export function saveBoardSize(w: number, h: number): void {
  try {
    localStorage.setItem(SIZE_KEY, JSON.stringify({ w: Math.round(w), h: Math.round(h) }));
  } catch {
    /* storage unavailable */
  }
}

/** The Done cards to display (CPE-531): the recent (top-level) Done, plus the archived (dated
    subfolder) Done only when `showArchived` is on — id-ordered. Keeps the default board bounded. */
export function doneWithArchived(recent: Card[], archived: Card[], showArchived: boolean): Card[] {
  const list = showArchived ? [...recent, ...archived] : recent;
  return [...list].sort((a, b) => idNum(a.id) - idNum(b.id));
}

/** The task string injected into a dispatched agent session (CPE-522): names the ticket it should
    work, reusing the CPE-313 explorer→console task hand-off. */
export function ticketTask(card: Pick<Card, "id" | "title">): string {
  const title = card.title.trim();
  return title ? `Work on ticket ${card.id}: ${title}` : `Work on ticket ${card.id}.`;
}
