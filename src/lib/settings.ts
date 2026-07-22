/**
 * User preferences, pinned folders, and recent files — persisted to a SINGLE
 * on-disk settings file (`settings.json` in the app config dir), read/written
 * through the read_settings / write_settings backend commands (CPE-226).
 *
 * The file is the source of truth. At startup `initSettings()` loads it into an
 * in-memory object; the synchronous load/save helpers below read and mutate
 * that object, and every save debounces a write back to the file. On first run
 * the older per-key localStorage values are migrated in, so nothing is lost.
 *
 * This is private data: stored locally, never sent anywhere. Every read is
 * defensive — a corrupt or hand-edited value degrades to its default rather than
 * crashing the app on launch.
 */
import { invoke } from "./invoke";
import type { ViewMode, SortKey, SortDir, RecentFile, Favorite } from "./types";
import { COLUMN_DEFAULTS } from "./columns";
import { parseRules, serializeRules } from "./colorRulesStore";
import type { ColorRule } from "./colorRules";
import { parseManifest } from "./integrity";
import type { ChecksumEntry } from "./integrity";
import { parseRules as parseWatchRules, serializeRules as serializeWatchRules } from "./watchRules";
import type { WatchRule } from "./watchRules";
import { parseWorkspaces, serializeWorkspaces } from "./workspaces";
import type { Workspace, WorkspaceTab } from "./workspaces";
import { parseJobs, serializeJobs } from "./backup";
import type { BackupJob } from "./backup";

export const KEYS = {
  view: "cpe.view",
  showHidden: "cpe.showHidden",
  showFolderSizes: "cpe.showFolderSizes",
  foldersFirst: "cpe.foldersFirst",
  sortKey: "cpe.sortKey",
  sortDir: "cpe.sortDir",
  showDetails: "cpe.showDetails",
  showPreview: "cpe.showPreview",
  sidebarWidth: "cpe.sidebarWidth",
  rightWidth: "cpe.rightWidth",
  pins: "cpe.pins",
  recents: "cpe.recents",
  favorites: "cpe.favorites",
  recentFolders: "cpe.recentFolders",
  columnWidths: "cpe.columnWidths",
  diagnostics: "cpe.diagnostics",
  verifyOnStartup: "cpe.verifyOnStartup",
  colorRules: "cpe.colorRules",
  integrityBaselines: "cpe.integrityBaselines",
  watchRules: "cpe.watchRules",
  workspaces: "cpe.workspaces",
  backupJobs: "cpe.backupJobs",
  watchedFolders: "cpe.watchedFolders",
  backupHistory: "cpe.backupHistory",
  autoRestore: "cpe.autoRestore",
  lastSession: "cpe.lastSession",
} as const;

const MAX_RECENTS = 20;

/** In-memory settings document, mirrored to settings.json. */
let state: Record<string, unknown> = {};

// ---- persistence -----------------------------------------------------------

let persistTimer: ReturnType<typeof setTimeout> | undefined;

/** Debounced write of the whole settings document to the backend file. */
function schedulePersist(): void {
  if (persistTimer) clearTimeout(persistTimer);
  persistTimer = setTimeout(() => {
    void invoke("write_settings", { contents: JSON.stringify(state) }).catch(() => {
      // A failed settings save must never break the app.
    });
  }, 150);
}

/**
 * Merge legacy per-key localStorage values into a settings object read from the
 * file. The file wins; only keys it lacks are backfilled from localStorage.
 * Pure and injectable so it can be unit-tested without a real localStorage.
 */
export function mergeLegacy(
  fileObj: Record<string, unknown>,
  get: (k: string) => string | null,
): Record<string, unknown> {
  const merged = { ...fileObj };
  for (const key of Object.values(KEYS)) {
    if (key in merged) continue;
    const raw = get(key);
    if (raw === null) continue;
    try {
      merged[key] = JSON.parse(raw);
    } catch {
      // ignore an unparseable legacy value
    }
  }
  return merged;
}

