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

export function selectAll(count: number): Selection {
  if (count <= 0) return emptySelection();
  return {
    indices: new Set(range(0, count - 1)),
    anchor: 0,
    lead: count - 1,
  };
}

/** Build a selection from an explicit set of row indices. Negative indices are
 *  ignored; the lowest becomes the anchor and the highest leads. */
export function selectIndices(indices: number[]): Selection {
  const clean = indices.filter((i) => i >= 0);
  if (clean.length === 0) return emptySelection();
  return {
    indices: new Set(clean),
    anchor: Math.min(...clean),
    lead: Math.max(...clean),
  };
}

/** Flip the selection across `count` visible rows: every row not currently
 *  selected becomes selected, and vice-versa. */
export function invertSelection(sel: Selection, count: number): Selection {
  const out: number[] = [];
  for (let i = 0; i < count; i++) {
    if (!sel.indices.has(i)) out.push(i);
  }
  return selectIndices(out);
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
