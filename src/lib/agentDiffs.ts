import { writable, type Readable } from "svelte/store";
import { listen } from "@tauri-apps/api/event";
import { inlineDiff, type InlineSeg } from "./diff";

/**
 * Per-path before/after store for Agent Watch "Edit Diff Peek" (CPE-744, epic CPE-727).
 *
 * The host emits `ai-console://fs-diff` records — `{path, before, after}` — for each agent write it
 * shadowed (CPE-743; `before` is "" for a newly-created file). Here we fold them into a bounded,
 * newest-first, per-path store the touched rows + timeline (CPE-745/746) look up to show what changed.
 * The diff itself is computed on demand via `diff.ts` (`inlineDiff`) — the host stays dumb and ships
 * only the two strings. Strictly additive + idle-by-default: the store is empty and no listener runs
 * until watching starts, and it clears on stop (AGENT-WATCH.md: "off means off").
 */

/** One before/after record for a file the agent wrote. `before` is "" for a created file (all-added). */
export interface FsDiff {
  path: string;
  before: string;
  after: string;
}

/** How many per-file diffs to retain (newest-first). Bounded so a big refactor can't grow the store. */
export const DIFF_CAP = 200;
/** Total character budget across retained before+after content; oldest entries evict when exceeded. */
export const DIFF_CHAR_CAP = 4_000_000;

/** The store state: the latest diff per path, plus newest-first order + a running char total for
 *  bounded eviction. */
export interface DiffState {
  byPath: Record<string, FsDiff>;
  order: string[];
  chars: number;
}

/** A fresh, empty state (the idle/off value). */
export const emptyDiffState = (): DiffState => ({ byPath: {}, order: [], chars: 0 });

const cost = (d: FsDiff): number => d.before.length + d.after.length;

/**
 * Fold a batch of diffs into the previous state (pure). Latest-per-path wins and moves to the front;
 * the store is then evicted from the oldest end until within both the count cap and the char cap.
 */
export function foldDiffs(
  prev: DiffState,
  items: FsDiff[],
  cap = DIFF_CAP,
  charCap = DIFF_CHAR_CAP,
): DiffState {
  const byPath = { ...prev.byPath };
  let order = prev.order.slice();
  let chars = prev.chars;
  for (const it of items) {
    const existing = byPath[it.path];
    if (existing) {
      chars -= cost(existing);
      order = order.filter((p) => p !== it.path);
    }
    byPath[it.path] = it;
    chars += cost(it);
    order.unshift(it.path);
  }
  while (order.length > cap || chars > charCap) {
    const victim = order.pop();
    if (victim === undefined) break;
    const d = byPath[victim];
    if (d) {
      chars -= cost(d);
      delete byPath[victim];
    }
  }
  return { byPath, order, chars };
}

/** Normalize an `ai-console://fs-diff` payload into a clean, typed list, dropping anything malformed.
 *  Pure so the host→UI wire format is unit-testable headlessly (sibling of `normalizeFsActivity`). */
export function normalizeFsDiff(payload: unknown): FsDiff[] {
  if (!Array.isArray(payload)) return [];
  const out: FsDiff[] = [];
  for (const item of payload) {
    const path = (item as { path?: unknown })?.path;
    const before = (item as { before?: unknown })?.before;
    const after = (item as { after?: unknown })?.after;
    if (
      typeof path === "string" &&
      path &&
      typeof before === "string" &&
      typeof after === "string"
    ) {
      out.push({ path, before, after });
    }
  }
  return out;
}

/** The latest before/after for a path, or `null` if none is recorded. */
export function diffFor(state: DiffState, path: string): FsDiff | null {
  return state.byPath[path] ?? null;
}

/** Intra-content diff segments (old/new) for a path's latest write via `diff.ts`, or `null` if none. */
export function diffSegs(
  state: DiffState,
  path: string,
): { old: InlineSeg[]; new: InlineSeg[] } | null {
  const d = state.byPath[path];
  return d ? inlineDiff(d.before, d.after) : null;
}

/** One rendered line of a compact diff peek: unchanged context, an added line, or a removed line. */
export interface DiffRow {
  kind: "context" | "add" | "del";
  text: string;
}

/**
 * A compact line-level diff of `before` → `after` for the peek (CPE-745). Cheap + deterministic —
 * a common-prefix / common-suffix **line** scan (not a full LCS), matching `diff.ts`'s philosophy:
 * good enough for the localized edits an agent typically makes. The differing middle is shown as
 * removed lines then added lines, wrapped in up to `context` unchanged lines each side. A `created`
 * file (empty `before`) renders as all-added.
 */
