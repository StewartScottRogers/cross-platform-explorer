<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke, convertFileSrc } from "@tauri-apps/api/core";
  import { open as openFolderDialog, save as saveFileDialog } from "@tauri-apps/plugin-dialog";
  import { check, type Update } from "@tauri-apps/plugin-updater";
  import { relaunch, exit } from "@tauri-apps/plugin-process";
  import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
  import { getVersion } from "@tauri-apps/api/app";
  import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
  import { emit, once } from "@tauri-apps/api/event";

  import Icon from "./lib/components/Icon.svelte";
  import ContextBar from "./lib/components/ContextBar.svelte";
  import MenuBar from "./lib/components/MenuBar.svelte";
  import AboutDialog from "./lib/components/AboutDialog.svelte";
  import SettingsDialog from "./lib/components/SettingsDialog.svelte";
  import ConsentSheet from "./lib/components/ConsentSheet.svelte";
  import { startAiConsole, consoleUrlWith, platformActive, consentState, setConsent, type Capability, type ConsentState } from "./lib/sidecar";
  import { initAgentSessions, agentSessions, watchTargetFor, normalizePath, clearAgentSessions } from "./lib/agentSessions";
  import { startAgentWatch, stopAgentWatch, type FsActivity } from "./lib/sidecar";
  import { initAgentActivity, fsActivity, recentActivities, agentTimeline, affectsListing } from "./lib/agentActivity";
  import AgentTimeline from "./lib/components/AgentTimeline.svelte";
  import UpdateDialog from "./lib/components/UpdateDialog.svelte";
  import TabBar from "./lib/components/TabBar.svelte";
  import NavToolbar from "./lib/components/NavToolbar.svelte";
  import CommandBar from "./lib/components/CommandBar.svelte";
  import Sidebar from "./lib/components/Sidebar.svelte";
  import RepoBrowser from "./lib/components/RepoBrowser.svelte";
  import AgentMenu from "./lib/components/AgentMenu.svelte";
  import Toolbar from "./lib/components/Toolbar.svelte";
  import FileList from "./lib/components/FileList.svelte";
  import HomeView from "./lib/components/HomeView.svelte";
  import DetailsPane from "./lib/components/DetailsPane.svelte";
  import PreviewPane from "./lib/components/PreviewPane.svelte";
  import type { ArchiveEntry } from "./lib/preview/provider";
  import StatusBar from "./lib/components/StatusBar.svelte";
  import SyncDialog from "./lib/components/SyncDialog.svelte";
  import { loadSyncPolicy } from "./lib/syncPolicy";
  import { loadAutoMirror, isDue, autoSyncActions, pausedReason } from "./lib/autoMirror";
  import ContextMenu from "./lib/components/ContextMenu.svelte";
  import ConfirmDialog from "./lib/components/ConfirmDialog.svelte";
  import ShortcutsDialog from "./lib/components/ShortcutsDialog.svelte";
  import ContentSearchDialog from "./lib/components/ContentSearchDialog.svelte";
  import DuplicatesDialog from "./lib/components/DuplicatesDialog.svelte";
  import { namesList, detailList, csvList } from "./lib/listing";
  import { parentDir as parentOfPath } from "./lib/contentSearch";
  import PropertiesDialog from "./lib/components/PropertiesDialog.svelte";
  import BatchRenameDialog from "./lib/components/BatchRenameDialog.svelte";
  import type { RenameItem } from "./lib/batchRename";

  import { t } from "./lib/i18n";
  import { friendlyError, splitPath, formatPathsForClipboard } from "./lib/format";
  import { withBusy } from "./lib/busy";
  import { sortEntries } from "./lib/sort";
  import { uniqueName, uniqueNameWithExt } from "./lib/naming";
  import { validateFileName } from "./lib/filename";
  import { matchesQuery } from "./lib/search";
  import { matchesGlob } from "./lib/glob";
  import PatternSelectDialog from "./lib/components/PatternSelectDialog.svelte";
  import { firstMatchIndex } from "./lib/typeahead";
  import { clampWidth } from "./lib/resize";
  import {
    createHistory, visit, back, forward, canGoBack, canGoForward, current,
    type History,
  } from "./lib/history";
  import { pushClosedTab, keepOnly, keepThroughRight } from "./lib/tabs";
  import TabMenu from "./lib/components/TabMenu.svelte";
  import {
    emptySelection, click as selClick, selectOnly, selectAll, moveLead,
    selectedIndices, selectedCount, remapByPath, invertSelection, selectIndices,
    type Selection,
  } from "./lib/selection";
  import {
    emptyClipboard, stage, isEmpty as clipEmpty, canPaste as clipCanPaste,
    type Clipboard,
  } from "./lib/clipboard";
  import { detectContexts, type FolderAction } from "./lib/folderContext";
  import { isExecutable, iconFor, sameTypeIndices, matchesFileFilter } from "./lib/filetypes";
  import * as settings from "./lib/settings";
  import {
    pushUndo, popUndo, canUndo, peekLabel, invert, deletedPaths, type UndoEntry,
  } from "./lib/undo";
  import type { DirEntry, Place, SortKey, SortDir, ViewMode, RecentFile, Favorite } from "./lib/types";

  interface OpResult { path: string; ok: boolean; error: string }

  const HOME = " home"; // sentinel: the Home view, not a filesystem path

  interface Tab { id: number; history: History }

  let nextTabId = 2;
  let tabs: Tab[] = [{ id: 1, history: createHistory(HOME) }];
  let activeId = 1;
  /** Folders of recently-closed tabs, for Ctrl+Shift+T (CPE-356). */
  let closedTabPaths: string[] = [];
  /** Open tab context menu (CPE-357), or null. */
  let tabMenu: { id: number; x: number; y: number } | null = null;

  let entries: DirEntry[] = [];
  let places: Place[] = [];
  let drives: Place[] = [];

  let error = "";
  let loading = false;
  let notice = "";
  let noticeIsError = false;
  let noticeTimer: ReturnType<typeof setTimeout> | undefined;

  let selection: Selection = emptySelection();
  let rowEls: HTMLElement[] = [];
  // Type-ahead find: accumulated prefix and the time of the last keystroke.
  let typeAheadBuffer = "";
  let typeAheadAt = 0;
  let clipboard: Clipboard = emptyClipboard();

  let sortKey: SortKey = "name";
  let sortDir: SortDir = "asc";
  let view: ViewMode = "details";
  /** Active file-type filter key (CPE-358); "all" = no filter. */
  let fileFilter = "all";
  /** Whether folders sort above files (CPE-359). */
  let foldersFirst = true;
  let showDetails = true;
  let showPreview = true;
  /** Cap on how much of a text file the preview will load. */
  const PREVIEW_MAX_BYTES = 256 * 1024;

  // ---- resizable panels ----
  const SIDEBAR_MIN = 160, SIDEBAR_MAX = 480;
  const RIGHT_MIN = 220, RIGHT_MAX = 560;
  let sidebarWidth = 220;
  let rightWidth = 300;
  let resizing: null | "left" | "right" = null;
  let resizeStartX = 0;
  let resizeStartW = 0;

  $: gridCols = showDetails
    ? `${sidebarWidth}px 6px 1fr 6px ${rightWidth}px`
    : `${sidebarWidth}px 6px 1fr`;

  function startResize(which: "left" | "right", e: MouseEvent) {
    resizing = which;
    resizeStartX = e.clientX;
    resizeStartW = which === "left" ? sidebarWidth : rightWidth;
    window.addEventListener("mousemove", onResize);
    window.addEventListener("mouseup", endResize);
    e.preventDefault();
  }
  function onResize(e: MouseEvent) {
    const dx = e.clientX - resizeStartX;
    if (resizing === "left") {
      sidebarWidth = clampWidth(resizeStartW + dx, SIDEBAR_MIN, SIDEBAR_MAX);
    } else if (resizing === "right") {
      // The right pane grows as the divider moves left, so subtract dx.
      rightWidth = clampWidth(resizeStartW - dx, RIGHT_MIN, RIGHT_MAX);
    }
  }
  function endResize() {
    window.removeEventListener("mousemove", onResize);
    window.removeEventListener("mouseup", endResize);
    if (resizing === "left") settings.saveSidebarWidth(sidebarWidth);
    else if (resizing === "right") settings.saveRightWidth(rightWidth);
    resizing = null;
  }
  let showHidden = false;
  let pins: string[] = [];
  let recents: RecentFile[] = [];
  let favorites: Favorite[] = [];
  let recentFolders: RecentFile[] = [];
  let columnWidths: number[] = settings.loadColumnWidths();
  let search = "";
  let editingPath = false;

  let renamingPath = "";
  let renameValue = "";
  /** Path of a freshly-created folder, so we can auto-rename it once listed. */
  let pendingRenamePath = "";
  let pendingSelectPath = ""; // select (no rename) a just-created item after reload

  let undoStack: UndoEntry[] = [];
  /** Whether THIS platform can restore from the trash (false on macOS). */
  let canRestoreTrash = false;
  /** Paths currently being dragged, shared with the sidebar as a drop target. */
  let draggedPaths: string[] = [];
  let ctx: { x: number; y: number; target: "item" | "empty" } | null = null;
  let confirm: { title: string; message: string; label: string; onYes: () => void } | null = null;
  let propsFor: DirEntry[] | null = null;
  let batchRenameFor: DirEntry[] | null = null;

  // ---- Application menu (CPE-229) ----
  const REPO_URL = "https://github.com/StewartScottRogers/cross-platform-explorer";
  let showAbout = false;
  let showSettings = false;
  let shortcutsOpen = false;
  /** "Search in files" content-search overlay (Ctrl+Shift+F), scoped to the current folder (CPE-417). */
  let contentSearchOpen = false;
  /** "Find duplicate files" overlay, scoped to the current folder (CPE-421). */
  let duplicatesOpen = false;
  let patternSelectOpen = false;
  /** Repositories browser overlay (CPE-434/435) — browse GitHub & other forges in-app. */
  let showRepos = false;
  /** Git sync status of the current folder (CPE-462) — two-way mirror status bar. Null when the
      folder isn't a git repo, or in the plain (non-sidecar) build where the command is absent. */
  let gitStatus: { is_repo?: boolean; branch?: string; ahead?: number; behind?: number; dirty?: boolean } | null = null;

  /** The path whose full two-way-mirror Sync dialog is open (CPE-495), or null when closed. */
  let syncDialogPath: string | null = null;

  /** Refresh the git sync status when the folder changes (read-only, best-effort). The dry-run
      preview honours this repo's saved on-diverge policy so the status bar and the Sync dialog agree. */
  async function refreshGitStatus(path: string) {
    if (!path || isHome || archive) { gitStatus = null; return; }
    try {
      const s = await invoke<typeof gitStatus>("forge_repo_status", { path, onDiverge: loadSyncPolicy(path) });
      gitStatus = s && (s as { is_repo?: boolean }).is_repo ? s : null;
    } catch {
      gitStatus = null; // plain build (command absent) or git unavailable
    }
  }
  $: refreshGitStatus(currentPath);

  /** Run a safe sync step (Pull = ff-only, Push = no-force) via the host, then re-list (CPE-462). */
  async function doSync(action: "pull" | "push") {
    try {
      await withBusy(() => invoke("forge_sync", { path: currentPath, action }));
      await refreshGitStatus(currentPath);
      refresh();
    } catch (e) {
      notice = "Sync failed: " + (e instanceof Error ? e.message : String(e));
      noticeIsError = true;
    }
  }

  // --- Scheduled / background auto-mirror (CPE-497) -----------------------------------------------
  /** Last successful auto-sync per repo path (this session) — gates the interval. In-memory: a
      restart just means the next auto-sync happens sooner, which is harmless. */
  let lastAutoSync = new Map<string, number>();
  let autoMirrorTimer: ReturnType<typeof setInterval> | undefined;
  let autoSyncRunning = false;

  /** If the current repo has auto-mirror enabled and is due, run ONLY the unattended-safe steps
      (fast-forward pull + push). A divergence/conflict pauses and surfaces — it is never reconciled
      in the background, and nothing is ever force-pushed (`forge_sync` has no force action). */
  async function maybeAutoSync() {
    const path = currentPath;
    if (autoSyncRunning || !path || isHome || archive) return;
    if (!gitStatus?.is_repo) return;
    const cfg = loadAutoMirror(path);
    if (!cfg.enabled) return;
    if (!isDue(lastAutoSync.get(path) ?? null, cfg.intervalMinutes, Date.now())) return;

    autoSyncRunning = true;
    try {
      const plan = await invoke<typeof gitStatus>("forge_repo_status", { path, onDiverge: loadSyncPolicy(path) });
      if (!plan || !(plan as { is_repo?: boolean }).is_repo) return;
      const actions = autoSyncActions(plan as Parameters<typeof autoSyncActions>[0]);
      if (actions.length === 0) {
        const reason = pausedReason(plan as Parameters<typeof pausedReason>[0]);
        if (reason) showNotice("Auto-sync paused — " + reason, false);
        return; // nothing safe to do (or diverged) — don't hammer; wait the interval out
      }
      for (const action of actions) {
        await invoke("forge_sync", { path, action });
      }
      lastAutoSync.set(path, Date.now());
      if (currentPath === path) { await refreshGitStatus(path); refresh(); }
      showNotice(`Auto-synced ${new Date().toLocaleTimeString()}`, false);
    } catch (e) {
      // A failed background sync must never nag repeatedly: back off by marking it "done" for this
      // interval, and surface once.
      lastAutoSync.set(path, Date.now());
      showNotice("Auto-sync failed: " + (e instanceof Error ? e.message : String(e)), true);
    } finally {
      autoSyncRunning = false;
    }
  }
  /** Right-click "close the AI Console" menu (CPE-457), shown from an Agents leaf or the AI
      Console button. `label` differs per source; confirming stops the console + clears the leaves. */
  let agentMenu: { x: number; y: number; label: string; sessionId?: string; sessionLabel?: string } | null = null;

  /** Close the AI Console entirely (all running agents) and clear the Agents leaves. The console
      process is reaped, so no per-session `ended` arrives — clear the leaves here (CPE-457). */
  async function closeAllConsoles() {
    agentMenu = null;
    try {
      await invoke("sidecar_stop", { id: "ai-console" });
    } catch (e) {
      console.debug("close consoles failed:", e);
    }
    clearAgentSessions();
  }
  /** Close a single agent session (CPE-489) — routes to the AI Console's per-session close endpoint
      via the host. The console emits an `ended` for it, which prunes the leaf; the others keep running. */
  async function closeOneConsole(sessionId: string) {
    agentMenu = null;
    try {
      await invoke("sidecar_close_session", { sessionId });
    } catch (e) {
      console.debug("close session failed:", e);
    }
  }
  /** True in sidecar-platform builds — gates the AI Console toolbar button (CPE-351). */
  let aiConsoleAvailable = false;
  /** Teardown for the Agent Watch session listener (CPE-396). */
  let unlistenSessions: (() => void) | null = null;
  // Agent Watch view (CPE-399): the Project folder currently being watched (or ""), and the
  // teardown for its activity listener. Watching turns on only while the explorer is inside a
  // running agent's project, and off the moment it leaves — off means off (AGENT-WATCH.md).
  let activeWatchCwd = "";
  let unlistenActivity: (() => void) | null = null;
  /** Whether the Agent Watch activity timeline drawer is open (CPE-400). */
  let showTimeline = false;

  const baseNameOf = (p: string) => normalizePath(p).split("/").pop() || p;

  /** Debounce handle for live folder re-list while watching (CPE-401). */
  let watchRefreshTimer: ReturnType<typeof setTimeout> | null = null;

  /** When the agent adds/removes a file in the folder on screen, re-list it (debounced) so the
   *  change appears — created files show up (and get their badge), deleted ones vanish (CPE-401). */
  function onAgentBatch(items: FsActivity[]) {
    if (!activeWatchCwd || !affectsListing(items, currentPath)) return;
    if (watchRefreshTimer) clearTimeout(watchRefreshTimer);
    watchRefreshTimer = setTimeout(() => refresh(), 400);
  }

  /** Start/stop the filesystem watch as the watched project changes (CPE-398/399). */
  async function syncAgentWatch(cwd: string) {
    if (cwd === activeWatchCwd) return;
    activeWatchCwd = cwd;
    unlistenActivity?.();
    unlistenActivity = null;
    if (watchRefreshTimer) { clearTimeout(watchRefreshTimer); watchRefreshTimer = null; }
    await stopAgentWatch();
    if (cwd) {
      unlistenActivity = await initAgentActivity(onAgentBatch);
      await startAgentWatch(cwd);
    } else {
      showTimeline = false; // no watched project ⇒ close the timeline drawer (CPE-400)
    }
  }

  // Re-evaluate whenever the session list or the current folder changes.
  $: syncAgentWatch(watchTargetFor($agentSessions, currentPath));
  $: watchedAgentName =
    $agentSessions.find((s) => normalizePath(s.cwd) === normalizePath(activeWatchCwd))?.agentName || "agent";
  $: recentChanges = activeWatchCwd ? recentActivities($fsActivity, 6) : [];

  // Free disk space for the status bar (CPE-403). Refetched on navigation; hidden for Home /
  // archives; a stale response (navigated away before it resolved) is discarded.
  let diskFree: number | null = null;
  let diskTotal: number | null = null;
  /** Per-drive free/total for the sidebar usage bars (CPE-406); filled once on mount. */
  let driveUsage: Record<string, { free: number; total: number }> = {};

  /** Probe each drive's capacity once, non-blocking — a slow/failed probe never delays the UI. */
  async function loadDriveUsage(list: Place[]) {
    await Promise.all(
      list.map(async (d) => {
        try {
          const s = await invoke<{ free: number; total: number }>("disk_space", { path: d.path });
          driveUsage = { ...driveUsage, [d.path]: s };
        } catch {
          /* skip a drive we can't stat (e.g. an empty card reader) */
        }
      }),
    );
  }
  $: updateDiskSpace(currentPath, isHome, !!archive);
  async function updateDiskSpace(path: string, home: boolean, inArchive: boolean) {
    if (home || inArchive || !path) { diskFree = null; diskTotal = null; return; }
    try {
      const d = await invoke<{ free: number; total: number }>("disk_space", { path });
      if (currentPath === path) { diskFree = d.free; diskTotal = d.total; }
    } catch { if (currentPath === path) { diskFree = null; diskTotal = null; } }
  }

  const AI_CONSOLE_LABEL = "ai-console";
  let consentPrompt: ConsentState | null = null;

  /** Open the AI Console in its own OS window (CPE-335) — native title bar (drag it around
      the screen), resize borders, and frame, independent of the explorer's focus. Only
      meaningful in a `sidecar-platform` build. Gated by capability consent (CPE-296). The
      window loads the sidecar's loopback URL directly and has NO Tauri API (its label is
      in no capability), so the untrusted sidecar UI stays isolated. */
  /** Pending explorer→console hand-off (CPE-313): folder to scope to and a task hint,
      consumed by launchAiConsole once consent (if any) is resolved. */
  let consoleContext: { cwd?: string; task?: string } = {};

  async function openAiConsole(ctx: { cwd?: string; task?: string } = {}) {
    showSettings = false;
    consoleContext = ctx;
    const existing = await WebviewWindow.getByLabel(AI_CONSOLE_LABEL);
    if (existing) {
      await existing.setFocus(); // can't re-scope a live window without disrupting sessions
      if (ctx.cwd) showNotice("AI Console is already open — set the working folder in its toolbar.", false);
      return;
    }
    const state = await consentState("ai-console");
    if (state && state.undecided.length > 0) {
      consentPrompt = state; // launch continues in onConsentDecision
      return;
    }
    await launchAiConsole();
  }

  async function launchAiConsole() {
    const base = await startAiConsole();
    if (!base) { showNotice("AI Console isn't available in this build.", true); return; }
    const url = consoleUrlWith(base, consoleContext.cwd, consoleContext.task);
    try {
      const win = new WebviewWindow(AI_CONSOLE_LABEL, {
        url,
        title: "AI Console",
        width: 1100,
        height: 760,
        minWidth: 640,
        minHeight: 400,
        resizable: true,
        center: true,
      });
      win.once("tauri://error", () => showNotice("Couldn't open the AI Console window.", true));
    } catch {
      showNotice("Couldn't open the AI Console window.", true);
    }
  }

  /** Persist the consent decision from the sheet, then launch the console. */
  async function onConsentDecision(
    e: CustomEvent<{ granted: Capability[]; decided: Capability[] }>,
  ) {
    await setConsent("ai-console", e.detail.granted, e.detail.decided);
    consentPrompt = null;
    await launchAiConsole();
  }
  let appVersion = "";

  // ---- In-app updates (CPE-230) ----
  // The updater already checks a signed manifest, downloads, verifies, and can
  // relaunch. Here we drive it through a consent-first UI: detect → prompt →
  // (on user's say-so) download with progress → install → relaunch.
  let pendingUpdate: Update | null = null;
  let showUpdate = false;
  let updateState: "checking" | "available" | "uptodate" | "downloading" | "error" = "checking";
  let updateProgress = 0;
  let updateIndeterminate = false;
  let updateError = "";

  // ---- Archive browsing (CPE-242): read-only virtual view inside a .zip ----
  const ARCH = "\u0000arch:"; // sentinel prefix for in-archive breadcrumb paths
  const ARCHIVE_EXTS = new Set(["zip", "jar", "apk", "war", "ear", "ipa", "xpi", "whl", "nupkg", "vsix"]);
  interface ArchiveView { zipPath: string; zipName: string; entries: ArchiveEntry[]; inner: string }
  let archive: ArchiveView | null = null;

  const isArchiveFile = (e: DirEntry) => !e.is_dir && ARCHIVE_EXTS.has(e.extension);

  // Formats extract_archive can unpack to a folder (CPE-252): the zip family plus
  // tar/gz/7z. ("foo.tar.gz" has extension "gz"; the backend disambiguates by the
  // full path, so listing "gz" here is enough to offer Extract.)
  const EXTRACT_EXTS = new Set([...ARCHIVE_EXTS, "tar", "gz", "tgz", "7z"]);
  const isExtractable = (e: DirEntry) => !e.is_dir && EXTRACT_EXTS.has(e.extension);

  /** The immediate children of the archive's current inner folder, as DirEntry-
      shaped rows (folders are derived from deeper paths when not explicit). */
  function archiveChildren(view: ArchiveView): DirEntry[] {
    const prefix = view.inner ? view.inner + "/" : "";
    const seen = new Map<string, DirEntry>();
    for (const e of view.entries) {
      // Normalise separators: some zips (PowerShell Compress-Archive) use "\".
      const full = e.name.replace(/\\/g, "/").replace(/\/+$/, "");
      if (!full || (prefix && !full.startsWith(prefix))) continue;
      const rest = full.slice(prefix.length);
      if (!rest) continue;
      const slash = rest.indexOf("/");
      const childName = slash === -1 ? rest : rest.slice(0, slash);
      if (seen.has(childName)) continue;
      const isDir = slash !== -1 || e.is_dir;
      seen.set(childName, {
        name: childName,
        path: prefix + childName,
        is_dir: isDir,
        size: slash === -1 && !e.is_dir ? e.size : 0,
        modified: null,
        extension: isDir ? "" : (childName.includes(".") ? childName.split(".").pop()!.toLowerCase() : ""),
        hidden: false,
      });
    }
    return [...seen.values()];
  }

  async function enterArchive(entry: DirEntry) {
    try {
      const entries = await invoke<ArchiveEntry[]>("read_archive_entries", { path: entry.path });
      archive = { zipPath: entry.path, zipName: entry.name, entries, inner: "" };
      selection = emptySelection();
      search = "";
    } catch {
      showNotice(`Couldn't open the archive "${entry.name}".`, true);
    }
  }

  function exitArchive() {
    archive = null;
    selection = emptySelection();
  }

  /** Guard file-mutating actions: the in-archive view is read-only. */
  function blockedInArchive(): boolean {
    if (archive) {
      showNotice("This is a read-only view inside an archive.", true);
      return true;
    }
    return false;
  }

  async function openInArchive(entry: DirEntry) {
    if (!archive) return;
    if (entry.is_dir) {
      archive = { ...archive, inner: entry.path };
      selection = emptySelection();
      return;
    }
    try {
      const zipPath = archive.zipPath;
      const temp = await withBusy(() =>
        invoke<string>("extract_archive_entry", { zip: zipPath, inner: entry.path }),
      );
      await invoke("open_external", { path: temp });
    } catch {
      showNotice(`Couldn't open "${entry.name}" from the archive.`, true);
    }
  }

  function archiveCrumbs(view: ArchiveView) {
    const out = [{ name: view.zipName, path: ARCH + "" }];
    if (view.inner) {
      let acc = "";
      for (const p of view.inner.split("/")) {
        acc = acc ? acc + "/" + p : p;
        out.push({ name: p, path: ARCH + acc });
      }
    }
    return out;
  }

  /** Crumb / address navigation — handles in-archive crumbs and exits the archive
      for real paths. */
  function onCrumbNavigate(detail: string) {
    if (detail.startsWith(ARCH)) {
      if (archive) { archive = { ...archive, inner: detail.slice(ARCH.length) }; selection = emptySelection(); }
      return;
    }
    if (archive) exitArchive();
    if (detail === HOME || detail.startsWith(" ")) navigate(detail);
    else navigateToTyped(detail);
  }

  let navToolbar: NavToolbar;

  $: activeTab = tabs.find((t) => t.id === activeId) as Tab;
  $: currentPath = current(activeTab.history) ?? HOME;
  $: isHome = currentPath === HOME;

  function showNotice(message: string, isError = false) {
    notice = message;
    noticeIsError = isError;
    clearTimeout(noticeTimer);
    noticeTimer = setTimeout(() => (notice = ""), 5000);
  }

  function setHistory(h: History) {
    tabs = tabs.map((t) => (t.id === activeId ? { ...t, history: h } : t));
  }

  async function loadPath(path: string, keepSelection = false) {
    const previouslySelected = keepSelection
      ? selectedIndices(selection).map((i) => visible[i]?.path).filter(Boolean)
      : [];

    if (!keepSelection) {
      selection = emptySelection();
      search = "";
    }
    error = "";

    if (path === HOME) {
      entries = [];
      loading = false;
      return;
    }

    loading = true;
    try {
      entries = await withBusy(() => invoke<DirEntry[]>("list_dir", { path }));
    } catch (e) {
      entries = [];
      error = friendlyError(String(e));
    } finally {
      loading = false;
    }

    // A folder we actually opened joins the recently-visited MRU (CPE-342). The
    // error guard means an unreadable path (or a file mistaken for a folder, e.g.
    // an archive) is never recorded.
    if (!error) recordRecentFolder(path);

    // Re-derive the selection from paths — indices are meaningless after a reload.
    if (keepSelection && previouslySelected.length > 0) {
      selection = remapByPath(previouslySelected, visible);
    }

    // A folder we just created gets selected and put straight into rename mode.
    if (pendingRenamePath) {
      const i = visible.findIndex((e) => e.path === pendingRenamePath);
      if (i >= 0) {
        selection = selectOnly(i);
        beginRename(visible[i]);
      }
      pendingRenamePath = "";
    }

    // A newly created zip/extract folder gets selected (but not renamed).
    if (pendingSelectPath) {
      const i = visible.findIndex((e) => e.path === pendingSelectPath);
      if (i >= 0) selection = selectOnly(i);
      pendingSelectPath = "";
    }
  }

  async function navigate(path: string) {
    setHistory(visit(activeTab.history, path));
    await loadPath(path);
  }

  /** Navigate to a file's folder and select + scroll to the file itself (CPE-423). Used by the
   *  content-search and duplicate-finder results so a hit lands on the file, not just its folder. */
  async function revealFileInApp(filePath: string) {
    const dir = parentOfPath(filePath);
    if (!dir) return;
    pendingSelectPath = filePath; // the post-load hook selects it; the reactive block scrolls to it
    await navigateToTyped(dir);
  }

  async function goBack() {
    if (!canGoBack(activeTab.history)) return;
    const h = back(activeTab.history);
    setHistory(h);
    await loadPath(current(h) as string);
  }

  async function goForward() {
    if (!canGoForward(activeTab.history)) return;
    const h = forward(activeTab.history);
    setHistory(h);
    await loadPath(current(h) as string);
  }

  async function goUp() {
    if (archive) {
      if (archive.inner === "") exitArchive();
      else { archive = { ...archive, inner: archive.inner.split("/").slice(0, -1).join("/") }; selection = emptySelection(); }
      return;
    }
    if (isHome) return;
    try {
      const parent = await invoke<string | null>("parent_dir", { path: currentPath });
      await navigate(parent ?? HOME);
    } catch {
      await navigate(HOME);
    }
  }

  async function refresh() {
    if (archive) {
      try {
        const entries = await invoke<ArchiveEntry[]>("read_archive_entries", { path: archive.zipPath });
        archive = { ...archive, entries };
      } catch { /* keep current view */ }
      return;
    }
    await loadPath(currentPath, true);
  }

  /** Navigate to a typed path, verifying it exists rather than dead-ending. */
  async function navigateToTyped(raw: string) {
    const expanded = raw.replace(/%([^%]+)%/g, (_m, name) => {
      // Only USERPROFILE is reliably available to the webview; anything else
      // is left as-is rather than silently blanked.
      if (String(name).toUpperCase() === "USERPROFILE") return homePath || _m;
      return _m;
    });
    try {
      await invoke<DirEntry[]>("list_dir", { path: expanded });
      await navigate(expanded);
    } catch {
      showNotice(`Can't find "${raw}". Check the spelling and try again.`, true);
    }
  }

  let homePath = "";

  async function open(entry: DirEntry) {
    if (archive) { await openInArchive(entry); return; }
    if (entry.is_dir) {
      await navigate(entry.path);
      return;
    }
    if (isArchiveFile(entry)) { await enterArchive(entry); return; }
    try {
      // open_external runs it through the OS shell — reliably launches the
      // default app, and executes .exe/.cmd/.bat (CPE-240).
      await invoke("open_external", { path: entry.path });
      recents = settings.addRecent(recents, { path: entry.path, name: entry.name });
      settings.saveRecents(recents);
    } catch (e) {
      console.debug("open failed:", e);
      showNotice(`Can't open "${entry.name}" — no app is associated with this file type.`, true);
    }
  }

  async function openRecent(path: string) {
    try {
      await invoke("open_external", { path });
    } catch {
      // A recent file that no longer opens is removed rather than nagging forever.
      recents = recents.filter((r) => r.path !== path);
      settings.saveRecents(recents);
      showNotice("That file is no longer available — removed from Recent.", true);
    }
  }

  // ---- tabs ----
  function newTab() {
    const tab: Tab = { id: nextTabId++, history: createHistory(HOME) };
    tabs = [...tabs, tab];
    activeId = tab.id;
    loadPath(HOME);
  }

  /** Open a folder in a new background tab, leaving the current tab active. */
  function openInNewTab(entry: DirEntry) {
    if (!entry?.is_dir) return;
    const tab: Tab = { id: nextTabId++, history: createHistory(entry.path) };
    tabs = [...tabs, tab];
    showNotice(`Opened "${entry.name}" in a new tab.`);
  }

  function closeTab(id: number) {
    if (tabs.length === 1) return;
    const idx = tabs.findIndex((t) => t.id === id);
    const closing = tabs[idx];
    if (closing) closedTabPaths = pushClosedTab(closedTabPaths, current(closing.history) ?? HOME);
    tabs = tabs.filter((t) => t.id !== id);
    if (activeId === id) {
      const fallback = tabs[Math.max(0, idx - 1)];
      activeId = fallback.id;
      loadPath(current(fallback.history) ?? HOME);
    }
  }

  /** Reopen the most recently closed tab at its folder (Ctrl+Shift+T, CPE-356). */
  function reopenClosedTab() {
    if (closedTabPaths.length === 0) return;
    const path = closedTabPaths[closedTabPaths.length - 1];
    closedTabPaths = closedTabPaths.slice(0, -1);
    const tab: Tab = { id: nextTabId++, history: createHistory(path) };
    tabs = [...tabs, tab];
    activeId = tab.id;
    loadPath(path);
  }

  /** Record the folders of the tabs about to close so Ctrl+Shift+T can bring them back. */
  function recordClosing(closing: Tab[]) {
    for (const t of closing) closedTabPaths = pushClosedTab(closedTabPaths, current(t.history) ?? HOME);
  }

  /** Tab context-menu actions (CPE-357). */
  function onTabMenuAction(action: "duplicate" | "close-others" | "close-right") {
    const menu = tabMenu;
    tabMenu = null;
    if (!menu) return;
    if (action === "duplicate") {
      const t = tabs.find((x) => x.id === menu.id);
      if (t) {
        const path = current(t.history) ?? HOME;
        const tab: Tab = { id: nextTabId++, history: createHistory(path) };
        tabs = [...tabs, tab];
        activeId = tab.id;
        loadPath(path);
      }
      return;
    }
    const keep = action === "close-others"
      ? keepOnly(tabs.map((t) => t.id), menu.id)
      : keepThroughRight(tabs.map((t) => t.id), menu.id);
    recordClosing(tabs.filter((t) => !keep.includes(t.id)));
    const activeClosed = !keep.includes(activeId);
    tabs = tabs.filter((t) => keep.includes(t.id));
    if (activeClosed) {
      activeId = menu.id;
      const cur = tabs.find((t) => t.id === menu.id);
      if (cur) loadPath(current(cur.history) ?? HOME);
    }
  }

  /** Select every visible entry whose name matches the glob (CPE-360). */
  function selectByPattern(pattern: string) {
    patternSelectOpen = false;
    const idx = visible
      .map((e, i) => (matchesGlob(e.name, pattern) ? i : -1))
      .filter((i) => i >= 0);
    selection = selectIndices(idx);
    showNotice(
      idx.length === 0
        ? `No items match "${pattern}".`
        : `Selected ${idx.length} item${idx.length === 1 ? "" : "s"} matching "${pattern}".`,
    );
  }

  function selectTab(id: number) {
    activeId = id;
    const tab = tabs.find((t) => t.id === id);
    if (tab) loadPath(current(tab.history) ?? HOME);
  }

  function cycleTab(delta: number) {
    if (tabs.length < 2) return;
    const i = tabs.findIndex((t) => t.id === activeId);
    const next = (i + delta + tabs.length) % tabs.length;
    selectTab(tabs[next].id);
  }

  // ---- derived listing ----
  $: folderName = archive
    ? (archive.inner ? archive.inner.split("/").at(-1)! : archive.zipName)
    : isHome ? "Home" : (splitPath(currentPath).at(-1)?.name ?? currentPath);
  $: searching = search.trim().length > 0;

  // Folder-context detection (CPE-235): runs on the RAW listing (so hidden
  // markers like `.git` are seen regardless of the show-hidden setting).
  $: folderContexts = (isHome || archive) ? [] : detectContexts({ path: currentPath, entries });

  $: shown = entries.filter((e) => showHidden || !e.hidden);

  $: filtered = searching
    ? shown.filter((e) => matchesQuery(e.name, search))
    : shown;

  // File-type filter (CPE-358): narrows to a category, applied after search/hidden.
  $: typeFiltered = fileFilter === "all"
    ? filtered
    : filtered.filter((e) => matchesFileFilter(e, fileFilter));

  $: visible = archive
    ? sortEntries(archiveChildren(archive), sortKey, sortDir, foldersFirst)
    : sortEntries(typeFiltered, sortKey, sortDir, foldersFirst);

  $: crumbs = archive
    ? [{ name: "Home", path: HOME }, ...splitPath(currentPath), ...archiveCrumbs(archive)]
    : isHome
      ? [{ name: "Home", path: HOME }]
      : [{ name: "Home", path: HOME }, ...splitPath(currentPath)];

  $: selectedEntries = selectedIndices(selection)
    .map((i) => visible[i])
    .filter(Boolean);

  $: selectedSize = selectedEntries.reduce((n, e) => n + (e.is_dir ? 0 : e.size), 0);
  $: itemCount = isHome ? places.length + drives.length + pins.length : visible.length;
  // The folder's pre-filter total, so the status bar can read "X of Y items" (CPE-407).
  $: totalCount = isHome || archive ? itemCount : shown.length;
  $: pasteCheck = clipCanPaste(clipboard, isHome ? "" : currentPath);
  $: cutPaths = clipboard.mode === "cut" ? clipboard.paths : [];

  $: tabList = tabs.map((t) => {
    const p = current(t.history) ?? HOME;
    return { id: t.id, title: p === HOME ? "Home" : (splitPath(p).at(-1)?.name ?? p) };
  });

  $: if (selection.lead >= 0 && rowEls[selection.lead]) {
    rowEls[selection.lead].scrollIntoView({ block: "nearest" });
  }

  // ---- file operations ----
  function reportResults(results: OpResult[], verb: string) {
    const failed = results.filter((r) => !r.ok);
    if (failed.length === 0) {
      showNotice(`${verb} ${results.length} item${results.length === 1 ? "" : "s"}.`);
    } else {
      // Never swallow a partial failure — name what went wrong.
      const first = failed[0];
      const name = first.path.split(/[\\/]/).pop();
      showNotice(
        failed.length === 1
          ? `Couldn't ${verb.toLowerCase()} "${name}": ${first.error}`
          : `${failed.length} of ${results.length} items failed. First: "${name}" — ${first.error}`,
        true,
      );
    }
  }

  function beginRename(entry: DirEntry) {
    if (blockedInArchive()) return;
    renamingPath = entry.path;
    renameValue = entry.name;
  }

  /** Open the batch-rename dialog for the current multi-selection (CPE-255). */
  function beginBatchRename() {
    if (blockedInArchive() || selectedEntries.length < 2) return;
    batchRenameFor = selectedEntries;
  }

  /** Apply a batch rename: one move_exact within the current folder, pushed as a
      single undoable step (CPE-255). */
  async function applyBatchRename(items: RenameItem[]) {
    batchRenameFor = null;
    if (items.length === 0) return;
    const dir = currentPath;
    const pairs: [string, string][] = items.map((it) => [
      joinPath(dir, it.from),
      joinPath(dir, it.to),
    ]);
    try {
      const results = await invoke<OpResult[]>("move_exact", { pairs });
      reportResults(results, "Renamed");
      const moves = results
        .map((r, i) => ({ from: pairs[i][0], to: r.path, ok: r.ok }))
        .filter((m) => m.ok)
        .map(({ from, to }) => ({ from, to }));
      if (moves.length > 0) {
        undoStack = pushUndo(undoStack, {
          kind: "rename",
          moves,
          label: `Rename ${moves.length} item${moves.length === 1 ? "" : "s"}`,
        });
      }
      await loadPath(currentPath);
    } catch (e) {
      showNotice(String(e), true);
    }
  }

  async function commitRename(newName: string) {
    const path = renamingPath;
    renamingPath = "";
    if (!path) return;

    const entry = visible.find((e) => e.path === path);
    if (!entry || newName.trim() === "" || newName === entry.name) return;

    const invalid = validateFileName(newName);
    if (invalid) {
      showNotice(invalid, true);
      return;
    }

    try {
      const to = await invoke<string>("rename_entry", { path, newName });
      undoStack = pushUndo(undoStack, {
        kind: "rename",
        moves: [{ from: path, to }],
        label: `Rename to "${newName}"`,
      });
      await loadPath(currentPath);
    } catch (e) {
      showNotice(String(e), true);
    }
  }

  /** Undo the last rename or move. Copies and deletes are deliberately not
      undoable — see the comment at the top of lib/undo.ts. */
  async function undo() {
    const { entry, rest } = popUndo(undoStack);
    if (!entry) {
      showNotice("Nothing to undo.");
      return;
    }
    try {
      let results: OpResult[];

      if (entry.kind === "delete") {
        // Only ever pushed onto the stack when the platform can restore, so we
        // never reach here on macOS.
        results = await invoke<OpResult[]>("restore_from_trash", {
          paths: deletedPaths(entry),
        });
      } else {
        const pairs = invert(entry).map((m) => [m.from, m.to] as [string, string]);
        results = await invoke<OpResult[]>("move_exact", { pairs });
      }

      const failed = results.filter((r) => !r.ok);
      if (failed.length > 0) {
        // Do NOT pop the entry on failure — the user can retry once they've
        // cleared whatever is in the way.
        showNotice(`Couldn't undo: ${failed[0].error}`, true);
        return;
      }
      undoStack = rest;
      showNotice(`Undone: ${entry.label}`);
      await loadPath(currentPath);
    } catch (e) {
      showNotice(String(e), true);
    }
  }

  async function newFolder() {
    if (isHome || blockedInArchive()) return;
    try {
      const name = uniqueName("New folder", entries.map((e) => e.name));
      const created = await invoke<string>("create_dir", {
        path: currentPath,
        name,
      });
      pendingRenamePath = created; // select + inline-rename it once the list reloads
      await loadPath(currentPath);
    } catch (e) {
      showNotice(String(e), true);
    }
  }

  async function newFile() {
    if (isHome || blockedInArchive()) return;
    try {
      const name = uniqueNameWithExt(
        "New Text Document",
        ".txt",
        entries.map((e) => e.name),
      );
      const created = await invoke<string>("create_file", {
        path: currentPath,
        name,
      });
      pendingRenamePath = created; // select + inline-rename it once the list reloads
      await loadPath(currentPath);
    } catch (e) {
      showNotice(String(e), true);
    }
  }

  function doCopy() {
    if (blockedInArchive() || selectedEntries.length === 0) return;
    clipboard = stage(selectedEntries.map((e) => e.path), "copy");
    showNotice(`Copied ${clipboard.paths.length} item${clipboard.paths.length === 1 ? "" : "s"}.`);
  }

  function doCut() {
    if (blockedInArchive() || selectedEntries.length === 0) return;
    clipboard = stage(selectedEntries.map((e) => e.path), "cut");
    showNotice(`Cut ${clipboard.paths.length} item${clipboard.paths.length === 1 ? "" : "s"}.`);
  }

  /** Browse to a folder via the native picker and navigate there (CPE-366) — avoids
      hand-typing a deep path in the address bar. */
  async function browseForFolder() {
    let dest: string | string[] | null;
    try {
      dest = await openFolderDialog({
        directory: true,
        multiple: false,
        defaultPath: isHome ? undefined : currentPath,
        title: "Go to folder…",
      });
    } catch {
      return; // dialog unavailable / errored — no-op
    }
    if (!dest || typeof dest !== "string") return; // cancelled
    if (archive) exitArchive();
    navigate(dest);
  }

  /** Copy or move the selection into a folder chosen from the native picker (CPE-355) —
      no cut/navigate/paste dance. A move leaves the current folder, so it reloads and is
      undoable; a copy only reloads when the destination is the folder in view. */
  async function copyMoveToFolder(move: boolean) {
    if (isHome || archive || selectedEntries.length === 0) return;
    const sources = selectedEntries.map((e) => e.path);
    const n = sources.length;
    let dest: string | string[] | null;
    try {
      dest = await openFolderDialog({
        directory: true,
        multiple: false,
        defaultPath: currentPath,
        title: `${move ? "Move" : "Copy"} ${n} item${n === 1 ? "" : "s"} to…`,
      });
    } catch {
      return; // dialog unavailable / errored — no-op
    }
    if (!dest || typeof dest !== "string") return; // cancelled
    try {
      const results = await invoke<OpResult[]>(move ? "move_entries" : "copy_entries", {
        paths: sources,
        dest,
      });
      reportResults(results, move ? "Moved" : "Copied");
      if (move) {
        const moves = results
          .map((r, i) => ({ from: sources[i], to: r.path, ok: r.ok }))
          .filter((m) => m.ok)
          .map(({ from, to }) => ({ from, to }));
        if (moves.length > 0) {
          undoStack = pushUndo(undoStack, {
            kind: "move",
            moves,
            label: `Move ${moves.length} item${moves.length === 1 ? "" : "s"}`,
          });
        }
        await loadPath(currentPath);
      } else if (dest === currentPath) {
        await loadPath(currentPath);
      }
    } catch (e) {
      showNotice(String(e), true);
    }
  }

  async function doPaste() {
    if (isHome || blockedInArchive() || clipEmpty(clipboard)) return;
    if (!pasteCheck.allowed) {
      showNotice(pasteCheck.reason, true);
      return;
    }
    const wasCut = clipboard.mode === "cut";
    const sources = [...clipboard.paths];
    const cmd = wasCut ? "move_entries" : "copy_entries";
    try {
      const results = await invoke<OpResult[]>(cmd, {
        paths: sources,
        dest: currentPath,
      });
      reportResults(results, wasCut ? "Moved" : "Copied");

      // Only a MOVE is undoable. A copy is not — undoing it would mean deleting
      // the new file, which is a destructive act to reverse a harmless one.
      if (wasCut) {
        const moves = results
          .map((r, i) => ({ from: sources[i], to: r.path, ok: r.ok }))
          .filter((m) => m.ok)
          .map(({ from, to }) => ({ from, to }));
        if (moves.length > 0) {
          undoStack = pushUndo(undoStack, {
            kind: "move",
            moves,
            label: `Move ${moves.length} item${moves.length === 1 ? "" : "s"}`,
          });
        }
        clipboard = emptyClipboard();
      }
      await loadPath(currentPath);
    } catch (e) {
      showNotice(String(e), true);
    }
  }

  /** Fetch a text file's contents for the preview pane (size-capped backend). */
  function loadPreviewText(path: string): Promise<string> {
    return invoke<string>("read_file_text", { path, maxBytes: PREVIEW_MAX_BYTES });
  }

  /** List an archive's entries for the preview pane. */
  function loadArchiveEntries(path: string): Promise<ArchiveEntry[]> {
    return invoke<ArchiveEntry[]>("read_archive_entries", { path });
  }

  /** Read a read-only text summary of a binary file for the preview pane. */
  function loadPreviewInfo(path: string): Promise<string> {
    return invoke<string>("read_preview_info", { path });
  }

  /** Decode a non-native image (TIFF/PSD) to a data: URL for the preview pane. */
  function loadImageData(path: string): Promise<string> {
    return invoke<string>("read_image_data_url", { path });
  }

  /** Save edited text back to a file, then refresh so size/date update. */
  async function savePreviewText(path: string, contents: string): Promise<void> {
    await invoke("write_file_text", { path, contents });
    await loadPath(currentPath);
  }

  /** Copy the selection's full path(s) to the OS clipboard, quoted, one per
      line — Explorer's "Copy as path". */
  async function doCopyPath() {
    if (selectedEntries.length === 0) return;
    const text = formatPathsForClipboard(selectedEntries.map((e) => e.path));
    try {
      await navigator.clipboard.writeText(text);
      showNotice(`Copied path${selectedEntries.length === 1 ? "" : "s"} to the clipboard.`);
    } catch {
      showNotice("Couldn't copy the path to the clipboard.", true);
    }
  }

  /** Copy just the selected item's name to the clipboard (CPE-248). */
  async function doCopyName() {
    const entry = selectedEntries[0];
    if (!entry) return;
    try {
      await navigator.clipboard.writeText(entry.name);
      showNotice(`Copied "${entry.name}".`);
    } catch {
      showNotice("Couldn't copy the name to the clipboard.", true);
    }
  }

  /** Reveal the selected item (or the current folder) in the OS file manager (CPE-247). */
  async function revealInExplorer() {
    const target = selectedEntries.length === 1 ? selectedEntries[0].path : (isHome ? "" : currentPath);
    if (!target) return;
    try {
      await revealItemInDir(target);
    } catch {
      showNotice("Couldn't reveal that in the file manager.", true);
    }
  }

  /** Open the OS terminal with its working directory set to `path` (CPE-253). */
  async function openTerminal(path: string) {
    if (isHome || archive || !path) return;
    try {
      await invoke("open_terminal", { path });
    } catch {
      showNotice("Couldn't open a terminal here.", true);
    }
  }

  /** Pin/unpin the selected folder in the Home view (CPE-249). */
  function togglePinSelected() {
    const entry = selectedEntries[0];
    if (!entry?.is_dir) return;
    const wasPinned = pins.includes(entry.path);
    pins = settings.togglePin(pins, entry.path);
    settings.savePins(pins);
    showNotice(wasPinned ? `Unpinned "${entry.name}" from Home.` : `Pinned "${entry.name}" to Home.`);
  }

  /** "Work on this" — open the AI Console scoped to the selection (CPE-313). A single
      folder scopes to itself; files scope to the current folder with a task naming them;
      no selection just opens the current folder. Degrades cleanly when the console is
      absent (launchAiConsole shows a notice). */
  function openSelectionInConsole() {
    if (isHome || archive) { openAiConsole(); return; }
    const sel = selectedEntries;
    if (sel.length === 1 && sel[0].is_dir) {
      openAiConsole({ cwd: sel[0].path, task: `Work in the folder "${sel[0].name}".` });
    } else if (sel.length >= 1) {
      openAiConsole({ cwd: currentPath, task: `Work on: ${sel.map((e) => e.name).join(", ")}` });
    } else {
      openAiConsole({ cwd: currentPath });
    }
  }

  /** Star/unstar the single selected item (file or folder) as a Favorite (CPE-338). */
  function toggleFavoriteSelected() {
    const entry = selectedEntries[0];
    if (!entry) return;
    const wasFav = favorites.some((f) => f.path === entry.path);
    favorites = settings.toggleFavorite(favorites, {
      path: entry.path,
      name: entry.name,
      is_dir: entry.is_dir,
    });
    settings.saveFavorites(favorites);
    showNotice(wasFav ? `Removed "${entry.name}" from Favorites.` : `Added "${entry.name}" to Favorites.`);
  }

  /** Duplicate the selection in place — copy it into the folder it lives in.
      Not undoable, for the same reason a copy-paste isn't (see doPaste). */
  async function doDuplicate() {
    if (isHome || blockedInArchive() || selectedEntries.length === 0) return;
    const sources = selectedEntries.map((e) => e.path);
    try {
      const results = await invoke<OpResult[]>("copy_entries", {
        paths: sources,
        dest: currentPath,
      });
      reportResults(results, "Duplicated");
      await loadPath(currentPath);
    } catch (e) {
      showNotice(String(e), true);
    }
  }

  /** Compare exactly two selected files for byte-identical content (CPE-418) → a notice. */
  async function compareFiles() {
    if (selectedEntries.length !== 2 || selectedEntries.some((e) => e.is_dir)) return;
    const [a, b] = selectedEntries;
    try {
      const same = await invoke<boolean>("files_identical", { a: a.path, b: b.path });
      showNotice(
        same
          ? `"${a.name}" and "${b.name}" are identical.`
          : `"${a.name}" and "${b.name}" differ.`,
      );
    } catch (e) {
      showNotice(String(e), true);
    }
  }

  /** Join a directory and a leaf name using the directory's own separator. */
  function joinPath(dir: string, name: string): string {
    const sep = dir.includes("\\") ? "\\" : "/";
    return dir.endsWith(sep) ? dir + name : dir + sep + name;
  }

  /** File name minus its final extension ("report.docx" -> "report"). A leading
      dot (dotfiles) is kept, and a name with no dot is returned unchanged. */
  function stripExt(name: string): string {
    const i = name.lastIndexOf(".");
    return i > 0 ? name.slice(0, i) : name;
  }

  /** A friendly base name for an archive, stripping the compound archive suffix
      ("bundle.tar.gz" -> "bundle", "photos.zip" -> "photos"). */
  function archiveBaseName(name: string): string {
    const lower = name.toLowerCase();
    if (lower.endsWith(".tar.gz")) return name.slice(0, -7);
    if (lower.endsWith(".tar.bz2")) return name.slice(0, -8);
    return stripExt(name);
  }

  /** Compress the selection into a new .zip in the current folder (CPE-251). */
  async function doCompress() {
    if (isHome || blockedInArchive() || selectedEntries.length === 0) return;
    const base =
      selectedEntries.length === 1
        ? selectedEntries[0].is_dir
          ? selectedEntries[0].name
          : stripExt(selectedEntries[0].name)
        : "Archive";
    const name = uniqueNameWithExt(base, ".zip", entries.map((e) => e.name));
    const dest = joinPath(currentPath, name);
    const n = selectedEntries.length;
    try {
      const created = await invoke<string>("compress_to_zip", {
        paths: selectedEntries.map((e) => e.path),
        dest,
      });
      pendingSelectPath = created;
      showNotice(`Compressed ${n} item${n === 1 ? "" : "s"} to "${name}".`);
      await loadPath(currentPath);
    } catch (e) {
      showNotice(String(e), true);
    }
  }

  /** Extract the selected archive into a new subfolder of the current folder
      (CPE-252). Named after the archive, auto-numbered to avoid collisions. */
  async function doExtract() {
    if (isHome || blockedInArchive()) return;
    const entry = selectedEntries[0];
    if (selectedEntries.length !== 1 || !entry || !isExtractable(entry)) return;
    const name = uniqueName(archiveBaseName(entry.name), entries.map((e) => e.name));
    const dest = joinPath(currentPath, name);
    try {
      await invoke<string>("extract_archive", { path: entry.path, dest });
      pendingSelectPath = dest;
      showNotice(`Extracted "${entry.name}" to "${name}".`);
      await loadPath(currentPath);
    } catch (e) {
      showNotice(String(e), true);
    }
  }

  /** Move `paths` into `dest` (drag & drop). Ctrl-drag copies instead. */
  async function dropInto(paths: string[], dest: string, copy: boolean) {
    if (paths.length === 0 || !dest) return;

    // A folder can never be dropped into itself or its own descendant.
    for (const p of paths) {
      if (clipCanPaste(stage([p], copy ? "copy" : "cut"), dest).allowed === false) {
        const check = clipCanPaste(stage([p], copy ? "copy" : "cut"), dest);
        // "already in this folder" is a no-op, not an error worth shouting about.
        if (check.reason.includes("itself")) {
          showNotice(check.reason, true);
          return;
        }
        return;
      }
    }

    try {
      const results = await invoke<OpResult[]>(
        copy ? "copy_entries" : "move_entries",
        { paths, dest },
      );
      reportResults(results, copy ? "Copied" : "Moved");
      if (!copy) {
        const moves = results
          .map((r, i) => ({ from: paths[i], to: r.path, ok: r.ok }))
          .filter((m) => m.ok)
          .map(({ from, to }) => ({ from, to }));
        if (moves.length > 0) {
          undoStack = pushUndo(undoStack, {
            kind: "move",
            moves,
            label: `Move ${moves.length} item${moves.length === 1 ? "" : "s"}`,
          });
        }
      }
      await loadPath(currentPath);
    } catch (e) {
      showNotice(String(e), true);
    }
  }

  function askDelete(permanent: boolean) {
    if (blockedInArchive() || selectedEntries.length === 0) return;
    const n = selectedEntries.length;
    const what = n === 1 ? `"${selectedEntries[0].name}"` : `${n} items`;

    if (!permanent) {
      // Recycle bin is recoverable, so no modal — just do it and say so.
      doDelete(false);
      return;
    }
    confirm = {
      title: "Delete permanently?",
      message: `${what} will be permanently deleted. This cannot be undone and does not go to the Recycle Bin.`,
      label: "Delete permanently",
      onYes: () => doDelete(true),
    };
  }

  async function doDelete(permanent: boolean) {
    confirm = null;
    const paths = selectedEntries.map((e) => e.path);
    if (paths.length === 0) return;
    try {
      const results = await invoke<OpResult[]>(
        permanent ? "delete_permanent" : "delete_to_trash",
        { paths },
      );
      reportResults(results, permanent ? "Permanently deleted" : "Moved to Recycle Bin:");

      // A trashed delete is undoable — but ONLY where the platform can actually
      // restore. On macOS `canRestoreTrash` is false, so we don't push it, and
      // Ctrl+Z will offer whatever came before instead of a button that lies.
      // A permanent delete is never undoable, anywhere.
      if (!permanent && canRestoreTrash) {
        const restored = results
          .filter((r) => r.ok)
          .map((r) => ({ from: r.path, to: "" }));
        if (restored.length > 0) {
          undoStack = pushUndo(undoStack, {
            kind: "delete",
            moves: restored,
            label: `Delete ${restored.length} item${restored.length === 1 ? "" : "s"}`,
          });
        }
      }
      await loadPath(currentPath);
    } catch (e) {
      showNotice(String(e), true);
    }
  }

  function openProperties() {
    if (selectedEntries.length === 0) return;
    propsFor = selectedEntries;
  }

  /** Run the selected executable normally (CPE-241) — same shell open as double-click. */
  async function executeSelected() {
    const entry = selectedEntries[0];
    if (!entry || !isExecutable(entry)) return;
    try {
      await invoke("open_external", { path: entry.path });
    } catch {
      showNotice(`Couldn't run "${entry.name}".`, true);
    }
  }

  /** Run the selected executable elevated (UAC prompt on Windows) (CPE-241). */
  async function executeAsAdmin() {
    const entry = selectedEntries[0];
    if (!entry || !isExecutable(entry)) return;
    try {
      await invoke("run_as_admin", { path: entry.path });
    } catch {
      showNotice(`Couldn't run "${entry.name}" as administrator.`, true);
    }
  }

  // ---- context menu / command dispatch ----
  function runAction(action: string) {
    switch (action) {
      case "open": if (selectedEntries[0]) open(selectedEntries[0]); break;
      case "execute": executeSelected(); break;
      case "execute-admin": executeAsAdmin(); break;
      case "open-new-tab": if (selectedEntries[0]) openInNewTab(selectedEntries[0]); break;
      case "cut": doCut(); break;
      case "copy": doCopy(); break;
      case "paste": doPaste(); break;
      case "duplicate": doDuplicate(); break;
      case "compare": compareFiles(); break;
      case "compress": doCompress(); break;
      case "extract": doExtract(); break;
      case "copy-path": doCopyPath(); break;
      case "copy-name": doCopyName(); break;
      case "reveal": revealInExplorer(); break;
      case "terminal": openTerminal(currentPath); break;
      case "terminal-folder": if (selectedEntries[0]?.is_dir) openTerminal(selectedEntries[0].path); break;
      case "pin": togglePinSelected(); break;
      case "favorite": toggleFavoriteSelected(); break;
      case "open-in-console": openSelectionInConsole(); break;
      case "copy-to": copyMoveToFolder(false); break;
      case "move-to": copyMoveToFolder(true); break;
      case "open-folder-in-console": if (!isHome && !archive) openAiConsole({ cwd: currentPath }); break;
      case "rename": if (selectedEntries.length === 1) beginRename(selectedEntries[0]); break;
      case "batch-rename": beginBatchRename(); break;
      case "delete": askDelete(false); break;
      case "properties": openProperties(); break;
      case "new-folder": newFolder(); break;
      case "new-file": newFile(); break;
      case "select-all": selection = selectAll(visible.length); break;
      case "invert-selection": selection = invertSelection(selection, visible.length); break;
      case "select-pattern": patternSelectOpen = true; break;
      case "select-type": {
        const e = selectedEntries[0];
        if (e && !e.is_dir) selection = selectIndices(sameTypeIndices(visible, e.extension));
        break;
      }
      case "refresh": refresh(); break;
    }
  }

  function onRowContext(e: { x: number; y: number; index: number }) {
    // Right-clicking an unselected row selects it first, as Explorer does.
    if (!selection.indices.has(e.index)) selection = selectOnly(e.index);
    ctx = { x: e.x, y: e.y, target: "item" };
  }

  // ---- keyboard ----
  function handleKeydown(event: KeyboardEvent) {
    const target = event.target as HTMLElement | null;
    // Never hijack keys while typing in an editor, the path bar, or search.
    if (target && ["INPUT", "TEXTAREA"].includes(target.tagName)) return;
    if (renamingPath) return;

    const ctrl = event.ctrlKey || event.metaKey;

    if (ctrl && event.key.toLowerCase() === "l") { event.preventDefault(); editingPath = true; return; }
    if (event.altKey && event.key.toLowerCase() === "d") { event.preventDefault(); editingPath = true; return; }
    if (ctrl && event.key.toLowerCase() === "f") { event.preventDefault(); navToolbar?.focusSearch(); return; }
    if (ctrl && event.shiftKey && event.key.toLowerCase() === "n") { event.preventDefault(); newFolder(); return; }
    if (ctrl && event.shiftKey && event.key.toLowerCase() === "o") { event.preventDefault(); popOutPreview(); return; }
    if (ctrl && event.shiftKey && event.key.toLowerCase() === "t") { event.preventDefault(); reopenClosedTab(); return; }
    if (ctrl && event.shiftKey && event.key.toLowerCase() === "f") { event.preventDefault(); if (!isHome && !archive) contentSearchOpen = true; return; }
    if (ctrl && event.key.toLowerCase() === "t") { event.preventDefault(); newTab(); return; }
    if (ctrl && event.key.toLowerCase() === "w") { event.preventDefault(); closeTab(activeId); return; }
    if (ctrl && event.key === "Tab") { event.preventDefault(); cycleTab(event.shiftKey ? -1 : 1); return; }
    if (ctrl && event.key.toLowerCase() === "a") { event.preventDefault(); selection = selectAll(visible.length); return; }
    if (ctrl && event.shiftKey && event.key.toLowerCase() === "c") { event.preventDefault(); doCopyPath(); return; }
    // Don't hijack Ctrl+C when text is selected (e.g. in the Preview Pane) — let the browser copy it.
    if (ctrl && event.key.toLowerCase() === "c" && !(window.getSelection()?.isCollapsed ?? true)) return;
    if (ctrl && event.key.toLowerCase() === "c") { event.preventDefault(); doCopy(); return; }
    if (ctrl && event.key.toLowerCase() === "x") { event.preventDefault(); doCut(); return; }
    if (ctrl && event.key.toLowerCase() === "v") { event.preventDefault(); doPaste(); return; }
    if (ctrl && event.key.toLowerCase() === "d") { event.preventDefault(); doDuplicate(); return; }
    if (ctrl && event.key.toLowerCase() === "z") { event.preventDefault(); undo(); return; }

    if (event.altKey && event.key === "ArrowLeft") { event.preventDefault(); goBack(); return; }
    if (event.altKey && event.key === "ArrowRight") { event.preventDefault(); goForward(); return; }
    if (event.altKey && event.key === "ArrowUp") { event.preventDefault(); goUp(); return; }
    if (event.altKey && event.key === "Enter") { event.preventDefault(); openProperties(); return; }
    if (event.altKey && event.key.toLowerCase() === "p") {
      event.preventDefault();
      showDetails = !showDetails;
      settings.saveShowDetails(showDetails);
      return;
    }

    // Type-ahead find: a printable key with no modifier jumps to the next match.
    if (!ctrl && !event.altKey && event.key.length === 1 && /\S/.test(event.key)) {
      event.preventDefault();
      const now = Date.now();
      const continuing = now - typeAheadAt <= 700;
      typeAheadBuffer = continuing ? typeAheadBuffer + event.key : event.key;
      typeAheadAt = now;
      const single = typeAheadBuffer.length === 1;
      const idx = firstMatchIndex(
        visible.map((e) => e.name),
        typeAheadBuffer,
        selection.lead,
        single,
      );
      if (idx >= 0) selection = selectOnly(idx);
      return;
    }

    switch (event.key) {
      case "F1": event.preventDefault(); shortcutsOpen = true; break;
      case "F5": event.preventDefault(); refresh(); break;
      case "F2":
        event.preventDefault();
        if (selectedEntries.length === 1) beginRename(selectedEntries[0]);
        break;
      case "Delete":
        event.preventDefault();
        askDelete(event.shiftKey); // Shift+Del = permanent, and is confirmed
        break;
      case "Escape":
        selection = emptySelection();
        ctx = null;
        break;
      case "ArrowDown":
        event.preventDefault();
        selection = moveLead(selection, 1, visible.length, event.shiftKey);
        break;
      case "ArrowUp":
        event.preventDefault();
        selection = moveLead(selection, -1, visible.length, event.shiftKey);
        break;
      case "Home":
        event.preventDefault();
        selection = moveLead(selection, -visible.length, visible.length, event.shiftKey);
        break;
      case "End":
        event.preventDefault();
        selection = moveLead(selection, visible.length, visible.length, event.shiftKey);
        break;
      case "Enter":
        if (target?.closest(".row")) return;
        event.preventDefault();
        if (selectedEntries[0]) open(selectedEntries[0]);
        break;
      case "Backspace":
        event.preventDefault();
        goUp();
        break;
    }
  }

  /** Pull every preference from the settings store into the reactive UI vars. */
  function applySettings() {
    view = settings.loadView();
    showHidden = settings.loadShowHidden();
    foldersFirst = settings.loadFoldersFirst();
    sortKey = settings.loadSortKey();
    sortDir = settings.loadSortDir();
    showDetails = settings.loadShowDetails();
    showPreview = settings.loadShowPreview();
    sidebarWidth = clampWidth(settings.loadSidebarWidth(), SIDEBAR_MIN, SIDEBAR_MAX);
    rightWidth = clampWidth(settings.loadRightWidth(), RIGHT_MIN, RIGHT_MAX);
    pins = settings.loadPins();
    recents = settings.loadRecents();
    favorites = settings.loadFavorites();
    recentFolders = settings.loadRecentFolders();
    columnWidths = settings.loadColumnWidths();
  }

  /** Record a successfully-opened folder in the recently-visited MRU (CPE-342). */
  function recordRecentFolder(path: string) {
    const name = path.split(/[\\/]/).filter(Boolean).pop() ?? path;
    recentFolders = settings.addRecent(recentFolders, { path, name });
    settings.saveRecentFolders(recentFolders);
  }

  /** App-level Settings gear: restore every preference to its default. */
  function resetAllSettings() {
    settings.resetSettings();
    applySettings();
  }

  /** File > Exit — quit the whole app (process:default grants allow-exit). */
  async function exitApp() {
    await exit(0);
  }

  /** Tear off the current preview into the floating window (CPE-234). Pinned to
      the file; the in-app pane keeps following the selection. A second pop-out
      docks as another tab in the same window (created once, label "preview-float"). */
  const FLOAT_LABEL = "preview-float";
  async function popOutPreview() {
    const entry = selectedEntries.length === 1 ? selectedEntries[0] : null;
    if (!entry) {
      showNotice("Select a single file first, then pop its preview out.", true);
      return;
    }
    try {
      let win = await WebviewWindow.getByLabel(FLOAT_LABEL);
      if (!win) {
        // Register the readiness wait BEFORE creating the window so we can't miss it.
        const ready = new Promise<void>((resolve) => {
          let done = false;
          const finish = () => { if (!done) { done = true; resolve(); } };
          void once("float:ready", finish);
          setTimeout(finish, 2500); // fallback so a slow load never hangs the pop-out
        });
        win = new WebviewWindow(FLOAT_LABEL, {
          url: "index.html?float=1",
          title: "Preview",
          width: 480,
          height: 640,
          minWidth: 320,
          minHeight: 300,
        });
        await ready;
      }
      await emit("float:add", entry);
      await win.setFocus();
    } catch (e) {
      console.debug("pop out failed:", e);
      showNotice("Couldn't open the preview in a new window.", true);
    }
  }

  /** Route a menu selection to its action. See MenuBar's `menus` table. */
  function onMenuSelect(action: string) {
    switch (action) {
      case "exit": exitApp(); break;
      case "check-updates": checkForUpdates(true); break;
      case "settings": showSettings = true; break;
      case "shortcuts": shortcutsOpen = true; break;
      case "documentation": openExternal(REPO_URL); break;
      case "about": showAbout = true; break;
      case "content-search": if (!isHome && !archive) contentSearchOpen = true; break;
      case "find-duplicates": if (!isHome && !archive) duplicatesOpen = true; break;
      case "copy-file-names": copyListing(namesList(visible), "file names"); break;
      case "copy-file-list": copyListing(detailList(visible), "file list"); break;
      case "save-file-list": saveFileList(); break;
    }
  }

  /** Save the current (visible) folder listing to a CSV/TXT file via a native Save dialog (CPE-425). */
  async function saveFileList() {
    if (isHome || visible.length === 0) {
      showNotice("Nothing to save here.");
      return;
    }
    try {
      const path = await saveFileDialog({
        defaultPath: "file-list.csv",
        filters: [
          { name: "CSV", extensions: ["csv"] },
          { name: "Text", extensions: ["txt"] },
        ],
      });
      if (!path) return; // cancelled
      const text = path.toLowerCase().endsWith(".txt") ? detailList(visible) : csvList(visible);
      await invoke("write_file_text", { path, contents: text });
      showNotice(`Saved ${visible.length} rows to ${path.split(/[\\/]/).pop()}.`);
    } catch (e) {
      showNotice(String(e), true);
    }
  }

  /** Copy the current (visible) folder listing to the clipboard as text (CPE-422). */
  async function copyListing(text: string, what: string) {
    if (isHome || visible.length === 0) {
      showNotice("Nothing to copy here.");
      return;
    }
    try {
      await navigator.clipboard.writeText(text);
      showNotice(`Copied ${visible.length} ${what === "file names" ? "name" : "row"}${visible.length === 1 ? "" : "s"} to the clipboard.`);
    } catch (e) {
      showNotice(String(e), true);
    }
  }

  // Drag-the-pane-header-to-pop-out (CPE-238): true cross-window drag isn't
  // possible in a webview, so a drag gesture on the preview header just triggers
  // the same pop-out as the button. A plain click (no movement) is unaffected.
  let previewHeaderDrag: { x: number; y: number } | null = null;
  function onPreviewHeaderDown(e: PointerEvent) {
    if (selectedEntries.length !== 1) return;
    previewHeaderDrag = { x: e.clientX, y: e.clientY };
  }
  function onPreviewHeaderMove(e: PointerEvent) {
    if (!previewHeaderDrag) return;
    if (Math.hypot(e.clientX - previewHeaderDrag.x, e.clientY - previewHeaderDrag.y) > 24) {
      previewHeaderDrag = null;
      popOutPreview();
    }
  }
  function endPreviewHeaderDrag() {
    previewHeaderDrag = null;
  }

  /** Run a folder-context action (CPE-235): open a marker file, or open the
      repo's GitHub/remote page (resolved from .git/config by the backend). */
  async function handleContextAction(a: FolderAction) {
    try {
      if (a.kind === "open-path") {
        await invoke("open_external", { path: a.target });
        showNotice(`${a.label}…`);
      } else if (a.kind === "open-github") {
        const url = await invoke<string | null>("git_remote_url", { path: a.target });
        if (url) await openUrl(url);
        else showNotice("This repository has no remote URL configured.", true);
      }
    } catch (e) {
      console.debug("context action failed:", e);
      showNotice("Couldn't run that action.", true);
    }
  }

  /** Open a URL in the default browser, surfacing failures rather than swallowing. */
  async function openExternal(url: string) {
    try {
      await openUrl(url);
    } catch {
      showNotice("Couldn't open the link.", true);
    }
  }

  /** Check the signed manifest for a newer version. On startup this runs quietly
      (`manual=false`): silence when up to date, a prompt when there's an update —
      never a silent auto-install. From the Application menu (`manual=true`) it
      also reports "up to date" and surfaces errors. Nothing installs here. */
  async function checkForUpdates(manual = false) {
    // A manual check always opens the dialog so it never feels like nothing
    // happened: "Checking…" → available / up to date / error (CPE-231). The
    // silent startup check (manual=false) stays quiet unless an update exists.
    if (manual) {
      pendingUpdate = null;
      updateError = "";
      updateProgress = 0;
      updateIndeterminate = false;
      updateState = "checking";
      showUpdate = true;
    }
    try {
      const update = await check();
      if (update) {
        pendingUpdate = update;
        updateProgress = 0;
        updateIndeterminate = false;
        updateError = "";
        updateState = "available";
        showUpdate = true;
      } else if (manual) {
        updateState = "uptodate";
      }
    } catch (e) {
      console.debug("update check failed:", e);
      if (manual) {
        updateState = "error";
        updateError = "Couldn't check for updates right now. Check your connection and try again.";
      }
    }
  }

  /** Download + install the pending update with progress, then relaunch. Only
      ever called when the user clicks Install & Restart. */
  async function installUpdate() {
    if (!pendingUpdate) return;
    updateState = "downloading";
    updateProgress = 0;
    updateError = "";

    let total = 0;
    let downloaded = 0;
    updateIndeterminate = false;

    try {
      await pendingUpdate.downloadAndInstall((event) => {
        switch (event.event) {
          case "Started":
            total = event.data.contentLength ?? 0;
            updateIndeterminate = total === 0; // server didn't send a length
            break;
          case "Progress":
            downloaded += event.data.chunkLength;
            if (total > 0) {
              updateProgress = Math.min(100, Math.round((downloaded / total) * 100));
            }
            break;
          case "Finished":
            updateProgress = 100;
            updateIndeterminate = false;
            break;
        }
      });
      // Installed — restart into the new version. relaunch replaces this process.
      await relaunch();
    } catch (e) {
      console.debug("update install failed:", e);
      updateState = "error";
      updateError = "The update couldn't be installed. Please try again later.";
    }
  }

  /** "Later" / "Close" — dismiss the dialog; any pending update stays available. */
  function dismissUpdate() {
    showUpdate = false;
  }

  /** "Try Again" — retry the install if an update is pending, else re-check. */
  function retryUpdate() {
    if (pendingUpdate) installUpdate();
    else checkForUpdates(true);
  }

  onMount(async () => {
    applySettings();
    // Reveal the AI Console button only when the sidecar platform is present (CPE-351).
    platformActive().then((v) => (aiConsoleAvailable = v)).catch(() => {});
    // Listen for coding-agent sessions launched from the console so the explorer can surface
    // them (Agent Watch, CPE-396). Idle until a session announces itself; unlistened on teardown.
    initAgentSessions().then((un) => (unlistenSessions = un)).catch(() => {});

    try {
      const [p, d, h, canRestore] = await Promise.all([
        invoke<Place[]>("special_folders"),
        invoke<Place[]>("list_drives"),
        invoke<string>("home_dir"),
        invoke<boolean>("can_restore_from_trash"),
      ]);
      places = p;
      drives = d;
      homePath = h;
      canRestoreTrash = canRestore;
      loadDriveUsage(d); // fire-and-forget: sidebar usage bars (CPE-406)
    } catch (e) {
      console.debug("could not load places:", e);
    }
    try {
      appVersion = await getVersion();
    } catch {
      // Version is cosmetic (About dialog) — a failure must not break startup.
    }

    await loadPath(HOME);
    checkForUpdates();

    // Auto-mirror scheduler (CPE-497): a 60s tick + a window-focus check. Both funnel through
    // maybeAutoSync, which no-ops unless the current repo opted in and its interval has elapsed.
    autoMirrorTimer = setInterval(maybeAutoSync, 60_000);
    window.addEventListener("focus", maybeAutoSync);
  });

  onDestroy(() => {
    unlistenSessions?.();
    unlistenActivity?.();
    if (watchRefreshTimer) clearTimeout(watchRefreshTimer);
    if (autoMirrorTimer) clearInterval(autoMirrorTimer);
    window.removeEventListener("focus", maybeAutoSync);
  });
