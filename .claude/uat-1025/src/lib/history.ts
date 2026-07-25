/**
 * Browser-style navigation history: a list of visited paths plus a cursor.
 * Pure and immutable so it can be unit-tested without a UI.
 */
export interface History {
  entries: string[];
  index: number;
}

export function createHistory(initial?: string): History {
  return initial ? { entries: [initial], index: 0 } : { entries: [], index: -1 };
}

/**
 * Visit a new path. This truncates any forward history — the standard
 * behaviour: going back and then somewhere new discards the old forward path.
 * Re-visiting the current path is a no-op, so refresh doesn't pile up entries.
 */
export function visit(h: History, path: string): History {
  if (h.index >= 0 && h.entries[h.index] === path) return h;
  const entries = [...h.entries.slice(0, h.index + 1), path];
  return { entries, index: entries.length - 1 };
}

export function canGoBack(h: History): boolean {
  return h.index > 0;
}

export function canGoForward(h: History): boolean {
  return h.index >= 0 && h.index < h.entries.length - 1;
}

export function back(h: History): History {
  return canGoBack(h) ? { ...h, index: h.index - 1 } : h;
}

export function forward(h: History): History {
  return canGoForward(h) ? { ...h, index: h.index + 1 } : h;
}

export function current(h: History): string | null {
  return h.index >= 0 ? h.entries[h.index] : null;
}

/**
 * Distinct visited paths for a "recent locations" list — most-recently-visited first, with the
 * current path excluded and duplicates collapsed, capped at `max`. Pure; drives the command
 * palette's recent-folder jump entries.
 */
export function recentPaths(h: History, max = 8): string[] {
  const cur = current(h);
  const seen = new Set<string>();
  const out: string[] = [];
  for (let i = h.entries.length - 1; i >= 0 && out.length < max; i--) {
    const p = h.entries[i];
    if (p === cur || seen.has(p)) continue;
    seen.add(p);
    out.push(p);
  }
  return out;
}
