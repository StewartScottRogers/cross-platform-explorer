// Smart-folder live-refresh (CPE-1230, epic CPE-978): while a smart folder (tag-only OR the CPE-1229
// structured kind) is open, a filesystem change under its scope should recompute the result view without
// a manual re-run. The existing `$tags` reactivity already covers a *tag* change; this covers a *disk*
// change (create/delete/rename) — reusing the EXISTING `folder-watch` FS-event bus (CPE-794) rather than
// standing up a second `notify` watcher (App.svelte merges the open smart folder's scope into the same
// `folder_watch_start` path set it already arms for the watched-folder-rules feature). Everything here is
// pure + DOM-free (unit-tested); `App.svelte` wires it to the live `folder-watch` listener + the existing
// `loadSmartEntries`/`loadStructuredSearchEntries` recompute functions.

import { normalizePath } from "./agentSessions";
import { parentDir } from "./contentSearch";
import type { FolderWatchEvent } from "./folderWatch";

/**
 * What an open smart folder needs watched, expressed as either:
 * - `"paths"` — the exact set of currently-matched paths (a tag smart folder: matches can live anywhere
 *   on disk, so there's no single root to watch — CPE-667 v1 scopes only to a tag, not a folder); or
 * - `"root"` — one recursive root (a structured search always evaluates from the one folder it was
 *   captured under, CPE-1229 — see `resolveSavedSearchRoot`).
 *
 * `null` when no smart folder is open — the caller should stop watching entirely so there's no
 * always-on cost while the plain listing is what's on screen.
 */
export type SmartFolderScope = { kind: "paths"; paths: string[] } | { kind: "root"; root: string } | null;

/** The literal paths to fold into `folder_watch_start`'s path set for `scope` — the single
 *  structured-search root, or (for a tag smart folder) the PARENT DIRECTORIES of the individually-
 *  tagged files. Empty when `scope` is `null`. Pure.
 *
 *  A tag scope's matches are bare FILE paths, but the backend `folder_watch_start`'s `notify` watcher
 *  only arms on directories — it silently skips a non-directory path (see `src-tauri/src/lib.rs`), so
 *  passing the tagged files themselves watches nothing and a tag-only smart folder never live-refreshes
 *  (UAT: "tag folders don't actually live-refresh"). Watching each file's parent directory instead
 *  arms the watcher; `changedPathInScope`'s exact-path match still filters the resulting events down
 *  to just the tagged files, so an unrelated sibling change in the same directory is not treated as
 *  in-scope. */
export function watchPathsForScope(scope: SmartFolderScope): string[] {
  if (!scope) return [];
  if (scope.kind === "root") return [scope.root];
  const dirs = scope.paths.map(parentDir).filter((d) => d !== "");
  return Array.from(new Set(dirs));
}

/** Whether one changed path from a `folder-watch` batch is "under" `scope` and should trigger a
 *  recompute: an exact match against a tracked tag path, or at/under the structured search's root.
 *  Path comparison is normalized (separator + case) via the shared `normalizePath`, matching how the
 *  rest of the app already compares OS paths cross-platform. Pure. */
export function changedPathInScope(changedPath: string, scope: SmartFolderScope): boolean {
  if (!scope) return false;
  const c = normalizePath(changedPath);
  if (scope.kind === "paths") return scope.paths.some((p) => normalizePath(p) === c);
  const r = normalizePath(scope.root);
  return c === r || c.startsWith(r + "/");
}

/** Whether any event in a `folder-watch` batch is relevant to the open smart folder's scope. Pure. */
export function batchTouchesScope(batch: FolderWatchEvent[], scope: SmartFolderScope): boolean {
  return batch.some((e) => changedPathInScope(e.path, scope));
}

/**
 * A trailing debounce: `schedule()` (re)arms a timer that fires `run` once after `waitMs` of quiet, so a
 * burst of `folder-watch` batches (a multi-file move, an extraction, a git checkout) collapses into ONE
 * recompute instead of one per batch — mirroring how the backend's own watch pumps
 * (`folder_watch_pump`/`index_watch_pump`) coalesce a debounce window before flushing. `cancel()` drops
 * any pending timer with no recompute (called when the smart folder closes, so a stale recompute never
 * lands after exit). Timer functions are injectable so this is unit-testable with fake timers.
 */
export class TrailingDebounce {
  private handle: ReturnType<typeof setTimeout> | null = null;

  constructor(
    private waitMs: number,
    private setTimer: typeof setTimeout = setTimeout,
    private clearTimer: typeof clearTimeout = clearTimeout,
  ) {}

  /** (Re)arm the timer; only the most recent `run` passed within `waitMs` actually fires. */
  schedule(run: () => void): void {
    if (this.handle !== null) this.clearTimer(this.handle);
    this.handle = this.setTimer(run, this.waitMs);
  }

  /** Drop any pending timer without firing it. Idempotent. */
  cancel(): void {
    if (this.handle !== null) {
      this.clearTimer(this.handle);
      this.handle = null;
    }
  }
}
