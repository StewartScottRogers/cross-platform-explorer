// Sidebar section collapse state (CPE-675, epic CPE-660): one persisted store for whether each sidebar
// group (Explore / Quick access / Drives / Favorites / Agents / Tags / Smart) is expanded, so a layout
// the user sets sticks across restarts. Keyed by a section id; an unset section defaults to open, so a
// fresh install is fully expanded (zero behavioural change). Pure reducers below are unit-tested; the
// store tail persists via the shared localStorage layer.

import { writable } from "svelte/store";
import { lsGet, lsSet } from "./persist";

const KEY = "cpe.sidebarSections";

/** Parse the persisted JSON into a clean id→open map, dropping anything non-boolean. Pure. */
export function parseSections(json: string | null): Record<string, boolean> {
  if (!json) return {};
  try {
    const v = JSON.parse(json);
    if (!v || typeof v !== "object" || Array.isArray(v)) return {};
    const out: Record<string, boolean> = {};
    for (const [k, val] of Object.entries(v)) if (typeof val === "boolean") out[k] = val;
    return out;
  } catch {
    return {};
  }
}

/** Whether section `id` is open — default true (unset = expanded). Pure. */
export function isOpen(state: Record<string, boolean>, id: string): boolean {
  return state[id] !== false;
}

/** Flip section `id`'s open state (unset → collapsed). Pure — returns a new map. */
export function toggled(state: Record<string, boolean>, id: string): Record<string, boolean> {
  return { ...state, [id]: state[id] === false ? true : false };
}

const store = writable<Record<string, boolean>>(parseSections(lsGet(KEY)));
store.subscribe((s) => lsSet(KEY, JSON.stringify(s)));

/** Reactive per-section open state (persisted). */
export const sidebarSections = store;

/** Toggle a section's open/collapsed state. */
export function toggleSection(id: string): void {
  store.update((s) => toggled(s, id));
}
