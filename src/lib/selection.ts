/**
 * List selection model: a set of selected indices plus an anchor for range
 * selection. Pure and immutable so it can be unit-tested without a UI.
 *
 * Mirrors Explorer/Finder semantics:
 *   - plain click      -> select only that item, and set the anchor
 *   - Ctrl+click       -> toggle that item, and move the anchor to it
 *   - Shift+click      -> select the contiguous range from the anchor
 *   - Ctrl+Shift+click -> add the range to the existing selection
 */
export interface Selection {
  /** Selected row indices. */
  indices: Set<number>;
  /** Anchor for Shift-range selection; -1 when there is none. */
  anchor: number;
  /** The row that has keyboard focus ("lead"); -1 when there is none. */
  lead: number;
}

export function emptySelection(): Selection {
  return { indices: new Set(), anchor: -1, lead: -1 };
}

export function isSelected(sel: Selection, index: number): boolean {
  return sel.indices.has(index);
}

export function selectedCount(sel: Selection): number {
  return sel.indices.size;
}

/** Indices in ascending order — callers need a stable order to map to entries. */
export function selectedIndices(sel: Selection): number[] {
  return [...sel.indices].sort((a, b) => a - b);
}

function range(a: number, b: number): number[] {
  const lo = Math.min(a, b);
  const hi = Math.max(a, b);
  const out: number[] = [];
  for (let i = lo; i <= hi; i++) out.push(i);
  return out;
}

export interface ClickModifiers {
  ctrl?: boolean;
  shift?: boolean;
}

/** Apply a click at `index` with the given modifiers. */
export function click(
  sel: Selection,
  index: number,
  mods: ClickModifiers = {},
): Selection {
  const { ctrl = false, shift = false } = mods;

  if (shift && sel.anchor >= 0) {
    const span = range(sel.anchor, index);
    // Ctrl+Shift extends the existing selection; plain Shift replaces it.
    const indices = ctrl ? new Set([...sel.indices, ...span]) : new Set(span);
    return { indices, anchor: sel.anchor, lead: index };
  }

  if (ctrl) {
    const indices = new Set(sel.indices);
    if (indices.has(index)) indices.delete(index);
    else indices.add(index);
    return { indices, anchor: index, lead: index };
  }

  return { indices: new Set([index]), anchor: index, lead: index };
}

/** Select exactly one index (used by keyboard arrows and programmatic selects). */
export function selectOnly(index: number): Selection {
  if (index < 0) return emptySelection();
  return { indices: new Set([index]), anchor: index, lead: index };
}

/**
 * Select every row from 0 to `count - 1`.
 *
 * `keepLead` (CPE-1373): when given a valid row (`0 <= keepLead < count`), the lead stays there
 * instead of jumping to the last row. Explorer/Finder don't move the viewport on Ctrl+A — only the
 * default (no `keepLead`, or an out-of-range one) falls back to the old "lead = last row" behaviour,
 * so existing callers are unaffected.
 */
export function selectAll(count: number, keepLead = -1): Selection {
  if (count <= 0) return emptySelection();
  const lead = keepLead >= 0 && keepLead < count ? keepLead : count - 1;
  return {
    indices: new Set(range(0, count - 1)),
    anchor: 0,
    lead,
  };
}

/**
 * Build a selection from an explicit set of row indices. Negative indices are ignored; the lowest
 * becomes the anchor and the highest leads — unless `keepLead` (CPE-1373) names a row, in which case
 * the lead stays there instead (used by bulk selections — Invert, Select-all-of-type — so they don't
 * yank the viewport to the selection's max row; see `selectAll`'s doc for why).
 */
export function selectIndices(indices: number[], keepLead = -1): Selection {
  const clean = indices.filter((i) => i >= 0);
  if (clean.length === 0) return emptySelection();
  // Min/max via a loop, NOT `Math.min(...clean)`: spreading a large array into a call overflows the
  // argument-count limit and throws on big folders (CPE-696) — the case "Invert selection" and
  // "Select all of this type" hit on the large folders CPE-688 targets.
  let anchor = clean[0];
  let lead = clean[0];
  for (const i of clean) {
    if (i < anchor) anchor = i;
    if (i > lead) lead = i;
  }
  if (keepLead >= 0) lead = keepLead;
  return { indices: new Set(clean), anchor, lead };
}

