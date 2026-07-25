// Pure workspace / layout-session model (CPE-787, epic CPE-708). A named set of tabs (path + view/sort/
// filter) with tolerant parse/serialize, immutable CRUD, and a restore-time prune of moved/missing paths.
// No DOM/IO — unit-tested — so the switcher (CPE-788) and auto-restore (CPE-789) are thin. Mirrors the
// `smartFolders.ts` list-store shape.

import type { ViewMode, SortKey, SortDir } from "./types";

/** One tab captured in a workspace. */
export interface WorkspaceTab {
  path: string;
  view?: ViewMode;
  sortKey?: SortKey;
  sortDir?: SortDir;
  filter?: string;
}

/** A named window state. */
export interface Workspace {
  id: string;
  name: string;
  tabs: WorkspaceTab[];
}

function newId(): string {
  return `ws_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 7)}`;
}

function sanitizeTab(t: unknown): WorkspaceTab | null {
  if (!t || typeof t !== "object") return null;
  const o = t as Record<string, unknown>;
  if (typeof o.path !== "string" || o.path === "") return null;
  const tab: WorkspaceTab = { path: o.path };
  if (typeof o.view === "string") tab.view = o.view as ViewMode;
  if (typeof o.sortKey === "string") tab.sortKey = o.sortKey as SortKey;
  if (typeof o.sortDir === "string") tab.sortDir = o.sortDir as SortDir;
  if (typeof o.filter === "string") tab.filter = o.filter;
  return tab;
}

/** Parse persisted JSON into a workspace list. Tolerant: bad JSON / wrong shape → `[]`, invalid entries
    (and invalid tabs within them) are dropped. */
export function parseWorkspaces(json: string | null): Workspace[] {
  if (!json) return [];
  let raw: unknown;
  try {
    raw = JSON.parse(json);
  } catch {
    return [];
  }
  if (!Array.isArray(raw)) return [];
  const out: Workspace[] = [];
  for (const item of raw) {
    if (!item || typeof item !== "object") continue;
    const o = item as Record<string, unknown>;
    if (typeof o.id !== "string" || typeof o.name !== "string" || !Array.isArray(o.tabs)) continue;
    const tabs = o.tabs.map(sanitizeTab).filter((t): t is WorkspaceTab => t !== null);
    out.push({ id: o.id, name: o.name, tabs });
  }
  return out;
}

export function serializeWorkspaces(list: Workspace[]): string {
  return JSON.stringify(list);
}

export function addWorkspace(list: Workspace[], name: string, tabs: WorkspaceTab[]): Workspace[] {
  return [...list, { id: newId(), name, tabs }];
}

export function renameWorkspace(list: Workspace[], id: string, name: string): Workspace[] {
  return list.map((w) => (w.id === id ? { ...w, name } : w));
}

export function removeWorkspace(list: Workspace[], id: string): Workspace[] {
  return list.filter((w) => w.id !== id);
}

/** Replace a workspace's captured tabs (re-capture the current window state). */
export function updateWorkspace(list: Workspace[], id: string, tabs: WorkspaceTab[]): Workspace[] {
  return list.map((w) => (w.id === id ? { ...w, tabs } : w));
}

/** A copy of the workspace with only tabs whose path still exists — graceful restore of moved/absent paths. */
export function pruneMissing(ws: Workspace, exists: (path: string) => boolean): Workspace {
  return { ...ws, tabs: ws.tabs.filter((t) => exists(t.path)) };
}