</script>

<svelte:window on:keydown={handleKeydown} />

<MenuBar on:select={(e) => onMenuSelect(e.detail)} />

<Toolbar label={$t("tb.application")}>
  <svelte:fragment slot="actions">
    {#if aiConsoleAvailable}
      <button
        class="tb-console"
        type="button"
        title={$agentSessions.length === 0
          ? $t("tb.openConsole")
          : $agentSessions.length === 1
            ? $t("tb.openConsoleOne")
            : $t("tb.openConsoleMany", { count: $agentSessions.length })}
        on:click={() => openAiConsole()}
        on:contextmenu|preventDefault={(e) => (agentMenu = { x: e.clientX, y: e.clientY, label: $t("tb.closeAllConsoles") })}
      >
        <Icon name="code" size={15} /> {$t("tb.aiConsole")}
        {#if $agentSessions.length}
          <span class="tb-console-count" aria-label={$t("tb.agentsRunning", { count: $agentSessions.length })}>{$agentSessions.length}</span>
        {/if}
      </button>
    {/if}
  </svelte:fragment>
  <div class="settings-row">
    <span>{$t("tb.showDetailsPane")}</span>
    <input type="checkbox" bind:checked={showDetails}
      on:change={() => settings.saveShowDetails(showDetails)} />
  </div>
  <div class="settings-row">
    <span>{$t("cmd.showHidden")}</span>
    <input type="checkbox" bind:checked={showHidden}
      on:change={() => settings.saveShowHidden(showHidden)} />
  </div>
  <div class="settings-row">
    <button class="settings-btn" on:click={resetAllSettings}>{$t("tb.resetSettings")}</button>
  </div>
</Toolbar>

<TabBar
  tabs={tabList}
  {activeId}
  on:select={(e) => selectTab(e.detail)}
  on:close={(e) => closeTab(e.detail)}
  on:new={newTab}
  on:menu={(e) => (tabMenu = e.detail)}
/>

{#if tabMenu}
  <TabMenu
    x={tabMenu.x}
    y={tabMenu.y}
    hasOthers={tabs.length > 1}
    hasRight={tabs.findIndex((t) => t.id === tabMenu?.id) < tabs.length - 1}
    on:action={(e) => onTabMenuAction(e.detail)}
    on:close={() => (tabMenu = null)}
  />
{/if}

<NavToolbar
  bind:this={navToolbar}
  bind:editingPath
  {crumbs}
  {currentPath}
  recentPaths={recentFolders.map((r) => r.path)}
  canBack={canGoBack(activeTab.history)}
  canForward={canGoForward(activeTab.history)}
  {search}
  searchScope={folderName}
  on:back={goBack}
  on:forward={goForward}
  on:up={goUp}
  on:refresh={refresh}
  on:browse={browseForFolder}
  on:navigate={(e) => onCrumbNavigate(e.detail)}
  on:search={(e) => { search = e.detail; selection = emptySelection(); }}
/>

<CommandBar
  selectionCount={selectedCount(selection)}
  canPaste={pasteCheck.allowed}
  {showDetails}
  {showHidden}
  {sortKey}
  {sortDir}
  {view}
  {fileFilter}
  {foldersFirst}
  on:action={(e) => runAction(e.detail)}
  on:sort={(e) => {
    sortKey = e.detail.key; sortDir = e.detail.dir;
    settings.saveSortKey(sortKey); settings.saveSortDir(sortDir);
  }}
  on:view={(e) => { view = e.detail; settings.saveView(view); }}
  on:filter={(e) => (fileFilter = e.detail)}
  on:toggleHidden={() => { showHidden = !showHidden; settings.saveShowHidden(showHidden); }}
  on:toggleFoldersFirst={() => { foldersFirst = !foldersFirst; settings.saveFoldersFirst(foldersFirst); }}
  on:toggleDetails={() => { showDetails = !showDetails; settings.saveShowDetails(showDetails); }}
/>

<div
  class="main"
  class:with-details={showDetails}
  class:resizing
  style="grid-template-columns: {gridCols}"
>
  <div class="pane-col">
    <Toolbar label={$t("tb.navigation")}>
      <div class="settings-row">
        <span>{$t("tb.paneWidth")}</span>
        <input
          type="number"
          min={SIDEBAR_MIN}
          max={SIDEBAR_MAX}
          bind:value={sidebarWidth}
          on:change={() => {
            sidebarWidth = clampWidth(sidebarWidth, SIDEBAR_MIN, SIDEBAR_MAX);
            settings.saveSidebarWidth(sidebarWidth);
          }}
        />
      </div>
    </Toolbar>
    <Sidebar
      {places}
      {drives}
      {favorites}
      {driveUsage}
      sessions={$agentSessions}
      {currentPath}
      {isHome}
      selectedPath={selectedEntries.length === 1 && selectedEntries[0]?.is_dir ? selectedEntries[0].path : ""}
      {draggedPaths}
      on:navigate={(e) => { if (archive) exitArchive(); navigate(e.detail); }}
      on:openFile={(e) => openRecent(e.detail)}
      on:home={() => { if (archive) exitArchive(); navigate(HOME); }}
      on:repos={() => (showRepos = true)}
      on:agentMenu={(e) => (agentMenu = { x: e.detail.x, y: e.detail.y, label: $t("tb.closeAllConsoles"), sessionId: e.detail.sessionId, sessionLabel: e.detail.sessionLabel })}
      on:drop={(e) => dropInto(e.detail.paths, e.detail.dest, e.detail.copy)}
    />
  </div>

  <!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
  <div
    class="resizer"
    role="separator"
    aria-orientation="vertical"
    aria-label={$t("tb.resizeNav")}
    title={$t("tb.resizeTip")}
    on:mousedown={(e) => startResize("left", e)}
  ></div>

  <!-- File List Pane (middle column) -->
  <div class="pane-col">
   <Toolbar label={$t("tb.fileList")}>
    <div class="settings-row">
      <span>{$t("menu.view")}</span>
      <select bind:value={view} on:change={() => settings.saveView(view)}>
        <option value="details">{$t("view.details")}</option>
        <option value="list">{$t("view.list")}</option>
        <option value="icons">{$t("tb.icons")}</option>
      </select>
    </div>
    <div class="settings-row">
      <span>{$t("tb.sortBy")}</span>
      <select bind:value={sortKey} on:change={() => settings.saveSortKey(sortKey)}>
        <option value="name">{$t("sort.name")}</option>
        <option value="modified">{$t("tb.modified")}</option>
        <option value="type">{$t("sort.type")}</option>
        <option value="size">{$t("sort.size")}</option>
      </select>
    </div>
    <div class="settings-row">
      <span>{$t("tb.direction")}</span>
      <select bind:value={sortDir} on:change={() => settings.saveSortDir(sortDir)}>
        <option value="asc">{$t("cmd.ascending")}</option>
        <option value="desc">{$t("cmd.descending")}</option>
      </select>
    </div>
    <div class="settings-row">
      <span>{$t("cmd.showHidden")}</span>
      <input type="checkbox" bind:checked={showHidden}
        on:change={() => settings.saveShowHidden(showHidden)} />
    </div>
   </Toolbar>
   <ContextBar contexts={folderContexts} on:action={(e) => handleContextAction(e.detail)} />
   <div class="filelist-pane" role="region" aria-label={$t("tb.fileList")}>
    {#if isHome}
      <HomeView
        {places}
        {drives}
        {pins}
        {recents}
        {favorites}
        {recentFolders}
        on:navigate={(e) => navigate(e.detail)}
        on:openFile={(e) => openRecent(e.detail)}
        on:unpin={(e) => { pins = settings.togglePin(pins, e.detail); settings.savePins(pins); }}
        on:unfavorite={(e) => { favorites = favorites.filter((f) => f.path !== e.detail); settings.saveFavorites(favorites); }}
        on:removeRecent={(e) => { recents = settings.removeRecent(recents, e.detail); settings.saveRecents(recents); }}
        on:removeRecentFolder={(e) => { recentFolders = settings.removeRecent(recentFolders, e.detail); settings.saveRecentFolders(recentFolders); }}
        on:clearRecents={() => { recents = []; settings.saveRecents(recents); }}
      />
    {:else}
      {#if activeWatchCwd}
        <div class="agent-strip" role="status">
          <span class="agent-dot" />
          <span class="agent-strip-label">{$t("agent.watch", { name: watchedAgentName })}</span>
          {#each recentChanges as c (c.path)}
            <span class="agent-chip {c.kind}" title={c.path}>{c.kind === "removed" ? "−" : c.kind === "created" ? "+" : "~"} {baseNameOf(c.path)}</span>
          {/each}
          {#if recentChanges.length === 0}
            <span class="agent-strip-idle">{$t("agent.watching")}</span>
          {/if}
          <button class="agent-log-btn" on:click={() => (showTimeline = !showTimeline)} title={$t("agent.showLog")}>
            {$t("agent.log")} {$agentTimeline.length ? `(${$agentTimeline.length})` : ""}
          </button>
        </div>
      {/if}
      <FileList
        entries={visible}
        activity={activeWatchCwd ? $fsActivity : {}}
        {selection}
        {sortKey}
        {sortDir}
        {view}
        {error}
        {loading}
        {searching}
        {cutPaths}
        {renamingPath}
        {renameValue}
        assetUrl={convertFileSrc}
        {columnWidths}
        on:resizeColumns={(e) => { columnWidths = e.detail; settings.saveColumnWidths(columnWidths); }}
        bind:rowEls
        bind:draggedPaths
        on:click={(e) => (selection = selClick(selection, e.detail.index, e.detail))}
        on:open={(e) => open(e.detail)}
        on:sort={(e) => {
          sortKey = e.detail.key; sortDir = e.detail.dir;
          settings.saveSortKey(sortKey); settings.saveSortDir(sortDir);
        }}
        on:context={(e) => onRowContext(e.detail)}
        on:contextEmpty={(e) => (ctx = { x: e.detail.x, y: e.detail.y, target: "empty" })}
        on:commitRename={(e) => commitRename(e.detail)}
        on:cancelRename={() => (renamingPath = "")}
        on:drop={(e) => dropInto(e.detail.paths, e.detail.dest, e.detail.copy)}
      />
    {/if}
   </div>
  </div>

  {#if showDetails}
    <!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
    <div
      class="resizer"
      role="separator"
      aria-orientation="vertical"
      aria-label={$t("tb.resizeDetails")}
      title={$t("tb.resizeTip")}
      on:mousedown={(e) => startResize("right", e)}
    ></div>

    <div class="preview-pane">
      <Toolbar label={$t("tb.preview")}>
        <button
          slot="actions"
          class="popout-btn"
          title={$t("tb.popoutTip")}
          aria-label={$t("tb.popoutAria")}
          disabled={selectedEntries.length !== 1}
          on:click={popOutPreview}
        ><Icon name="popout" size={16} /></button>
        <div class="settings-row">
          <span>{$t("tb.defaultTab")}</span>
          <select
            value={showPreview ? "preview" : "details"}
            on:change={(e) => {
              showPreview = e.currentTarget.value === "preview";
              settings.saveShowPreview(showPreview);
            }}
          >
            <option value="preview">{$t("tb.preview")}</option>
            <option value="details">{$t("view.details")}</option>
          </select>
        </div>
        <div class="settings-row">
          <span>{$t("tb.paneWidth")}</span>
          <input
            type="number"
            min={RIGHT_MIN}
            max={RIGHT_MAX}
            bind:value={rightWidth}
            on:change={() => {
              rightWidth = clampWidth(rightWidth, RIGHT_MIN, RIGHT_MAX);
              settings.saveRightWidth(rightWidth);
            }}
          />
        </div>
      </Toolbar>
      <!-- svelte-ignore a11y-no-static-element-interactions -->
      <div
        class="preview-pane-toggle"
        role="tablist"
        aria-label={$t("tb.previewOrDetails")}
        title={$t("tb.dragPopoutTip")}
        on:pointerdown={onPreviewHeaderDown}
        on:pointermove={onPreviewHeaderMove}
        on:pointerup={endPreviewHeaderDrag}
        on:pointerleave={endPreviewHeaderDrag}
      >
        <button
          role="tab"
          class:active={showPreview}
          aria-selected={showPreview}
          on:click={() => { showPreview = true; settings.saveShowPreview(true); }}
        >{$t("tb.preview")}</button>
        <button
          role="tab"
          class:active={!showPreview}
          aria-selected={!showPreview}
          on:click={() => { showPreview = false; settings.saveShowPreview(false); }}
        >{$t("view.details")}</button>
      </div>

      {#if showPreview}
        <PreviewPane
          entry={selectedEntries.length === 1 ? selectedEntries[0] : null}
          assetUrl={convertFileSrc}
          loadText={loadPreviewText}
          loadEntries={loadArchiveEntries}
          loadInfo={loadPreviewInfo}
          loadImageData={loadImageData}
          saveText={savePreviewText}
        >
          <DetailsPane selected={selectedEntries} {folderName} {itemCount} />
        </PreviewPane>
      {:else}
        <DetailsPane selected={selectedEntries} {folderName} {itemCount} />
      {/if}
    </div>
  {/if}
</div>

<StatusBar
  {itemCount}
  {totalCount}
  selectedCount={selectedCount(selection)}
  {selectedSize}
  hiddenShown={showHidden}
  {notice}
  {noticeIsError}
  {diskFree}
  {diskTotal}
  git={gitStatus}
  on:pull={() => doSync("pull")}
  on:push={() => doSync("push")}
  on:sync={() => (syncDialogPath = currentPath)}
/>

{#if syncDialogPath}
  <SyncDialog
    path={syncDialogPath}
    on:done={() => { refreshGitStatus(currentPath); refresh(); }}
    on:close={() => (syncDialogPath = null)}
  />
{/if}

{#if ctx}
  <ContextMenu
    x={ctx.x}
    y={ctx.y}
    target={ctx.target}
    canPaste={pasteCheck.allowed}
    selectionCount={selectedCount(selection)}
    folderSelected={selectedEntries.length === 1 && selectedEntries[0]?.is_dir}
    executableSelected={selectedEntries.length === 1 && isExecutable(selectedEntries[0])}
    openIcon={selectedEntries.length === 1 ? iconFor(selectedEntries[0]) : "folder"}
    pinned={selectedEntries.length === 1 && pins.includes(selectedEntries[0].path)}
    favorited={selectedEntries.length === 1 && favorites.some((f) => f.path === selectedEntries[0].path)}
    extractable={!isHome && !archive && selectedEntries.length === 1 && isExtractable(selectedEntries[0])}
    compressible={!isHome && !archive && selectedEntries.length >= 1}
    comparable={!isHome && !archive && selectedEntries.length === 2 && selectedEntries.every((e) => !e.is_dir)}
    canTerminal={!isHome && !archive}
    sameTypeExt={selectedEntries.length === 1 && !selectedEntries[0].is_dir ? selectedEntries[0].extension : ""}
    on:action={(e) => runAction(e.detail)}
    on:close={() => (ctx = null)}
  />
{/if}

{#if confirm}
  <ConfirmDialog
    title={confirm.title}
    message={confirm.message}
    confirmLabel={confirm.label}
    danger
    on:confirm={confirm.onYes}
    on:cancel={() => (confirm = null)}
  />
{/if}

{#if activeWatchCwd && showTimeline}
  <AgentTimeline
    entries={$agentTimeline}
    agentName={watchedAgentName}
    on:navigate={(e) => navigate(e.detail)}
    on:close={() => (showTimeline = false)}
  />
{/if}

{#if batchRenameFor}
  <BatchRenameDialog
    names={batchRenameFor.map((e) => e.name)}
    on:apply={(e) => applyBatchRename(e.detail)}
    on:cancel={() => (batchRenameFor = null)}
  />
{/if}

{#if propsFor}
  <PropertiesDialog entries={propsFor} on:close={() => (propsFor = null)} />
{/if}

{#if showSettings}
  <SettingsDialog
    {showHidden}
    {showDetails}
    on:setHidden={(e) => { showHidden = e.detail; settings.saveShowHidden(showHidden); }}
    on:setDetails={(e) => { showDetails = e.detail; settings.saveShowDetails(showDetails); }}
    on:reset={resetAllSettings}
    on:openConsole={() => openAiConsole()}
    on:close={() => (showSettings = false)}
  />
{/if}

{#if consentPrompt}
  <ConsentSheet
    sidecarId="ai-console"
    state={consentPrompt}
    on:decide={onConsentDecision}
    on:cancel={() => (consentPrompt = null)}
  />
{/if}

{#if showAbout}
  <AboutDialog
    version={appVersion}
    repoUrl={REPO_URL}
    on:openurl={(e) => openExternal(e.detail)}
    on:close={() => (showAbout = false)}
  />
{/if}

{#if shortcutsOpen}
  <ShortcutsDialog on:close={() => (shortcutsOpen = false)} />
{/if}

{#if contentSearchOpen}
  <ContentSearchDialog
    root={currentPath}
    on:navigate={(e) => { contentSearchOpen = false; revealFileInApp(e.detail); }}
    on:close={() => (contentSearchOpen = false)}
  />
{/if}

{#if duplicatesOpen}
  <DuplicatesDialog
    root={currentPath}
    on:navigate={(e) => { duplicatesOpen = false; revealFileInApp(e.detail); }}
    on:close={() => (duplicatesOpen = false)}
  />
{/if}

{#if showRepos}
  <RepoBrowser on:close={() => (showRepos = false)} />
{/if}

{#if agentMenu}
  <AgentMenu
    x={agentMenu.x}
    y={agentMenu.y}
    label={agentMenu.label}
    sessionId={agentMenu.sessionId}
    sessionLabel={agentMenu.sessionLabel}
    on:confirm={closeAllConsoles}
    on:closeOne={(e) => closeOneConsole(e.detail)}
    on:close={() => (agentMenu = null)}
  />
{/if}

{#if patternSelectOpen}
  <PatternSelectDialog
    on:submit={(e) => selectByPattern(e.detail)}
    on:cancel={() => (patternSelectOpen = false)}
  />
{/if}

{#if showUpdate}
  <UpdateDialog
    state={updateState}
    version={pendingUpdate?.version ?? ""}
    currentVersion={appVersion}
    notes={pendingUpdate?.body ?? ""}
    progress={updateProgress}
    indeterminate={updateIndeterminate}
    error={updateError}
    on:install={installUpdate}
    on:retry={retryUpdate}
    on:close={dismissUpdate}
  />
{/if}

<style>
  /* Agent Watch activity strip (CPE-399) — a thin live banner above the file list, shown only
     while the explorer is inside a running agent's Project folder. */
  .agent-strip {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 12px;
    font-size: 12px;
    background: color-mix(in srgb, var(--accent) 10%, var(--surface));
    border-bottom: 1px solid var(--border);
    overflow: hidden;
    white-space: nowrap;
  }
  .agent-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #3a9d4a;
    flex: 0 0 auto;
    animation: agent-dot-pulse 1.6s ease-in-out infinite;
  }
  @keyframes agent-dot-pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.35; }
  }
  .agent-strip-label { font-weight: 600; flex: 0 0 auto; }
  .agent-strip-idle { opacity: 0.6; }
  .agent-chip {
    flex: 0 0 auto;
    padding: 1px 7px;
    border-radius: 999px;
    font-size: 11px;
    color: #fff;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 180px;
  }
  .agent-chip.created { background: #3a9d4a; }
  .agent-chip.modified { background: #b5872b; }
  .agent-chip.renamed { background: #3a72b5; }
  .agent-chip.removed { background: #b5433a; }
  .agent-log-btn {
    flex: 0 0 auto;
    margin-left: auto;
    height: 20px;
    padding: 0 9px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--surface);
    color: var(--text);
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
  }
  .agent-log-btn:hover { background: var(--surface-alt); }

  /* AI Console toolbar button (CPE-351) — sits next to the settings gear. */
  .tb-console {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 24px;
    margin-left: 4px;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text);
    font-size: 12px;
  }
  .tb-console:hover {
    background: var(--surface-alt);
  }
  /* Live count of running agent sessions (CPE-404) — visible even with the console window closed. */
  .tb-console-count {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 16px;
    height: 16px;
    padding: 0 4px;
    border-radius: 999px;
    background: #3a9d4a;
    color: #fff;
    font-size: 10px;
    font-weight: 700;
    line-height: 1;
  }
</style>
