// Pure selection-criteria engine (CPE-780, epic CPE-711). Return the indices of entries matching a
// criteria, reusing the CPE-774 `Condition` model rather than a parallel matcher — so the "Select by…"
// dialog (CPE-782) is a thin wire into the selection. No DOM/IO; unit-tested.

import type { DirEntry } from "./types";
import { matchesCondition, type Condition } from "./colorRules";

/** Indices of entries that satisfy the condition. */
export function selectMatching(entries: DirEntry[], condition: Condition, now: number): number[] {
  const out: number[] = [];
  for (let i = 0; i < entries.length; i++) {
    if (matchesCondition(entries[i], condition, now)) out.push(i);
  }
  return out;
}

/**
 * Invert a selection over a list of `count` items: every index in `[0, count)` that is NOT currently
 * selected, ascending. Out-of-range members of `selected` are ignored. Pure — the "Invert selection" move.
 */
export function invertSelection(count: number, selected: Iterable<number>): number[] {
  const sel = new Set(selected);
  const out: number[] = [];
  for (let i = 0; i < count; i++) {
    if (!sel.has(i)) out.push(i);
  }
  return out;
}

/** Lowercased extension without the dot, or "" for dotfiles / no extension. */
function extOf(name: string): string {
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
}

/**
 * Extend a seed selection to every file sharing an extension with any seed file (the "select same type"
 * move). Directories and out-of-range/extension-less seed items are ignored; the result is sorted ascending.
 */
export function sameExtensionAs(entries: DirEntry[], seed: Iterable<number>): number[] {
  const exts = new Set<string>();
  for (const i of seed) {
    const e = entries[i];
    if (e && !e.is_dir) {
      const x = extOf(e.name);
      if (x) exts.add(x);
    }
  }
  if (exts.size === 0) return [];
  const out: number[] = [];
  for (let i = 0; i < entries.length; i++) {
    const e = entries[i];
    if (!e.is_dir && exts.has(extOf(e.name))) out.push(i);
  }
  return out;
}