export function compactLineDiff(before: string, after: string, context = 3): DiffRow[] {
  const a = before.length ? before.split("\n") : [];
  const b = after.length ? after.split("\n") : [];
  let start = 0;
  while (start < a.length && start < b.length && a[start] === b[start]) start++;
  let endA = a.length;
  let endB = b.length;
  while (endA > start && endB > start && a[endA - 1] === b[endB - 1]) {
    endA--;
    endB--;
  }
  const rows: DiffRow[] = [];
  for (let i = Math.max(0, start - context); i < start; i++) rows.push({ kind: "context", text: a[i] });
  for (let i = start; i < endA; i++) rows.push({ kind: "del", text: a[i] });
  for (let i = start; i < endB; i++) rows.push({ kind: "add", text: b[i] });
  for (let i = endA; i < Math.min(a.length, endA + context); i++)
    rows.push({ kind: "context", text: a[i] });
  return rows;
}

/** One aligned row of a side-by-side diff (CPE-746): `left`/`right` are line text or `null` for a
 *  blank cell (a line present on only one side); `changed` marks the differing middle. */
export interface SideRow {
  left: string | null;
  right: string | null;
  changed: boolean;
}

/**
 * A side-by-side (before | after) alignment of `before` → `after` for the full diff view (CPE-746).
 * Same cheap prefix/suffix **line** scan as {@link compactLineDiff}: common context lines align on
 * both sides; the differing middle pairs removed with added line-by-line (extra removals get a blank
 * right cell, extra additions a blank left cell). `context` unchanged lines are kept either side.
 */
export function sideBySideRows(before: string, after: string, context = 3): SideRow[] {
  const a = before.length ? before.split("\n") : [];
  const b = after.length ? after.split("\n") : [];
  let start = 0;
  while (start < a.length && start < b.length && a[start] === b[start]) start++;
  let endA = a.length;
  let endB = b.length;
  while (endA > start && endB > start && a[endA - 1] === b[endB - 1]) {
    endA--;
    endB--;
  }
  const rows: SideRow[] = [];
  for (let i = Math.max(0, start - context); i < start; i++)
    rows.push({ left: a[i], right: a[i], changed: false });
  const delN = endA - start;
  const addN = endB - start;
  const pair = Math.min(delN, addN);
  for (let i = 0; i < pair; i++)
    rows.push({ left: a[start + i], right: b[start + i], changed: true });
  for (let i = pair; i < delN; i++) rows.push({ left: a[start + i], right: null, changed: true });
  for (let i = pair; i < addN; i++) rows.push({ left: null, right: b[start + i], changed: true });
  for (let i = endA; i < Math.min(a.length, endA + context); i++)
    rows.push({ left: a[i], right: a[i], changed: false });
  return rows;
}

/** Added/removed line counts for a path's latest write (for a compact "+a −d" summary), or `null`. */
export function diffLineStats(state: DiffState, path: string): { add: number; del: number } | null {
  const d = state.byPath[path];
  if (!d) return null;
  let add = 0;
  let del = 0;
  for (const r of compactLineDiff(d.before, d.after, 0)) {
    if (r.kind === "add") add++;
    else if (r.kind === "del") del++;
  }
  return { add, del };
}

const store = writable<DiffState>(emptyDiffState());

/** Reactive per-path diff state (empty when not watching). */
export const agentDiffs: Readable<DiffState> = store;

/** Fold a raw `ai-console://fs-diff` payload into the store (exposed for headless tests). */
export function ingestDiff(payload: unknown): void {
  const items = normalizeFsDiff(payload);
  if (items.length) store.update((prev) => foldDiffs(prev, items));
}

/** Test/introspection helper: the current diff state synchronously. */
export function currentDiffs(): DiffState {
  let snapshot = emptyDiffState();
  store.subscribe((v) => (snapshot = v))();
  return snapshot;
}

/** Clear the diff store (on stop-watching), so a stale session never bleeds into a new one. */
export function clearDiffs(): void {
  store.set(emptyDiffState());
}

/**
 * Start consuming `ai-console://fs-diff` events. Returns a teardown that unlistens and clears the
 * store. Call when watching begins; call the teardown when it ends (mirrors `initAgentActivity`).
 */
export async function initAgentDiffs(): Promise<() => void> {
  const unlisten = await listen("ai-console://fs-diff", (e) => ingestDiff(e.payload));
  return () => {
    unlisten();
    clearDiffs();
  };
}