/**
 * Load settings.json into memory, migrating any legacy localStorage prefs, then
 * persist the merged result. Call once at startup before the UI reads settings.
 */
export async function initSettings(): Promise<void> {
  let fileObj: Record<string, unknown> = {};
  try {
    const raw = await invoke<string>("read_settings");
    const parsed = JSON.parse(raw);
    if (parsed && typeof parsed === "object") fileObj = parsed as Record<string, unknown>;
  } catch {
    // absent/corrupt/no-backend → start from defaults
  }

  const get = (k: string): string | null => {
    try {
      return typeof localStorage !== "undefined" ? localStorage.getItem(k) : null;
    } catch {
      return null;
    }
  };
  state = mergeLegacy(fileObj, get);

  // Persist the merged document so the migration is captured on disk.
  schedulePersist();
}

// ---- typed accessors -------------------------------------------------------

function read<T>(key: string, fallback: T, validate: (v: unknown) => boolean): T {
  const v = state[key];
  return v !== undefined && validate(v) ? (v as T) : fallback;
}

function write(key: string, value: unknown): void {
  state[key] = value;
  schedulePersist();
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
const isFavoriteArray = (v: unknown): v is Favorite[] =>
  Array.isArray(v) &&
  v.every(
    (x) =>
      x &&
      typeof x === "object" &&
      typeof (x as Favorite).path === "string" &&
      typeof (x as Favorite).name === "string" &&
      typeof (x as Favorite).is_dir === "boolean",
  );

export const loadView = (): ViewMode => read(KEYS.view, "details", isView);
export const saveView = (v: ViewMode) => write(KEYS.view, v);

export const loadShowHidden = (): boolean => read(KEYS.showHidden, false, isBool);
export const saveShowHidden = (v: boolean) => write(KEYS.showHidden, v);

// Recursive folder-size column (CPE-750): opt-in, off by default so a plain listing pulls no dir_size
// walks. When on, the details Size column fills each folder's computed subtree size lazily.
export const loadShowFolderSizes = (): boolean => read(KEYS.showFolderSizes, false, isBool);
export const saveShowFolderSizes = (v: boolean) => write(KEYS.showFolderSizes, v);

// Diagnostics mode (CPE-758): on-screen timing of every backend/OS call. Off by default; user-toggled
// from the Application menu. Set `localStorage["cpe.diagnostics"] = "true"` to force it on from a console.
export const loadDiagnostics = (): boolean => read(KEYS.diagnostics, false, isBool);
export const saveDiagnostics = (v: boolean) => write(KEYS.diagnostics, v);

// Integrity: opt-in auto-verify of all baselined folders on app startup (CPE-872). Off by default
// (fast-when-off / no background scanning unless the user asks). When on, the app verifies once at launch.
export const loadVerifyOnStartup = (): boolean => read(KEYS.verifyOnStartup, false, isBool);
export const saveVerifyOnStartup = (v: boolean) => write(KEYS.verifyOnStartup, v);

export const loadFoldersFirst = (): boolean => read(KEYS.foldersFirst, true, isBool);
export const saveFoldersFirst = (v: boolean) => write(KEYS.foldersFirst, v);

export const loadSortKey = (): SortKey => read(KEYS.sortKey, "name", isSortKey);
export const saveSortKey = (v: SortKey) => write(KEYS.sortKey, v);

export const loadSortDir = (): SortDir => read(KEYS.sortDir, "asc", isSortDir);
export const saveSortDir = (v: SortDir) => write(KEYS.sortDir, v);

export const loadShowDetails = (): boolean => read(KEYS.showDetails, true, isBool);
export const saveShowDetails = (v: boolean) => write(KEYS.showDetails, v);

export const loadShowPreview = (): boolean => read(KEYS.showPreview, true, isBool);
export const saveShowPreview = (v: boolean) => write(KEYS.showPreview, v);

const isPosNum = (v: unknown): v is number =>
  typeof v === "number" && Number.isFinite(v) && v > 0;
export const loadSidebarWidth = (): number => read(KEYS.sidebarWidth, 220, isPosNum);
export const saveSidebarWidth = (v: number) => write(KEYS.sidebarWidth, v);
export const loadRightWidth = (): number => read(KEYS.rightWidth, 300, isPosNum);
export const saveRightWidth = (v: number) => write(KEYS.rightWidth, v);

export const loadPins = (): string[] => read(KEYS.pins, [], isStringArray);
export const savePins = (v: string[]) => write(KEYS.pins, v);

export const loadRecents = (): RecentFile[] => read(KEYS.recents, [], isRecentArray);
export const saveRecents = (v: RecentFile[]) => write(KEYS.recents, v);

export const loadFavorites = (): Favorite[] => read(KEYS.favorites, [], isFavoriteArray);
export const saveFavorites = (v: Favorite[]) => write(KEYS.favorites, v);

export const loadRecentFolders = (): RecentFile[] => read(KEYS.recentFolders, [], isRecentArray);
export const saveRecentFolders = (v: RecentFile[]) => write(KEYS.recentFolders, v);

// Details-view column widths (CPE-350): exactly four positive numbers (Name/Date/Type/Size).
const isColumnWidths = (v: unknown): v is number[] =>
  Array.isArray(v) && v.length === COLUMN_DEFAULTS.length &&
  v.every((x) => typeof x === "number" && Number.isFinite(x) && x > 0);
export const loadColumnWidths = (): number[] =>
  read(KEYS.columnWidths, COLUMN_DEFAULTS.slice(), isColumnWidths);
export const saveColumnWidths = (v: number[]) => write(KEYS.columnWidths, v);

// Rule-based file coloring & labels (CPE-776, epic CPE-709). Stored as the raw `ColorRule[]`; loaded
// through the tolerant `parseRules` so a corrupt/hand-edited value degrades to `[]` rather than crashing.
export const loadColorRules = (): ColorRule[] => {
  const v = state[KEYS.colorRules];
  return v === undefined ? [] : parseRules(serializeRules(v as ColorRule[]));
};
export const saveColorRules = (v: ColorRule[]): void => write(KEYS.colorRules, v);

// Integrity baselines (CPE-792, epic CPE-737): a per-folder checksum manifest, keyed by folder path.
// Each manifest is loaded through the tolerant `parseManifest` so a corrupt entry degrades to a shorter
// (or empty) baseline rather than crashing. Opt-in — only folders the user baselines appear here.
export const loadIntegrityBaselines = (): Record<string, ChecksumEntry[]> => {
  const v = state[KEYS.integrityBaselines];
  if (!v || typeof v !== "object") return {};
  const out: Record<string, ChecksumEntry[]> = {};
  for (const [path, entries] of Object.entries(v as Record<string, unknown>)) {
    out[path] = parseManifest(JSON.stringify(entries));
  }
  return out;
};
export const saveIntegrityBaselines = (m: Record<string, ChecksumEntry[]>): void =>
  write(KEYS.integrityBaselines, m);

// Watched-folder rules (CPE-795, epic CPE-734): the ordered rule set, loaded through the tolerant
// watchRules parser so a corrupt entry degrades rather than crashing. Opt-in — empty until the user adds one.
export const loadWatchRules = (): WatchRule[] => {
  const v = state[KEYS.watchRules];
  return v === undefined ? [] : parseWatchRules(serializeWatchRules(v as WatchRule[]));
};
export const saveWatchRules = (v: WatchRule[]): void => write(KEYS.watchRules, v);

// Saved workspaces (CPE-788, epic CPE-708): named window states (tab paths + view/sort/filter), loaded
// through the tolerant `parseWorkspaces` so a corrupt entry degrades rather than crashing.
export const loadWorkspaces = (): Workspace[] => {
  const v = state[KEYS.workspaces];
  return v === undefined ? [] : parseWorkspaces(serializeWorkspaces(v as Workspace[]));
};
export const saveWorkspaces = (v: Workspace[]): void => write(KEYS.workspaces, v);

// Launch-time auto-restore (CPE-789, epic CPE-708): opt-in flag + the last window session (the tabs open
// at last save). Off by default so single-tab startup is unchanged. `lastSession` is parsed through the
// tolerant workspace tab sanitiser (wrapping it in a throwaway workspace) so a corrupt entry degrades to [].
export const loadAutoRestore = (): boolean => read(KEYS.autoRestore, false, isBool);
export const saveAutoRestore = (v: boolean): void => write(KEYS.autoRestore, v);
export const loadLastSession = (): WorkspaceTab[] => {
  const v = state[KEYS.lastSession];
  if (v === undefined) return [];
  return parseWorkspaces(serializeWorkspaces([{ id: "s", name: "s", tabs: v as WorkspaceTab[] }]))[0]?.tabs ?? [];
};
export const saveLastSession = (tabs: WorkspaceTab[]): void => write(KEYS.lastSession, tabs);

// Backup jobs (CPE-798, epic CPE-736): source→dest definitions, loaded through the tolerant `parseJobs`.
export const loadBackupJobs = (): BackupJob[] => {
  const v = state[KEYS.backupJobs];
  return v === undefined ? [] : parseJobs(serializeJobs(v as BackupJob[]));
};
export const saveBackupJobs = (v: BackupJob[]): void => write(KEYS.backupJobs, v);

// Per-job backup run history (CPE-798): a small ring of recent runs keyed by job id. Loosely validated —
// a corrupt shape degrades to {}. Capped by the writer, not here.
export interface BackupRunRecord { when: number; ok: number; failed: number; label: string; }
export const loadBackupHistory = (): Record<string, BackupRunRecord[]> => {
  const v = state[KEYS.backupHistory];
  if (!v || typeof v !== "object") return {};
  const out: Record<string, BackupRunRecord[]> = {};
  for (const [id, runs] of Object.entries(v as Record<string, unknown>)) {
    if (Array.isArray(runs)) {
      out[id] = runs.filter(
        (r): r is BackupRunRecord =>
          !!r && typeof r === "object" &&
          typeof (r as BackupRunRecord).when === "number" &&
          typeof (r as BackupRunRecord).ok === "number" &&
          typeof (r as BackupRunRecord).failed === "number" &&
          typeof (r as BackupRunRecord).label === "string",
      );
    }
  }
  return out;
};
export const saveBackupHistory = (m: Record<string, BackupRunRecord[]>): void => write(KEYS.backupHistory, m);

// Watched folders for live watch-rules (CPE-794): the folders the live watcher monitors. Opt-in — empty
// means nothing is watched. Stored as a plain string[]; degrades to [] if corrupt.
export const loadWatchedFolders = (): string[] => read(KEYS.watchedFolders, [], isStringArray);
export const saveWatchedFolders = (v: string[]): void => write(KEYS.watchedFolders, v);

/** Reset every stored preference to its default (used by the app Settings gear). */
export function resetSettings(): void {
  state = {};
  schedulePersist();
}

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

/** Drop a single entry from the recent list by path (leaves the rest in order). */
export function removeRecent(list: RecentFile[], path: string): RecentFile[] {
  return list.filter((r) => r.path !== path);
}

/** Toggle a folder's pinned state. */
export function togglePin(pins: string[], path: string): string[] {
  return pins.includes(path)
    ? pins.filter((p) => p !== path)
    : [...pins, path];
}

/**
 * Toggle an item's favorite state, keyed by path. Removing wins when present;
 * otherwise the entry (file or folder) is appended, newest last.
 */
export function toggleFavorite(
  favorites: Favorite[],
  entry: { path: string; name: string; is_dir: boolean },
): Favorite[] {
  return favorites.some((f) => f.path === entry.path)
    ? favorites.filter((f) => f.path !== entry.path)
    : [...favorites, { path: entry.path, name: entry.name, is_dir: entry.is_dir }];
}
