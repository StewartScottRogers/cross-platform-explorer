/**
 * Local persistence for user preferences, pinned folders, and recent files.
 *
 * This is private data: it is stored locally in the app's own storage and is
 * never sent anywhere. Every read is defensive — a corrupt or hand-edited value
 * must degrade to the default rather than crash the app on launch.
 */
import type { ViewMode, SortKey, SortDir, RecentFile } from "./types";

const KEYS = {
  view: "cpe.view",
  showHidden: "cpe.showHidden",
  sortKey: "cpe.sortKey",
  sortDir: "cpe.sortDir",
  showDetails: "cpe.showDetails",
  showPreview: "cpe.showPreview",
  pins: "cpe.pins",
  recents: "cpe.recents",
} as const;

const MAX_RECENTS = 20;

function read<T>(key: string, fallback: T, validate: (v: unknown) => boolean): T {
  try {
    const raw = localStorage.getItem(key);
    if (raw === null) return fallback;
    const parsed = JSON.parse(raw);
    return validate(parsed) ? (parsed as T) : fallback;
  } catch {
    return fallback;
  }
}

function write(key: string, value: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // Storage can be full or unavailable. A failed preference save must never
    // break the app, so this is deliberately swallowed.
  }
}

const isView = (v: unknown): v is ViewMode =>
  v === "details" || v === "list" || v === "icons";
const isSortKey = (v: unknown): v is SortKey =>
  v === "name" || v === "modified" || v === "type" || v === "size";
const isSortDir = (v: unknown): v is SortDir => v === "asc" || v === "desc";
const isBool = (v: unknown): v is boolean => typeof v === "boolean";
const isStringArray = (v: unknown): v is string[] =>
  Array.isArray(v) && v.every((x) => typeof x === "string");
const isRecentArray = (v: unknown): v is RecentFile[] =>
  Array.isArray(v) &&
  v.every(
    (x) =>
      x &&
      typeof x === "object" &&
      typeof (x as RecentFile).path === "string" &&
      typeof (x as RecentFile).name === "string" &&
      typeof (x as RecentFile).opened === "number",
  );

export const loadView = (): ViewMode => read(KEYS.view, "details", isView);
export const saveView = (v: ViewMode) => write(KEYS.view, v);

export const loadShowHidden = (): boolean =>
  read(KEYS.showHidden, false, isBool);
export const saveShowHidden = (v: boolean) => write(KEYS.showHidden, v);

export const loadSortKey = (): SortKey => read(KEYS.sortKey, "name", isSortKey);
export const saveSortKey = (v: SortKey) => write(KEYS.sortKey, v);

export const loadSortDir = (): SortDir => read(KEYS.sortDir, "asc", isSortDir);
export const saveSortDir = (v: SortDir) => write(KEYS.sortDir, v);

export const loadShowDetails = (): boolean =>
  read(KEYS.showDetails, true, isBool);
export const saveShowDetails = (v: boolean) => write(KEYS.showDetails, v);

export const loadShowPreview = (): boolean =>
  read(KEYS.showPreview, true, isBool);
export const saveShowPreview = (v: boolean) => write(KEYS.showPreview, v);

export const loadPins = (): string[] => read(KEYS.pins, [], isStringArray);
export const savePins = (v: string[]) => write(KEYS.pins, v);

export const loadRecents = (): RecentFile[] =>
  read(KEYS.recents, [], isRecentArray);
export const saveRecents = (v: RecentFile[]) => write(KEYS.recents, v);

/**
 * Add a file to the recent list: most recent first, de-duplicated by path,
 * capped so it cannot grow without bound.
 */
export function addRecent(
  list: RecentFile[],
  entry: { path: string; name: string },
  now: number = Date.now(),
): RecentFile[] {
  const without = list.filter((r) => r.path !== entry.path);
  return [{ path: entry.path, name: entry.name, opened: now }, ...without].slice(
    0,
    MAX_RECENTS,
  );
}

/** Toggle a folder's pinned state. */
export function togglePin(pins: string[], path: string): string[] {
  return pins.includes(path)
    ? pins.filter((p) => p !== path)
    : [...pins, path];
}