/** Flip the selection across `count` visible rows: every row not currently selected becomes
 *  selected, and vice-versa. `keepLead` (CPE-1373) is forwarded to `selectIndices` to keep the lead
 *  (and scroll position) put instead of jumping to the inverted selection's max row. */
export function invertSelection(sel: Selection, count: number, keepLead = -1): Selection {
  const out: number[] = [];
  for (let i = 0; i < count; i++) {
    if (!sel.indices.has(i)) out.push(i);
  }
  return selectIndices(out, keepLead);
}

/**
 * Move the lead by `delta`, clamped to the list. With shift, extend the range
 * from the anchor; otherwise select only the new lead.
 */
export function moveLead(
  sel: Selection,
  delta: number,
  count: number,
  shift = false,
): Selection {
  if (count === 0) return emptySelection();

  const from = sel.lead < 0 ? (delta > 0 ? -1 : 0) : sel.lead;
  const next = Math.max(0, Math.min(count - 1, from + delta));

  if (shift) {
    const anchor = sel.anchor < 0 ? next : sel.anchor;
    return { indices: new Set(range(anchor, next)), anchor, lead: next };
  }
  return selectOnly(next);
}

/**
 * Pick which of two panes' state a keyboard action should act on (CPE-1370, dual-pane commander
 * mode): pane B only when dual-pane is on AND it's the active pane; single-pane — and pane A active
 * in dual-pane — always resolves to pane A. Generic + pure so the routing decision itself is
 * unit-testable without mounting the app; App.svelte's `activePaneState()` is a thin call to this
 * over its live `selection`/`selectionB` (etc.) bindings.
 */
export function pickActivePane<T>(
  dualPane: boolean,
  activePane: 0 | 1,
  paneA: T,
  paneB: T,
): T {
  return dualPane && activePane === 1 ? paneB : paneA;
}

/** A confirm-gated action's target, frozen at the moment the confirm dialog opens. */
export interface ConfirmTarget {
  /** Which pane the paths below came from — must be replayed as-is, never re-derived, once captured. */
  inPaneB: boolean;
  paths: string[];
}

/**
 * Snapshot which pane + paths a confirm-gated destructive action (Delete, …) targets, at the moment
 * the confirmation is asked for — CPE-1370 review (data-loss fix): a confirm dialog can stay open for
 * an arbitrary amount of time, during which `activePane` can change (e.g. Tab), so the code that
 * actually performs the action must NOT re-derive its target from live state once the dialog is up —
 * it would silently act on whatever pane happens to be active when the user clicks "confirm", not the
 * one they were shown and agreed to. Pure + returns a fresh array (`.map`, not the source reference) so
 * the result can't be mutated out from under the caller by a later change to `selectedEntries` either.
 * Unit-testable in isolation, mirroring `pickActivePane`.
 */
export function snapshotConfirmTarget(
  inPaneB: boolean,
  selectedEntries: { path: string }[],
): ConfirmTarget {
  return { inPaneB, paths: selectedEntries.map((e) => e.path) };
}

/**
 * Remap a selection after the listing changes (sort, filter, refresh). Indices
 * are meaningless across a re-order, so we re-derive them from the paths that
 * were selected. Anything that vanished is simply dropped.
 */
export function remapByPath(
  previouslySelectedPaths: string[],
  newOrder: { path: string }[],
): Selection {
  const wanted = new Set(previouslySelectedPaths);
  const indices = new Set<number>();
  newOrder.forEach((e, i) => {
    if (wanted.has(e.path)) indices.add(i);
  });
  if (indices.size === 0) return emptySelection();
  const sorted = [...indices].sort((a, b) => a - b);
  return { indices, anchor: sorted[0], lead: sorted[sorted.length - 1] };
}
