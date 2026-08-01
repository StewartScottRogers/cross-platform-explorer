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
import { commands } from "./bindings.gen"; // typed client (CPE-964)
import { invoke, unwrap } from "./invoke";
import type { ViewMode, SortKey, SortDir, RecentFile, Favorite } from "./types";
import { COLUMN_DEFAULTS, META_COL_MIN, COLUMN_MAX, type ActiveMetaColumn } from "./columns";
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
import { parseCommands, serializeCommands } from "./userCommands";
import type { UserCommand } from "./userCommands";
import { parseBindings, serializeBindings } from "./macroBindings";
import type { MacroBinding } from "./macroBindings";
import { parseFrecent, serializeFrecent } from "./spotlightFrecency";
import type { Visit } from "./bindings.gen";

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
  metaColumnsByFolder: "cpe.metaColumnsByFolder",
  diagnostics: "cpe.diagnostics",
  verifyOnStartup: "cpe.verifyOnStartup",
  colorRules: "cpe.colorRules",
  integrityBaselines: "cpe.integrityBaselines",
  watchRules: "cpe.watchRules",
  workspaces: "cpe.workspaces",
  backupJobs: "cpe.backupJobs",
  userCommands: "cpe.userCommands",
  macroBindings: "cpe.macroBindings",
  watchedFolders: "cpe.watchedFolders",
  backupHistory: "cpe.backupHistory",
  autoRestore: "cpe.autoRestore",
  lastSession: "cpe.lastSession",
  dualPane: "cpe.dualPane",
  paneBPath: "cpe.paneBPath",
  networkLocations: "cpe.networkLocations",
  nativeBridgeEnabled: "cpe.nativeBridgeEnabled",
  spotlightHotkeyEnabled: "cpe.spotlightHotkeyEnabled",
  spotlightHotkeyChord: "cpe.spotlightHotkeyChord",
  spotlightFrecency: "cpe.spotlightFrecency",
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
    void commands.writeSettings(JSON.stringify(state)).catch(() => {
      // A failed settings save must never break the app (transport throw swallowed; a backend
      // Result error is likewise ignored — best-effort persist).
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
    const raw = unwrap(await commands.readSettings());
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

  // Spotlight global hotkey (CPE-1215, epic CPE-704): a fresh process starts with NO OS-level
  // registration (the plugin is initialized but claims nothing at Rust startup), so re-claim it here
  // when the user left it enabled in a prior session. Off means off — this only runs when the persisted
  // flag is true, and a failure (chord taken by another app, mobile target, plugin unavailable) is
  // swallowed: it must never block startup, and the Settings toggle can always retry.
  if (loadSpotlightHotkeyEnabled()) {
    try {
      await invoke("register_spotlight_hotkey", { chord: loadSpotlightHotkeyChord() });
    } catch {
      // best-effort — see comment above
    }
  }
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
// A metadata-column sort key is the `meta:<columnId>` prefix convention (CPE-1146); loosely
// validated (any non-empty suffix) since the column-set membership check happens at use time — a
// persisted `meta:` key for a column that's since been removed just degrades to an Empty-tiebreak
// stable sort, never a crash.
const isSortKey = (v: unknown): v is SortKey =>
  v === "name" || v === "modified" || v === "type" || v === "size" ||
  (typeof v === "string" && v.startsWith("meta:") && v.length > 5);
const isSortDir = (v: unknown): v is SortDir => v === "asc" || v === "desc";
const isBool = (v: unknown): v is boolean => typeof v === "boolean";
const isString = (v: unknown): v is string => typeof v === "string";
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

// Dual-pane / commander mode (CPE-677, epic CPE-617): the layout toggle + pane B's own folder,
// persisted so the split survives a restart. OFF by default — single-pane is the unchanged default.
export const loadDualPane = (): boolean => read(KEYS.dualPane, false, isBool);
export const saveDualPane = (v: boolean) => write(KEYS.dualPane, v);
export const loadPaneBPath = (): string => read(KEYS.paneBPath, "", isString);
export const savePaneBPath = (v: string) => write(KEYS.paneBPath, v);

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

// User-added network locations for the Home "Shared" tab (CPE-1163): raw `\\server\share` / `smb://…`
// addresses the user typed. Persisted here (like pins/favorites) — the backend merges these with the
// OS-enumerated mapped drives; a corrupt value degrades to [].
export const loadNetworkLocations = (): string[] => read(KEYS.networkLocations, [], isStringArray);
export const saveNetworkLocations = (v: string[]) => write(KEYS.networkLocations, v);

// Native-bridge opt-in (CPE-1177, epic CPE-717): gates the OS-native tag/comment sync controls (Pull/Push
// in TagEditor, the read-only Native metadata section in PropertiesDialog per CPE-1176). Off by default —
// the plain explorer never touches native file metadata unless the user opts in here.
export const loadNativeBridgeEnabled = (): boolean => read(KEYS.nativeBridgeEnabled, false, isBool);
export const saveNativeBridgeEnabled = (v: boolean) => write(KEYS.nativeBridgeEnabled, v);

// Spotlight global hotkey (CPE-1215, epic CPE-704): an OS-wide chord (via tauri-plugin-global-shortcut)
// that fires `spotlight:open` even while the main window is hidden/unfocused — the CPE-1216 overlay
// listens for it. Off by default: registering an OS-wide hotkey is exactly the kind of background cost
// that must stay opt-in ([[avoid-modal-permission-popups]] — this control lives only in Settings, never
// a launch-time prompt). `initSettings()` re-registers on startup when the flag was left on from a prior
// session; the SpotlightHotkeySettings component registers/unregisters live as the toggle/chord change.
export const DEFAULT_SPOTLIGHT_HOTKEY_CHORD = "CommandOrControl+Shift+Space";
export const loadSpotlightHotkeyEnabled = (): boolean => read(KEYS.spotlightHotkeyEnabled, false, isBool);
export const saveSpotlightHotkeyEnabled = (v: boolean) => write(KEYS.spotlightHotkeyEnabled, v);
export const loadSpotlightHotkeyChord = (): string =>
  read(KEYS.spotlightHotkeyChord, DEFAULT_SPOTLIGHT_HOTKEY_CHORD, isString);
export const saveSpotlightHotkeyChord = (v: string) => write(KEYS.spotlightHotkeyChord, v);

/** Append a network location, trimmed + de-duplicated (case/trailing-slash-insensitive). */
export function addNetworkLocation(list: string[], path: string): string[] {
  const trimmed = path.trim();
  if (!trimmed) return list;
  const key = trimmed.replace(/[\\/]+$/, "").toLowerCase();
  const without = list.filter((p) => p.trim().replace(/[\\/]+$/, "").toLowerCase() !== key);
  return [...without, trimmed];
}

/** Drop a network location by path (trailing-slash / case-insensitive match). */
export function removeNetworkLocation(list: string[], path: string): string[] {
  const key = path.trim().replace(/[\\/]+$/, "").toLowerCase();
  return list.filter((p) => p.trim().replace(/[\\/]+$/, "").toLowerCase() !== key);
}

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

// Active metadata columns, per folder (CPE-1146, epic CPE-707): which of CPE-1145's pickable columns
// (Dimensions, Duration, Pages, …) are showing in a given folder, in display order, with their own
// widths — keyed by the folder's absolute path so different folders can carry different column sets.
// Stored as ONE `Record<path, ActiveMetaColumn[]>` document (not a key per folder) so it round-trips
// through the same single settings.json as everything else. The default is "none active" (the ticket's
// design: a fresh folder shows only the 4 built-ins) — a folder absent from the map, or the map itself
// absent/corrupt, degrades to `[]` rather than crashing. A folder saved with an EMPTY active set is
// pruned from the map rather than stored as `path: []`, so the document stays small for the common case.
const isActiveMetaColumnArray = (v: unknown): v is ActiveMetaColumn[] =>
  Array.isArray(v) &&
  v.every(
    (x) =>
      x && typeof x === "object" &&
      typeof (x as ActiveMetaColumn).id === "string" && (x as ActiveMetaColumn).id.length > 0 &&
      typeof (x as ActiveMetaColumn).width === "number" && Number.isFinite((x as ActiveMetaColumn).width) &&
      (x as ActiveMetaColumn).width > 0,
  );

export const loadMetaColumnsForFolder = (path: string): ActiveMetaColumn[] => {
  const v = state[KEYS.metaColumnsByFolder];
  if (!v || typeof v !== "object") return [];
  const forFolder = (v as Record<string, unknown>)[path];
  if (!isActiveMetaColumnArray(forFolder)) return [];
  // Re-clamp on load (CPE-1140-style guard): a width saved under an older/different clamp rule (or
  // hand-edited) can never paint a too-narrow/absurdly-wide column on restore.
  return forFolder.map((c) => ({
    ...c,
    width: Math.max(META_COL_MIN, Math.min(COLUMN_MAX, Math.round(c.width))),
  }));
};

export const saveMetaColumnsForFolder = (path: string, cols: ActiveMetaColumn[]): void => {
  const v = state[KEYS.metaColumnsByFolder];
  const map: Record<string, ActiveMetaColumn[]> =
    v && typeof v === "object" ? { ...(v as Record<string, ActiveMetaColumn[]>) } : {};
  if (cols.length === 0) delete map[path];
  else map[path] = cols;
  write(KEYS.metaColumnsByFolder, map);
};

// Rule-based file coloring & labels (CPE-776, epic CPE-709). Stored as the raw `ColorRule[]`; loaded
// through the tolerant `parseRules` so a corrupt/hand-edited value degrades to `[]` rather than crashing.
export const loadColorRules = (): ColorRule[] => {
  const v = state[KEYS.colorRules];
  return v === undefined ? [] : parseRules(serializeRules(v as ColorRule[]));
};
export const saveColorRules = (v: ColorRule[]): void => write(KEYS.colorRules, v);

// User-defined templated commands (CPE-783, epic CPE-711): the ordered command list, loaded through the
// tolerant `parseCommands` so a corrupt entry degrades rather than crashing. Opt-in — empty until defined.
export const loadUserCommands = (): UserCommand[] => {
  const v = state[KEYS.userCommands];
  return v === undefined ? [] : parseCommands(serializeCommands(v as UserCommand[]));
};
export const saveUserCommands = (v: UserCommand[]): void => write(KEYS.userCommands, v);

// Macro surface/hotkey bindings (CPE-1191, epic CPE-739): which saved macros (CPE-1188 catalog,
// joined by name) show up in the context menu / palette, and their optional hotkey. Loaded through
// the tolerant `parseBindings` so a corrupt entry degrades rather than crashing. Opt-in — empty until
// a macro is explicitly bound.
export const loadMacroBindings = (): MacroBinding[] => {
  const v = state[KEYS.macroBindings];
  return v === undefined ? [] : parseBindings(serializeBindings(v as MacroBinding[]));
};
export const saveMacroBindings = (v: MacroBinding[]): void => write(KEYS.macroBindings, v);

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

// Spotlight frecency (CPE-1216, epic CPE-704): the overlay's "opened/revealed here before" usage
// document, loaded through the tolerant `parseFrecent` so a corrupt entry degrades rather than
// crashing. Opt-in in effect — empty until the overlay is actually used.
export const loadSpotlightFrecency = (): Visit[] => {
  const v = state[KEYS.spotlightFrecency];
  return typeof v === "string" ? parseFrecent(v) : [];
};
export const saveSpotlightFrecency = (v: Visit[]): void => write(KEYS.spotlightFrecency, serializeFrecent(v));

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
