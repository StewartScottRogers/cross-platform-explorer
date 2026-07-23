<script lang="ts">
  import { onMount, onDestroy, tick } from "svelte";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { invoke } from "./lib/invoke";
  import { rawInvoke, createChannel } from "./lib/invoke";
  import { open as openFolderDialog, save as saveFileDialog } from "@tauri-apps/plugin-dialog";
  import { check, type Update } from "@tauri-apps/plugin-updater";
  import { relaunch, exit } from "@tauri-apps/plugin-process";
  import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
  import { getVersion } from "@tauri-apps/api/app";
  import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
  import { emit, once, listen } from "@tauri-apps/api/event";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { getCurrentWindow } from "@tauri-apps/api/window";

  import Icon from "./lib/components/Icon.svelte";
  import MenuBar from "./lib/components/MenuBar.svelte";
  import AboutDialog from "./lib/components/AboutDialog.svelte";
  import SettingsDialog from "./lib/components/SettingsDialog.svelte";
  import { startAiConsole, startAgentBoard, consoleUrlWith, platformActive, consentState, setConsent, CAPABILITY_INFO } from "./lib/sidecar";
  import { initAgentSessions, agentSessions, watchTargetFor, normalizePath, clearAgentSessions } from "./lib/agentSessions";
  import { startAgentWatch, stopAgentWatch, type FsActivity } from "./lib/sidecar";
  import { initAgentActivity, fsActivity, recentActivities, agentTimeline, affectsListing } from "./lib/agentActivity";
  import { initAgentDiffs } from "./lib/agentDiffs";
  import AgentTimeline from "./lib/components/AgentTimeline.svelte";
  import DiskSpaceView from "./lib/components/DiskSpaceView.svelte";
  import DiagnosticsOverlay from "./lib/components/DiagnosticsOverlay.svelte";
  import { setDiagnosticsEnabled } from "./lib/diagnostics";
  import UpdateDialog from "./lib/components/UpdateDialog.svelte";
  import TabBar from "./lib/components/TabBar.svelte";
  import NavToolbar from "./lib/components/NavToolbar.svelte";
  import CommandBar from "./lib/components/CommandBar.svelte";
  import Sidebar from "./lib/components/Sidebar.svelte";
  import RepoBrowser from "./lib/components/RepoBrowser.svelte";
  import BoardView from "./lib/components/BoardView.svelte";
  import { BOARD_MIN_W, BOARD_MIN_H } from "./lib/board";
  import WorkbenchView from "./lib/components/WorkbenchView.svelte";
  import DocsView from "./lib/components/DocsView.svelte";
  import { docSlugForSection, type Section } from "./lib/sectionDocs";
  import CommandPalette from "./lib/components/CommandPalette.svelte";
  import type { Command } from "./lib/commandPalette";
  import AgentMenu from "./lib/components/AgentMenu.svelte";
  import Toolbar from "./lib/components/Toolbar.svelte";
  import ExplorerPane from "./lib/components/ExplorerPane.svelte";
  import DetailsPane from "./lib/components/DetailsPane.svelte";
  import PreviewPane from "./lib/components/PreviewPane.svelte";
  import type { ArchiveEntry } from "./lib/preview/provider";
  import StatusBar from "./lib/components/StatusBar.svelte";
  import SyncDialog from "./lib/components/SyncDialog.svelte";
  import ConflictDialog from "./lib/components/ConflictDialog.svelte";
  import { loadSyncPolicy } from "./lib/syncPolicy";
  import { loadAutoMirror, isDue, autoSyncActions, pausedReason } from "./lib/autoMirror";
  import ContextMenu from "./lib/components/ContextMenu.svelte";
  import ConfirmDialog from "./lib/components/ConfirmDialog.svelte";
  import ShortcutsDialog from "./lib/components/ShortcutsDialog.svelte";
  import ContentSearchDialog from "./lib/components/ContentSearchDialog.svelte";
  import FileNameSearchDialog from "./lib/components/FileNameSearchDialog.svelte";
  import TransferPanel from "./lib/components/TransferPanel.svelte";
  import TransferConflictDialog from "./lib/components/TransferConflictDialog.svelte";
  import { initTransfers, startTransfer, collidingNames, type TransferReport, type ConflictPolicy } from "./lib/transfers";
  import DuplicatesDialog from "./lib/components/DuplicatesDialog.svelte";
  import { namesList, detailList, csvList } from "./lib/listing";
  import { parentDir as parentOfPath, baseName } from "./lib/contentSearch";
  import PropertiesDialog from "./lib/components/PropertiesDialog.svelte";
  import BatchRenameDialog from "./lib/components/BatchRenameDialog.svelte";
  import TagEditor from "./lib/components/TagEditor.svelte";
  import { initTags, tags, retagPath, renameTag, deleteTag, importTags, exportTags } from "./lib/tags";
  import { resolveEffect } from "./lib/dnd";
  import {
    smartFolders,
    smartFolderPaths,
    saveSmartFolder,
    renameSaved as renameSmartSaved,
    removeSaved as removeSmartSaved,
    type SmartFolder,
  } from "./lib/smartFolders";
  import TagMenu from "./lib/components/TagMenu.svelte";
  import SmartFolderMenu from "./lib/components/SmartFolderMenu.svelte";
  import { tagCounts } from "./lib/tagFilter";
  import type { RenameItem } from "./lib/batchRename";

  import { t } from "./lib/i18n";
  import { friendlyError, splitPath, formatPathsForClipboard } from "./lib/format";
  import { withBusy } from "./lib/busy";
  import { uniqueName, uniqueNameWithExt } from "./lib/naming";
  import { validateFileName } from "./lib/filename";
  import { matchesGlob } from "./lib/glob";
  import PatternSelectDialog from "./lib/components/PatternSelectDialog.svelte";
  import { firstMatchIndex } from "./lib/typeahead";
  import { clampWidth } from "./lib/resize";
  import {
    createHistory, visit, back, forward, canGoBack, canGoForward, current, recentPaths,
    type History,
  } from "./lib/history";
  import { pushClosedTab, keepOnly, keepThroughRight } from "./lib/tabs";
  import TabMenu from "./lib/components/TabMenu.svelte";
  import {
    emptySelection, click as selClick, selectOnly, selectAll, moveLead,
    selectedIndices, selectedCount, remapByPath, invertSelection, selectIndices,
    type Selection,
  } from "./lib/selection";
  import { arrowDelta } from "./lib/gridnav";
  import {
    emptyClipboard, stage, isEmpty as clipEmpty, canPaste as clipCanPaste,
    type Clipboard,
  } from "./lib/clipboard";
  import { detectContexts, type FolderAction } from "./lib/folderContext";
  import { isExecutable, iconFor, sameTypeIndices, isImage } from "./lib/filetypes";
  import QuickLook from "./lib/components/QuickLook.svelte";
  import * as settings from "./lib/settings";
  import type { ColorRule } from "./lib/colorRules";
  import ColorRulesDialog from "./lib/components/ColorRulesDialog.svelte";
  import SessionHistoryDialog from "./lib/components/SessionHistoryDialog.svelte";
  import CompareDialog from "./lib/components/CompareDialog.svelte";
  import IntegrityDialog from "./lib/components/IntegrityDialog.svelte";
  import type { ChecksumEntry, IntegrityReport } from "./lib/integrity";
  import SelectByDialog from "./lib/components/SelectByDialog.svelte";
  import { selectMatching } from "./lib/selectMatch";
  import type { Condition } from "./lib/colorRules";
  import WatchRulesDialog from "./lib/components/WatchRulesDialog.svelte";
  import type { WatchRule } from "./lib/watchRules";
  import { startFolderWatch, stopFolderWatch, undoFire, type WatchFire } from "./lib/folderWatch";
  import WorkspacesDialog from "./lib/components/WorkspacesDialog.svelte";
  import { pruneMissing, type Workspace, type WorkspaceTab } from "./lib/workspaces";
  import BackupDashboard from "./lib/components/BackupDashboard.svelte";
  import { planBackup, type BackupJob } from "./lib/backup";
  import type { CompareNode } from "./lib/treeDiff";
  import { startDriveScheduler, stopDriveScheduler } from "./lib/driveScheduler";
  import AttributesDialog from "./lib/components/AttributesDialog.svelte";
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
  // Monotonic token identifying the current folder load (CPE-664). A new load bumps it; batches from a
  // superseded stream carry a stale token and are dropped, so navigating away mid-load can't bleed rows.
  // Directory-listing fetch + LRU cache moved into <ExplorerPane> (CPE-676 domino 3b) — the pane owns
  // fetching its own listing via `explorerPane.loadListing(path, useCache)`.

  // --- Diagnostics mode (CPE-758) --------------------------------------------------------------------
  // On-screen timing of EVERY backend/OS call, captured by the instrumented invoke wrapper (src/lib/
  // diagnostics.ts). Toggled by the user from Application → Diagnostics, persisted across sessions.
  // `setDiagnosticsEnabled` gates recording so it costs nothing when off. (I can force it on for testing
  // via `localStorage["cpe.diagnostics"] = "true"`.)
  let diagnostics = settings.loadDiagnostics();
  $: setDiagnosticsEnabled(diagnostics);
  // ---------------------------------------------------------------------------------------------------

  let notice = "";
  let noticeIsError = false;
  let noticeTimer: ReturnType<typeof setTimeout> | undefined;

  let selection: Selection = emptySelection();

  // Dual-pane / commander mode (CPE-677, epic CPE-617). Pane B is a second <ExplorerPane> rendered beside
  // pane A when `dualPane` is on, navigating independently via navigateB/openB. Single-pane (default) is
  // unchanged. `activePane` drives the focus ring + Tab switch.
  let dualPane = settings.loadDualPane();
  let paneBPath = settings.loadPaneBPath();
  let explorerPaneB: ExplorerPane | undefined;
  let activePane: 0 | 1 = 0;
  // Pane B's own listing/selection state (ExplorerPane self-owns the derived pipeline + fetch; these are
  // bound back). Config (view/sort/hidden/colour rules/…) is shared with pane A for v1.
  let entriesB: DirEntry[] = [];
  let visibleB: DirEntry[] = [];
  let shownB: DirEntry[] = [];
  let loadingB = false;
  let errorB = "";
  let selectionB: Selection = emptySelection();
  let selectedEntriesB: DirEntry[] = [];

  // Owned+derived inside <ExplorerPane> (CPE-676); bound back here so App's ops keep reading it. When the
  // split lands (CPE-677) this comes from the active pane instead of a single binding.
  let selectedEntries: DirEntry[] = [];
  // `visible` (the sort/hidden/search/type/tag pipeline) + its pre-filter `shown` are derived + owned in
  // <ExplorerPane> now (CPE-676 domino 2); bound back here so App's ops + status bar keep reading them.
  let visible: DirEntry[] = [];
  let shown: DirEntry[] = [];
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

  // Dual-pane (CPE-677): two equal file columns, preview suppressed; reuses the preview grid slot.
  $: effectiveGridCols = dualPane ? `${sidebarWidth}px 6px 1fr 6px 1fr` : gridCols;

  /** Live column count of the file grid, for 2-D arrow-key nav (CPE-769). 1 for list/details; for the
      icons/gallery grid, read the resolved `grid-template-columns` off the live `.rows.grid` (the same
      source of truth FileList windows against), so it tracks pane width / view without extra plumbing. */
  function currentGridCols(): number {
    if (view !== "icons" && view !== "gallery") return 1;
    const el = document.querySelector<HTMLElement>(".rows.grid");
    if (!el) return 1;
    const tracks = getComputedStyle(el).gridTemplateColumns.split(" ").filter((s) => s && s !== "none").length;
    return Math.max(1, tracks);
  }

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
  /** Recursive folder-size column (CPE-750): opt-in toggle + a per-path cache of computed subtree sizes,
      filled lazily for visible folders. `pendingSizes` dedups in-flight `dir_size` calls. */
  let showFolderSizes = settings.loadShowFolderSizes();
  let folderSizes = new Map<string, number>();
  const pendingSizes = new Set<string>();
  let pins: string[] = [];
  let recents: RecentFile[] = [];
  let favorites: Favorite[] = [];
  let recentFolders: RecentFile[] = [];
  let columnWidths: number[] = settings.loadColumnWidths();
  /** Active rule-based coloring rule set (CPE-776, epic CPE-709); empty ⇒ rows unstyled. */
  let colorRules: ColorRule[] = settings.loadColorRules();
  let colorRulesOpen = false;
  let sessionHistoryOpen = false;
  let compareOpen = false;
  let compareLeft = "";
  let compareRight = "";
  let integrityOpen = false;
  let integrityBaselines: Record<string, ChecksumEntry[]> = settings.loadIntegrityBaselines();
  /** Opt-in: verify all baselined folders once at startup (CPE-872). Off by default. */
  let verifyOnStartup = settings.loadVerifyOnStartup();
  /** Periodic re-verify timer while the app stays open (CPE-875); cleared on teardown. */
  let verifyTimer: ReturnType<typeof setInterval> | undefined;
  const VERIFY_INTERVAL_MS = 6 * 60 * 60 * 1000; // every 6 hours
  /** Verify every baselined folder at once (CPE-871) and surface a one-line summary — the integrity
   *  guard's "check all my monitored folders" action. Silent corruption / missing files raise an error
   *  notice; a clean sweep confirms. */
  async function verifyAllBaselines() {
    const paths = Object.keys(integrityBaselines);
    if (paths.length === 0) return;
    try {
      const reports = await invoke<Record<string, IntegrityReport>>("verify_all_baselines", { baselines: integrityBaselines });
      const flagged = Object.values(reports).filter((r) => r.corrupted.length > 0 || r.missing.length > 0);
      if (flagged.length === 0) {
        showNotice(`All ${paths.length} baselined folder${paths.length === 1 ? "" : "s"} verified — no issues.`, false);
      } else {
        const bad = flagged.reduce((n, r) => n + r.corrupted.length + r.missing.length, 0);
        showNotice(`${flagged.length} of ${paths.length} baselined folders have issues — ${bad} file${bad === 1 ? "" : "s"} corrupted or missing. Open Integrity to review.`, true);
      }
    } catch (e) {
      showNotice(`Verify all failed: ${e}`, true);
    }
  }
  let selectByOpen = false;
  let watchRulesOpen = false;
  let watchRules: WatchRule[] = settings.loadWatchRules();
  // Live watched-folder rules (CPE-794, sidecar-only). Watched folders + on/off persist; the log is
  // an in-memory ring of recent executed rules.
  let watchedFolders: string[] = settings.loadWatchedFolders();
  let watchLive = false;
  let watchLog: WatchFire[] = [];
  let workspacesOpen = false;
  let workspaces: Workspace[] = settings.loadWorkspaces();
  /** CPE-789: opt-in launch-time auto-restore of the last session. `sessionReady` gates capture until
      after the restore attempt so the reactive save never clobbers the saved session with the default tab. */
  let autoRestore = settings.loadAutoRestore();
  let sessionReady = false;
  let backupOpen = false;
  let backupJobs: BackupJob[] = settings.loadBackupJobs();
  let backupHistory: Record<string, settings.BackupRunRecord[]> = settings.loadBackupHistory();

  /** Record a completed backup/restore run in the per-job history (CPE-798), capped + persisted. */
  function recordBackupRun(jobId: string, status: settings.BackupRunRecord) {
    const prev = backupHistory[jobId] ?? [];
    backupHistory = { ...backupHistory, [jobId]: [status, ...prev].slice(0, 8) };
    settings.saveBackupHistory(backupHistory);
  }

  /** Run a backup job now (used by the drive-connect scheduler, CPE-797). Same streamed apply the
      dashboard uses; records the run in history and shows a notice. */
  async function runBackupJobNow(job: BackupJob) {
    try {
      const [s, d] = await Promise.all([
        rawInvoke<CompareNode[]>("scan_tree", { path: job.source, maxDepth: 32 }),
        rawInvoke<CompareNode[]>("scan_tree", { path: job.dest, maxDepth: 32 }),
      ]);
      const p = planBackup(s, d, job.mirror);
      const results: OpResult[] = [];
      const channel = createChannel<OpResult[]>();
      channel.onmessage = (batch) => { for (const r of batch) results.push(r); };
      await rawInvoke("apply_backup_plan_stream", {
        sourceRoot: job.source, destRoot: job.dest,
        copy: p.copy, update: p.update, delete: p.delete, verify: true,
        onResult: channel,
      });
      const failed = results.filter((r) => !r.ok).length;
      recordBackupRun(job.id, { when: Date.now(), ok: results.length - failed, failed, label: "auto" });
      showNotice(`Auto-backup "${job.name}": ${results.length - failed} copied${failed ? `, ${failed} failed` : ""}`);
    } catch (e) {
      showNotice(`Auto-backup "${job.name}" failed: ${e}`, true);
    }
  }

  /** Start/stop the drive-connect scheduler to match the current jobs (CPE-797). No poll unless a job
      opts into auto-run. */
  function reconcileDriveScheduler() {
    void startDriveScheduler(() => backupJobs, runBackupJobNow);
  }
  let attributesOpen = false;
  let attrTargets: { path: string; name: string; modifiedMs: number | null }[] = [];
  let search = "";
  /** Active sidebar Tags filter — show only entries carrying this tag (CPE-639); "" = off. */
  let selectedTag = "";
  /** Right-click menu for a sidebar tag (rename/delete), or null (CPE-653). */
  let tagMenu: { x: number; y: number; tag: string } | null = null;
  /** Right-click menu for a sidebar smart folder (rename/delete), or null (CPE-667). */
  let smartFolderMenu: { x: number; y: number; id: string; name: string } | null = null;
  /** Full-screen quick-look of images (Space), or null (CPE-645). */
  let quickLook: { images: { path: string; name: string }[]; index: number } | null = null;

  /** Open quick-look on the selected image, seeding the folder's images. Returns false if not applicable. */
  function openQuickLook(): boolean {
    if (isHome || archive || selectedEntries.length !== 1) return false;
    const sel = selectedEntries[0];
    if (sel.is_dir || !isImage(sel.name)) return false;
    const images = visible.filter((e) => !e.is_dir && isImage(e.name)).map((e) => ({ path: e.path, name: e.name }));
    const index = images.findIndex((im) => im.path === sel.path);
    if (index < 0) return false;
    quickLook = { images, index };
    return true;
  }
  function quickLookMove(delta: number) {
    if (!quickLook) return;
    const n = quickLook.images.length;
    quickLook = { ...quickLook, index: (quickLook.index + delta + n) % n };
  }
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
  /** The entry whose tags/label are being edited (CPE-637), or null when the editor is closed. */
  let tagEditorFor: DirEntry[] | null = null;

  // ---- Application menu (CPE-229) ----
  const REPO_URL = "https://github.com/StewartScottRogers/cross-platform-explorer";
  let showAbout = false;
  let showSettings = false;
  let shortcutsOpen = false;
  /** "Search in files" content-search overlay (Ctrl+Shift+F), scoped to the current folder (CPE-417). */
  let contentSearchOpen = false;
  /** "Find files by name" recursive name-search overlay (Ctrl+P), scoped to the current folder (CPE-603). */
  let fileSearchOpen = false;
  /** Query the toolbar Search hands to the recursive find dialog on Enter (CPE-866). */
  let deepSearchQuery = "";
  /** "Find duplicate files" overlay, scoped to the current folder (CPE-421). */
  let duplicatesOpen = false;
  let patternSelectOpen = false;
  /** Repositories browser overlay (CPE-434/435) — browse GitHub & other forges in-app. */
  let showRepos = false;
  /** Agent Board (CPE-521) — Kanban over the current folder's Tickets/. */
  let showBoard = false;
  /** Integrated workbench (CPE-526) — git diff of the current folder. */
  let showWorkbench = false;
  /** Application → Documents (CPE-537) — the built-in docs viewer. */
  let showDocs = false;
  /** Optional deep-link slug for the docs viewer (CPE-594/596); null ⇒ default (Overview). */
  let docsSlug: string | null = null;
  /** Open Documents, optionally on a specific section's page (CPE-596). */
  function openDocs(section: Section | null = null) {
    docsSlug = section ? docSlugForSection(section) : null;
    showDocs = true;
  }
  /** Open Documents on a specific doc slug — for surfaces that aren't a `Section` (e.g. the search boxes
   * linking to the search-options page, CPE-921). */
  function openDocsSlug(slug: string) {
    docsSlug = slug;
    showDocs = true;
  }
  /** The section the user is currently in, for F1 / the global Documents open (CPE-596). */
  function currentSection(): Section {
    if (showWorkbench) return "workbench";
    return isHome ? "home" : "explorer";
  }
  /** Every documented section + a friendly label, for per-section jump-links (palette, menus) — CPE-764. */
  const DOC_SECTIONS: { section: Section; label: string }[] = [
    { section: "home", label: "Overview" },
    { section: "explorer", label: "Explorer" },
    { section: "disk-usage", label: "Disk usage" },
    { section: "workbench", label: "Workbench" },
    { section: "agent-board", label: "Agent Board" },
    { section: "ai-console", label: "Agent Deck" },
    { section: "agent-grid", label: "Agent Grid" },
    { section: "repositories", label: "Repositories" },
    { section: "swarms", label: "Swarms" },
  ];

  // Command Palette (CPE-602): Ctrl+Shift+P. The command list reuses existing handlers — nothing is
  // duplicated; `enabled` closures read live state so context-invalid commands grey out.
  let paletteOpen = false;
  const inFolder = () => !isHome && !archive && !smartFolder;
  const hasSelection = () => selectedEntries.length > 0;
  const oneSelected = () => selectedEntries.length === 1;
  const canCloseTab = () => tabs.length > 1;
  // Wrappers so the palette's reactive block references functions, not reactive reads/writes inline —
  // reading selectedEntries/activeId directly inside `$: paletteCommands` forms a dependency cycle.
  const renameSelected = () => { if (selectedEntries.length === 1) beginRename(selectedEntries[0]); };
  const closeActiveTab = () => closeTab(activeId);
  $: paletteCommands = [
    { id: "nav.home", group: $t("palette.groupGo"), label: $t("palette.home"), shortcut: "", run: () => { if (archive) exitArchive(); navigate(HOME); } },
    { id: "nav.back", group: $t("palette.groupGo"), label: $t("palette.back"), shortcut: "Alt+←", run: goBack, enabled: () => canGoBack(activeTab.history) },
    { id: "nav.forward", group: $t("palette.groupGo"), label: $t("palette.forward"), shortcut: "Alt+→", run: goForward, enabled: () => canGoForward(activeTab.history) },
    { id: "nav.up", group: $t("palette.groupGo"), label: $t("palette.upFolder"), shortcut: "Alt+↑", run: goUp, enabled: inFolder },
    { id: "nav.refresh", group: $t("palette.groupGo"), label: $t("palette.refresh"), shortcut: "F5", run: refresh },
    { id: "tab.new", group: $t("palette.groupGo"), label: $t("palette.newTab"), shortcut: "Ctrl+T", run: newTab },
    { id: "tab.close", group: $t("palette.groupGo"), label: $t("palette.closeTab"), shortcut: "Ctrl+W", run: closeActiveTab, enabled: canCloseTab },
    { id: "tab.reopen", group: $t("palette.groupGo"), label: $t("palette.reopenTab"), shortcut: "Ctrl+Shift+T", run: reopenClosedTab },
    { id: "file.newFolder", group: $t("palette.groupFile"), label: $t("palette.newFolder"), keywords: "create directory mkdir", run: newFolder, enabled: inFolder },
    { id: "file.newFile", group: $t("palette.groupFile"), label: $t("palette.newFile"), keywords: "create", run: newFile, enabled: inFolder },
    { id: "file.copy", group: $t("palette.groupFile"), label: $t("palette.copy"), shortcut: "Ctrl+C", run: doCopy, enabled: hasSelection },
    { id: "file.cut", group: $t("palette.groupFile"), label: $t("palette.cut"), shortcut: "Ctrl+X", run: doCut, enabled: hasSelection },
    { id: "file.paste", group: $t("palette.groupFile"), label: $t("palette.paste"), shortcut: "Ctrl+V", run: doPaste, enabled: inFolder },
    { id: "file.copyPath", group: $t("palette.groupFile"), label: $t("palette.copyPath"), shortcut: "Ctrl+Shift+C", run: doCopyPath, enabled: hasSelection },
    { id: "file.copyName", group: $t("palette.groupFile"), label: $t("palette.copyName"), run: doCopyName, enabled: hasSelection },
    { id: "file.rename", group: $t("palette.groupFile"), label: $t("palette.rename"), shortcut: "F2", run: renameSelected, enabled: oneSelected },
    { id: "file.duplicate", group: $t("palette.groupFile"), label: $t("palette.duplicate"), shortcut: "Ctrl+D", run: doDuplicate, enabled: hasSelection },
    { id: "file.delete", group: $t("palette.groupFile"), label: $t("palette.delete"), keywords: "recycle bin trash remove", shortcut: "Delete", run: () => doDelete(false), enabled: hasSelection },
    { id: "file.deletePermanent", group: $t("palette.groupFile"), label: $t("palette.deletePermanent"), keywords: "remove", shortcut: "Shift+Delete", run: () => doDelete(true), enabled: hasSelection },
    { id: "file.selectAll", group: $t("palette.groupFile"), label: $t("palette.selectAll"), shortcut: "Ctrl+A", run: selectAllVisible, enabled: inFolder },
    { id: "file.properties", group: $t("palette.groupFile"), label: $t("palette.properties"), shortcut: "Alt+Enter", run: openProperties, enabled: hasSelection },
    { id: "file.reveal", group: $t("palette.groupFile"), label: $t("palette.reveal"), keywords: "explorer finder show os", run: revealInExplorer, enabled: inFolder },
    { id: "file.terminal", group: $t("palette.groupFile"), label: $t("palette.terminal"), keywords: "shell command prompt console", run: () => openTerminal(currentPath), enabled: inFolder },
    { id: "view.details", group: $t("palette.groupView"), label: $t("palette.viewDetails"), run: () => { view = "details"; settings.saveView(view); } },
    { id: "view.list", group: $t("palette.groupView"), label: $t("palette.viewList"), run: () => { view = "list"; settings.saveView(view); } },
    { id: "view.icons", group: $t("palette.groupView"), label: $t("palette.viewIcons"), run: () => { view = "icons"; settings.saveView(view); } },
    { id: "view.gallery", group: $t("palette.groupView"), label: $t("palette.viewGallery"), run: () => { view = "gallery"; settings.saveView(view); } },
    { id: "sort.name", group: $t("palette.groupView"), label: $t("palette.sortName"), run: () => { sortKey = "name"; settings.saveSortKey(sortKey); } },
    { id: "sort.modified", group: $t("palette.groupView"), label: $t("palette.sortModified"), run: () => { sortKey = "modified"; settings.saveSortKey(sortKey); } },
    { id: "sort.type", group: $t("palette.groupView"), label: $t("palette.sortType"), run: () => { sortKey = "type"; settings.saveSortKey(sortKey); } },
    { id: "sort.size", group: $t("palette.groupView"), label: $t("palette.sortSize"), run: () => { sortKey = "size"; settings.saveSortKey(sortKey); } },
    { id: "sort.dir", group: $t("palette.groupView"), label: $t("palette.sortDir"), run: () => { sortDir = sortDir === "asc" ? "desc" : "asc"; settings.saveSortDir(sortDir); } },
    { id: "view.toggleDetails", group: $t("palette.groupView"), label: showDetails ? $t("palette.hideDetails") : $t("palette.showDetails"), shortcut: "Alt+P", run: () => { showDetails = !showDetails; settings.saveShowDetails(showDetails); } },
    { id: "view.popOut", group: $t("palette.groupView"), label: $t("palette.popOut"), shortcut: "Ctrl+Shift+O", run: popOutPreview },
    { id: "view.hidden", group: $t("palette.groupView"), label: showHidden ? $t("palette.hideHidden") : $t("palette.showHidden"), run: () => { showHidden = !showHidden; settings.saveShowHidden(showHidden); } },
    { id: "view.folderSizes", group: $t("palette.groupView"), label: showFolderSizes ? $t("palette.hideFolderSizes") : $t("palette.showFolderSizes"), keywords: "folder size recursive subtree column", run: toggleFolderSizes },
    { id: "view.foldersFirst", group: $t("palette.groupView"), label: foldersFirst ? $t("palette.mixFolders") : $t("palette.groupFolders"), run: () => { foldersFirst = !foldersFirst; settings.saveFoldersFirst(foldersFirst); } },
    { id: "view.dualPane", group: $t("palette.groupView"), label: dualPane ? $t("palette.singlePane") : $t("palette.dualPane"), keywords: "dual pane split commander two side by side", run: toggleDualPane },
    { id: "view.paneCopy", group: $t("palette.groupView"), label: $t("palette.paneCopy"), keywords: "commander copy other pane f5", run: commanderCopy, enabled: () => dualPane },
    { id: "view.paneMove", group: $t("palette.groupView"), label: $t("palette.paneMove"), keywords: "commander move other pane f6", run: commanderMove, enabled: () => dualPane },
    { id: "view.paneSwap", group: $t("palette.groupView"), label: $t("palette.paneSwap"), keywords: "commander swap panes exchange", run: swapPanes, enabled: () => dualPane },
    { id: "view.paneMirror", group: $t("palette.groupView"), label: $t("palette.paneMirror"), keywords: "commander mirror equal pane path", run: mirrorPane, enabled: () => dualPane },
    { id: "tool.findByName", group: $t("palette.groupTools"), label: $t("palette.findByName"), shortcut: "Ctrl+P", run: () => (fileSearchOpen = true), enabled: inFolder },
    { id: "tool.searchInFiles", group: $t("palette.groupTools"), label: $t("palette.searchInFiles"), shortcut: "Ctrl+Shift+F", run: () => (contentSearchOpen = true), enabled: inFolder },
    { id: "tool.findDuplicates", group: $t("palette.groupTools"), label: $t("palette.findDuplicates"), run: () => (duplicatesOpen = true), enabled: inFolder },
    { id: "tool.colorRules", group: $t("palette.groupTools"), label: $t("palette.colorRules"), keywords: "color rules highlight label", run: () => (colorRulesOpen = true) },
    { id: "tool.sessionHistory", group: $t("palette.groupTools"), label: $t("palette.sessionHistory"), keywords: "audit log history export sessions activity", run: () => (sessionHistoryOpen = true) },
    { id: "tool.compareFolders", group: $t("palette.groupTools"), label: $t("palette.compareFolders"), keywords: "diff compare folders directories tree", run: openCompare },
    { id: "tool.integrity", group: $t("palette.groupTools"), label: $t("palette.integrity"), keywords: "integrity checksum bitrot corruption verify baseline", run: () => (integrityOpen = true) },
    { id: "tool.verifyAll", group: $t("palette.groupTools"), label: $t("palette.verifyAll"), keywords: "integrity verify all baselined folders bitrot corruption monitor check", run: verifyAllBaselines, enabled: () => Object.keys(integrityBaselines).length > 0 },
    { id: "tool.selectBy", group: $t("palette.groupTools"), label: $t("palette.selectBy"), keywords: "select by criteria extension size date filter", run: () => (selectByOpen = true), enabled: inFolder },
    { id: "tool.watchRules", group: $t("palette.groupTools"), label: $t("palette.watchRules"), keywords: "watch rules folder automation move copy tag rename", run: () => (watchRulesOpen = true) },
    { id: "tool.workspaces", group: $t("palette.groupGo"), label: $t("palette.workspaces"), keywords: "workspace layout tabs save session restore", run: () => (workspacesOpen = true) },
    { id: "tool.backup", group: $t("palette.groupTools"), label: $t("palette.backup"), keywords: "backup jobs copy mirror restore sync", run: () => (backupOpen = true) },
    { id: "tool.attributes", group: $t("palette.groupTools"), label: $t("palette.attributes"), keywords: "attributes permissions readonly hidden mode chmod", run: openAttributes },
    { id: "tool.aiConsole", group: $t("palette.groupTools"), label: $t("palette.openAiConsole"), run: () => openAiConsole(), enabled: () => aiConsoleAvailable },
    { id: "tool.agentBoardWindow", group: $t("palette.groupTools"), label: $t("palette.openAgentBoardWindow"), keywords: "agent board kanban tickets window pop out", run: () => openAgentBoard() },
    { id: "app.settings", group: $t("palette.groupApp"), label: $t("palette.settings"), run: () => (showSettings = true) },
    { id: "app.documents", group: $t("palette.groupApp"), label: $t("palette.documents"), shortcut: "F1", run: () => openDocs(currentSection()) },
    { id: "app.shortcuts", group: $t("palette.groupApp"), label: $t("palette.shortcuts"), shortcut: "?", run: () => (shortcutsOpen = true) },
    { id: "app.exportTags", group: $t("palette.groupApp"), label: $t("palette.exportTags"), keywords: "tags backup", run: exportTagsToFile },
    { id: "app.importTags", group: $t("palette.groupApp"), label: $t("palette.importTags"), keywords: "tags restore merge", run: importTagsFromFile },
    { id: "app.about", group: $t("palette.groupApp"), label: $t("palette.about"), run: () => (showAbout = true) },
    // Jump back to a recently-visited folder (CPE-604) — the full path is a keyword so typing any
    // part of it matches, while the label stays the short folder name.
    ...recentPaths(activeTab.history).map((p) => ({
      id: `recent:${p}`, group: $t("palette.groupRecent"), label: baseName(p) || p, keywords: p, run: () => navigate(p),
    })),
    // Per-section docs jump-links (CPE-764): open Documents straight to any section's page from anywhere.
    ...DOC_SECTIONS.map((s) => ({
      id: `docs:${s.section}`, group: "Documents", label: `Docs: ${s.label}`, keywords: "documentation help guide",
      run: () => openDocs(s.section),
    })),
  ] satisfies Command[];

  /** Open a URL in a dedicated browser webview window (CPE-527) — safe under the strict CSP since it's
      a separate webview, not an iframe in the main window. The URL is validated http/https in-view. */
  function openBrowserWindow(url: string) {
    try {
      new WebviewWindow(`workbench-browser-${Date.now()}`, { url, title: url, width: 1000, height: 720 });
    } catch {
      showNotice("Couldn't open the browser window.", true);
    }
  }
  /** Git sync status of the current folder (CPE-462) — two-way mirror status bar. Null when the
      folder isn't a git repo, or in the plain (non-sidecar) build where the command is absent. */
  let gitStatus: { is_repo?: boolean; branch?: string; ahead?: number; behind?: number; dirty?: boolean; conflicted?: boolean } | null = null;

  /** The path whose full two-way-mirror Sync dialog is open (CPE-495), or null when closed. */
  let syncDialogPath: string | null = null;
  /** The path whose conflict resolver is open (CPE-496), or null when closed. */
  let conflictDialogPath: string | null = null;

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
  /** Right-click "close the Agent Deck" menu (CPE-457), shown from an Agents leaf or the AI
      Console button. `label` differs per source; confirming stops the console + clears the leaves. */
  let agentMenu: { x: number; y: number; label: string; sessionId?: string; sessionLabel?: string } | null = null;

  /** Close the Agent Deck entirely (all running agents) and clear the Agents leaves. The console
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
  /** Close a single agent session (CPE-489) — routes to the Agent Deck's per-session close endpoint
      via the host. The console emits an `ended` for it, which prunes the leaf; the others keep running. */
  async function closeOneConsole(sessionId: string) {
    agentMenu = null;
    try {
      await invoke("sidecar_close_session", { sessionId });
    } catch (e) {
      console.debug("close session failed:", e);
    }
  }
  /** True in sidecar-platform builds — gates the Agent Deck toolbar button (CPE-351). */
  let aiConsoleAvailable = false;
  /** Teardown for the Agent Watch session listener (CPE-396). */
  let unlistenSessions: (() => void) | null = null;
  let unlistenTransferDone: (() => void) | null = null;
  let unlistenOpenDocs: (() => void) | null = null;
  // OS file drop-in (CPE-670): overlay shown while OS files are dragged over the window.
  let osDragActive = false;
  let unlistenOsDrop: (() => void) | null = null;
  /** A copy-paste awaiting the user's conflict choice (CPE-624). */
  let pendingCopy: { sources: string[]; count: number } | null = null;
  // Agent Watch view (CPE-399): the Project folder currently being watched (or ""), and the
  // teardown for its activity listener. Watching turns on only while the explorer is inside a
  // running agent's project, and off the moment it leaves — off means off (AGENT-WATCH.md).
  let activeWatchCwd = "";
  let unlistenActivity: (() => void) | null = null;
  /** Teardown for the before/after diff listener (CPE-744); paired with the activity listener. */
  let unlistenDiffs: (() => void) | null = null;
  /** Whether the Agent Watch activity timeline drawer is open (CPE-400). */
  let showTimeline = false;

  /** Folder whose disk-usage treemap is open (CPE-751), or null when the Space view is closed. */
  let spacePath: string | null = null;
  // Bumped after a delete from the Space analyzer so it re-scans and the freed space shows (CPE-752).
  let spaceRefresh = 0;

  /** Delete an item chosen in the Space analyzer to the Recycle Bin, then refresh the map. Confirms
      first (a treemap delete is a deliberate, possibly-large removal). Reuses delete_to_trash + the undo
      stack like the file-list delete, but leaves the explorer listing alone (the modal owns the refresh).
      Kept separate from doDelete so the file-list delete path is untouched (CPE-752). */
  function spaceDelete(item: { path: string; name: string }) {
    confirm = {
      title: "Delete to Recycle Bin?",
      message: `"${item.name}" will be moved to the Recycle Bin. You can undo this.`,
      label: "Delete",
      onYes: async () => {
        confirm = null;
        try {
          const results = await invoke<OpResult[]>("delete_to_trash", { paths: [item.path] });
          reportResults(results, "Moved to Recycle Bin:");
          if (canRestoreTrash) {
            const restored = results.filter((r) => r.ok).map((r) => ({ from: r.path, to: "" }));
            if (restored.length > 0) {
              undoStack = pushUndo(undoStack, {
                kind: "delete",
                moves: restored,
                label: `Delete "${item.name}"`,
              });
            }
          }
          spaceRefresh += 1; // tell DiskSpaceView to re-scan so the map reflects the freed space
        } catch (e) {
          showNotice(String(e), true);
        }
      },
    };
  }

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
    unlistenDiffs?.();
    unlistenDiffs = null;
    if (watchRefreshTimer) { clearTimeout(watchRefreshTimer); watchRefreshTimer = null; }
    await stopAgentWatch();
    if (cwd) {
      unlistenActivity = await initAgentActivity(onAgentBatch);
      unlistenDiffs = await initAgentDiffs();
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

  /** Open the Agent Deck in its own OS window (CPE-335) — native title bar (drag it around
      the screen), resize borders, and frame, independent of the explorer's focus. Only
      meaningful in a `sidecar-platform` build. The window loads the sidecar's loopback URL
      directly and has NO Tauri API (its label is in no capability), so the untrusted sidecar
      UI stays isolated. Capability consent is managed in Settings → Platform, not at launch
      (CPE-860). */
  /** Pending explorer→console hand-off (CPE-313): folder to scope to and a task hint,
      consumed by launchAiConsole. */
  let consoleContext: { cwd?: string; task?: string; session?: string } = {};

  async function openAiConsole(ctx: { cwd?: string; task?: string; session?: string } = {}) {
    showSettings = false;
    consoleContext = ctx;
    const existing = await WebviewWindow.getByLabel(AI_CONSOLE_LABEL);
    if (existing) {
      await existing.setFocus(); // can't re-scope a live window without disrupting sessions
      if (ctx.session) showNotice("Agent Deck is already open — click the agent's tab to focus it.", false);
      else if (ctx.cwd) showNotice("Agent Deck is already open — set the working folder in its toolbar.", false);
      return;
    }
    // CPE-860: open directly — no launch-time consent popup. On first launch grant the
    // non-sensitive requested capabilities silently (matching the old sheet's defaults) and
    // leave sensitive ones (secrets, network) ungranted for a deliberate grant in Settings →
    // Platform. Sensitive capabilities are still never granted without an explicit user action.
    const state = await consentState("ai-console");
    if (state) {
      const defaults = state.undecided.filter((c) => !CAPABILITY_INFO[c].sensitive);
      if (defaults.length > 0) {
        const granted = [...state.granted, ...defaults];
        await setConsent("ai-console", granted, granted);
      }
    }
    await launchAiConsole();
  }

  /** Open the Agent Deck focused on a specific agent session's tab (CPE-532) — from double-clicking an
      Agents leaf or its context-menu "Open". Scopes to the agent's folder + passes the session id so
      the launcher activates that tab after reattach. */
  function openSession(sessionId: string, cwd?: string) {
    openAiConsole({ cwd, session: sessionId });
  }

  async function launchAiConsole() {
    const base = await startAiConsole();
    if (!base) { showNotice("Agent Deck couldn't start — open Settings → Platform to see why and Repair it.", true); return; }
    const url = consoleUrlWith(base, consoleContext.cwd, consoleContext.task, consoleContext.session);
    try {
      const win = new WebviewWindow(AI_CONSOLE_LABEL, {
        url,
        title: "Agent Deck",
        width: 1100,
        height: 760,
        minWidth: 640,
        minHeight: 400,
        resizable: true,
        center: true,
      });
      win.once("tauri://error", () => showNotice("Couldn't open the Agent Deck window.", true));
    } catch {
      showNotice("Couldn't open the Agent Deck window.", true);
    }
  }

  /** Open the Agent Board in its own window — an app-wide singleton. When the sidecar platform is present
      it prefers the **out-of-process** board sidecar (CPE-850/853): it starts the `agent-board` sidecar
      and frames its own served UI in an **isolated** window (label `agent-board-sidecar`, in no
      capability — the untrusted sidecar UI talks to its own loopback HTTP API, not Tauri). Otherwise it
      falls back to the in-process window (CPE-844): the same bundle with `?board` (CPE-843), whose label
      `agent-board` IS in `capabilities/default.json` so the trusted BoardView can invoke ticket_board. A
      second launch focuses the existing window; size/position persist via the window-state plugin. */
  const AGENT_BOARD_LABEL = "agent-board";
  const AGENT_BOARD_SIDECAR_LABEL = "agent-board-sidecar";
  const AGENT_BOARD_WIN = {
    title: "Agent Board",
    width: 1100,
    height: 720,
    minWidth: BOARD_MIN_W,
    minHeight: BOARD_MIN_H,
    resizable: true,
    center: true,
  };
  // The out-of-process sidecar board (CPE-850/853) is a barer reimplementation — a plain text header,
  // columns + drag, but NO toolbar and NO Board⇄Epics kanban toggle. Since CPE-920 made its binary
  // reliably bundled, preferring it hid the full-featured board (missing top / can't select Epics).
  // So the Agent Board now opens the in-process BoardView (toolbar + CPE-922 epics kanban + filter +
  // archive). Flip this to re-enable the sidecar board once it reaches feature parity (CPE-926).
  const PREFER_SIDECAR_BOARD = false;
  async function openAgentBoard() {
    // Out-of-process sidecar path — disabled until the sidecar board reaches parity (CPE-926).
    if (PREFER_SIDECAR_BOARD && aiConsoleAvailable) {
      const running = await WebviewWindow.getByLabel(AGENT_BOARD_SIDECAR_LABEL);
      if (running) { await running.setFocus(); return; }
      const base = await startAgentBoard(isHome ? undefined : currentPath);
      if (base) {
        try {
          const win = new WebviewWindow(AGENT_BOARD_SIDECAR_LABEL, { url: base, ...AGENT_BOARD_WIN });
          win.once("tauri://error", () => showNotice("Couldn't open the Agent Board window.", true));
          return;
        } catch {
          showNotice("Couldn't open the Agent Board window.", true);
          return;
        }
      }
      // Sidecar unavailable — fall through to the in-process window.
    }

    // In-process window fallback (also the only path in the plain build).
    const existing = await WebviewWindow.getByLabel(AGENT_BOARD_LABEL);
    if (existing) {
      await existing.setFocus();
      return;
    }
    try {
      const win = new WebviewWindow(AGENT_BOARD_LABEL, { url: "index.html?board=1", ...AGENT_BOARD_WIN });
      win.once("tauri://error", () => showNotice("Couldn't open the Agent Board window.", true));
    } catch {
      showNotice("Couldn't open the Agent Board window.", true);
    }
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

  // Active smart folder (CPE-667): a saved tag query opened as a virtual, read-only listing. `null` when
  // not in one. `smartEntries` is the statted result of its matching paths, refreshed reactively as the
  // tag store changes so the view self-updates.
  let smartFolder: SmartFolder | null = null;
  let smartEntries: DirEntry[] = [];
  $: smartPaths = smartFolder ? smartFolderPaths($tags, smartFolder) : [];
  $: void loadSmartEntries(smartFolder, smartPaths);
  async function loadSmartEntries(sf: SmartFolder | null, paths: string[]) {
    if (!sf) { smartEntries = []; return; }
    try {
      smartEntries = await invoke<DirEntry[]>("entries_for_paths", { paths });
    } catch {
      smartEntries = [];
    }
  }
  function openSmartFolder(sf: SmartFolder) {
    smartFolder = sf;
    archive = null;
    selectedTag = "";
    search = "";
    selection = emptySelection();
  }
  function exitSmartFolder() {
    smartFolder = null;
    selection = emptySelection();
  }

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
    if (smartFolder) {
      showNotice("This is a smart folder — a saved search view. Open a file's real location to change it.", true);
      return true;
    }
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
  /** The pane instance — App calls `explorerPane.loadListing(path)` to fetch a folder (CPE-676 domino 3b). */
  let explorerPane: ExplorerPane;

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

  async function loadPath(path: string, keepSelection = false, useCache = false) {
    const previouslySelected = keepSelection
      ? selectedIndices(selection).map((i) => visible[i]?.path).filter(Boolean)
      : [];

    smartFolder = null; // navigating to a real folder exits any open smart folder (CPE-667)

    if (!keepSelection) {
      selection = emptySelection();
      search = "";
      selectedTag = ""; // a tag filter is folder-scoped; leaving the folder clears it (CPE-639)
    }
    error = "";

    // A new listing (or a refresh) invalidates the recursive-size cache so sizes recompute (CPE-750).
    if (folderSizes.size > 0) folderSizes = new Map();
    pendingSizes.clear();

    if (path === HOME) {
      entries = [];
      loading = false;
      return;
    }

    // The pane owns the streaming fetch + directory cache now (CPE-676 domino 3b) and supersedes stale
    // navigations itself, populating the bound `entries`/`loading`/`error`. A `false` return means a newer
    // navigation took over, so we skip the post-load hooks below.
    const applied = await explorerPane.loadListing(path, useCache);
    if (!applied) return;

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
    await loadPath(path, false, true); // navigation uses the listing cache (CPE-756)
  }

  /** Navigate pane B independently of pane A (dual-pane, CPE-677); persists its folder. */
  async function navigateB(path: string) {
    paneBPath = path;
    settings.savePaneBPath(path);
    await explorerPaneB?.loadListing(path, true);
  }

  /** Open an entry in pane B: descend into a folder, or open a file with the OS default (CPE-677). */
  async function openB(entry: DirEntry) {
    if (entry.is_dir) { await navigateB(entry.path); return; }
    try {
      await invoke("open_external", { path: entry.path });
      recents = settings.addRecent(recents, { path: entry.path, name: entry.name });
      settings.saveRecents(recents);
    } catch {
      showNotice(`Can't open "${entry.name}" — no app is associated with this file type.`, true);
    }
  }

  /** Toggle single ⇄ dual pane (CPE-677); persists. On first enable pane B opens pane A's folder. */
  function toggleDualPane() {
    dualPane = !dualPane;
    settings.saveDualPane(dualPane);
    if (dualPane) { activePane = 1; void navigateB(paneBPath || currentPath || homePath); }
    else activePane = 0;
  }

  // Commander keybindings (CPE-678): the active pane's selection + folder, and the opposite pane's folder.
  function commanderContext() {
    const sel = activePane === 0 ? selectedEntries : selectedEntriesB;
    return {
      sources: sel.map((e) => e.path),
      from: activePane === 0 ? currentPath : paneBPath,
      to: activePane === 0 ? paneBPath : currentPath,
    };
  }

  /** Refresh both panes after a cross-pane mutation (a move changes both folders). */
  async function refreshBothPanes() {
    await loadPath(currentPath, true);
    if (dualPane && paneBPath) void explorerPaneB?.loadListing(paneBPath, false);
  }

  /** F5: copy the active pane's selection into the other pane's folder via the transfer engine (CPE-678). */
  async function commanderCopy() {
    const { sources, to } = commanderContext();
    if (sources.length === 0 || !to) return;
    try { await startTransfer(sources, to, "copy", "keepboth"); } catch (e) { showNotice(String(e), true); }
  }

  /** F6: move the active pane's selection into the other pane's folder (CPE-678). */
  async function commanderMove() {
    const { sources, to } = commanderContext();
    if (sources.length === 0 || !to) return;
    try {
      const results = await invoke<OpResult[]>("move_entries", { paths: sources, dest: to });
      reportResults(results, "Moved");
      const moves = results
        .map((r, i) => ({ from: sources[i], to: r.path, ok: r.ok }))
        .filter((m) => m.ok)
        .map(({ from, to }) => ({ from, to }));
      if (moves.length > 0) retagMoves(moves); // tags follow the moved files (CPE-657)
      await refreshBothPanes();
    } catch (e) { showNotice(String(e), true); }
  }

  /** Swap the two panes' folders (CPE-678). */
  async function swapPanes() {
    const a = currentPath, b = paneBPath;
    if (!b) return;
    await navigateB(a);
    await navigate(b);
  }

  /** Mirror: point the inactive pane at the active pane's folder (CPE-678). */
  async function mirrorPane() {
    if (activePane === 0) await navigateB(currentPath);
    else if (paneBPath) await navigate(paneBPath);
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
    await loadPath(current(h) as string, false, true); // CPE-756: instant from cache
  }

  async function goForward() {
    if (!canGoForward(activeTab.history)) return;
    const h = forward(activeTab.history);
    setHistory(h);
    await loadPath(current(h) as string, false, true); // CPE-756: instant from cache
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
  /** Apply a rich "Select by…" criterion (CPE-782) to the visible list via the shared matcher. */
  function applySelectBy(condition: Condition) {
    selectByOpen = false;
    const idx = selectMatching(visible, condition, Date.now());
    selection = selectIndices(idx);
    showNotice(
      idx.length === 0
        ? "No items match that criterion."
        : `Selected ${idx.length} item${idx.length === 1 ? "" : "s"}.`,
    );
  }

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
    : smartFolder ? smartFolder.name
    : isHome ? "Home" : (splitPath(currentPath).at(-1)?.name ?? currentPath);

  // Folder-context detection (CPE-235): runs on the RAW listing (so hidden
  // markers like `.git` are seen regardless of the show-hidden setting).
  $: folderContexts = (isHome || archive || smartFolder) ? [] : detectContexts({ path: currentPath, entries });

  // The sort/hidden/search/type/tag pipeline that turns the base listing into `visible` (+ its pre-filter
  // `shown`) now lives in <ExplorerPane> (CPE-676 domino 2). App resolves the base list + archive/smart
  // mode and passes them down; `visible`/`shown` are bound back for the status bar + operations.

  /** All tags with counts, for the sidebar Tags section. */
  $: tagList = tagCounts($tags);


  $: crumbs = archive
    ? [{ name: "Home", path: HOME }, ...splitPath(currentPath), ...archiveCrumbs(archive)]
    : smartFolder
      ? [{ name: "Home", path: HOME }, { name: smartFolder.name, path: "" }]
      : isHome
        ? [{ name: "Home", path: HOME }]
        : [{ name: "Home", path: HOME }, ...splitPath(currentPath)];

  // `selectedEntries` is derived and owned by <ExplorerPane> now (bound above); App only consumes it.
  $: selectedSize = selectedEntries.reduce((n, e) => n + (e.is_dir ? 0 : e.size), 0);
  $: itemCount = (isHome && !smartFolder) ? places.length + drives.length + pins.length : visible.length;
  // The folder's pre-filter total, so the status bar can read "X of Y items" (CPE-407).
  $: totalCount = ((isHome && !smartFolder) || archive) ? itemCount : shown.length;
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
      // Carry any tags to the new path so they follow the file (CPE-652); best-effort.
      retagPath(path, to).catch(() => {});
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

    // COPY → the transfer engine (CPE-625): shows the operations panel; the transfer://done listener
    // refreshes + reports. keep-both preserves auto-rename. (Copies aren't undoable.)
    if (!move) {
      try {
        await startTransfer(sources, dest, "copy", "keepboth");
      } catch (e) {
        showNotice(String(e), true);
      }
      return;
    }

    // MOVE → existing synchronous path (keeps undo).
    try {
      const results = await invoke<OpResult[]>("move_entries", { paths: sources, dest });
      reportResults(results, "Moved");
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
        retagMoves(moves); // tags follow the moved files (CPE-657)
      }
      await loadPath(currentPath);
    } catch (e) {
      showNotice(String(e), true);
    }
  }

  /** After a move, carry each moved file's tags to its new path so they follow it (CPE-657).
      Best-effort + fire-and-forget; an untagged file is a cheap no-op. */
  function retagMoves(moves: { from: string; to: string }[]) {
    for (const m of moves) retagPath(m.from, m.to).catch(() => {});
  }

  /** Start a copy of `sources` into the current folder with the chosen conflict policy (CPE-624). */
  async function startCopyWithPolicy(sources: string[], policy: ConflictPolicy) {
    try {
      await startTransfer(sources, currentPath, "copy", policy);
    } catch (e) {
      showNotice(String(e), true);
    }
  }

  /** The conflict dialog's choice: run the pending copy with that policy (CPE-624). */
  function resolveCopyConflict(policy: ConflictPolicy) {
    const p = pendingCopy;
    pendingCopy = null;
    if (p) startCopyWithPolicy(p.sources, policy);
  }

  async function doPaste() {
    if (isHome || blockedInArchive() || clipEmpty(clipboard)) return;
    if (!pasteCheck.allowed) {
      showNotice(pasteCheck.reason, true);
      return;
    }
    const wasCut = clipboard.mode === "cut";
    const sources = [...clipboard.paths];

    // COPY → the transfer engine (CPE-613): progress shows in the operations panel and the
    // transfer://done listener refreshes the folder + reports. Copies aren't undoable, so there's no
    // undo coupling. If names would collide, ask how to resolve the batch (CPE-624); otherwise
    // "keepboth" preserves the old auto-rename-on-collision behaviour.
    if (!wasCut) {
      const collisions = collidingNames(sources, entries.map((e) => e.name));
      if (collisions.length > 0) {
        pendingCopy = { sources, count: collisions.length };
        return; // the conflict dialog resumes via startCopyWithPolicy
      }
      startCopyWithPolicy(sources, "keepboth");
      return;
    }

    // MOVE → the existing synchronous path: instant same-volume rename and undo support.
    try {
      const results = await invoke<OpResult[]>("move_entries", { paths: sources, dest: currentPath });
      reportResults(results, "Moved");
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
        retagMoves(moves); // tags follow the moved files (CPE-657)
      }
      clipboard = emptyClipboard();
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

  /** "Work on this" — open the Agent Deck scoped to the selection (CPE-313). A single
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
  /** The drop-path of the folder row / sidebar place under a physical cursor position, or "" (CPE-670).
      Physical pixels → CSS pixels via the device pixel ratio before hit-testing the DOM. */
  function folderUnderCursor(pos: { x: number; y: number }): string {
    const dpr = window.devicePixelRatio || 1;
    const el = document.elementFromPoint(pos.x / dpr, pos.y / dpr);
    const target = el?.closest?.("[data-drop-path]") as HTMLElement | null;
    return target?.dataset.dropPath ?? "";
  }

  /** Copy OS files dropped onto the window (CPE-670) into the folder under the cursor, else the current
      folder. Always a COPY — the external originals must stay put. */
  async function importDroppedFiles(paths: string[], pos: { x: number; y: number }) {
    if (!paths || paths.length === 0) return;
    const dest = folderUnderCursor(pos) || (isHome || archive || smartFolder ? "" : currentPath);
    if (!dest) {
      showNotice($t("dnd.openFolderToImport"), true);
      return;
    }
    // Through the transfer engine (CPE-671) so a large OS import shows tracked progress; keepboth
    // auto-renames on collision. The transfer://done listener refreshes the folder + reports.
    try {
      await startTransfer(paths, dest, "copy", "keepboth");
    } catch (e) {
      showNotice(String(e), true);
    }
  }

  async function dropInto(paths: string[], dest: string, mods: { ctrlKey: boolean; shiftKey: boolean }) {
    if (paths.length === 0 || !dest) return;

    // Copy-vs-move follows the OS convention (CPE-669): a modifier overrides, else same-volume moves and
    // cross-volume copies. same_volume is best-effort — on error it returns false → copy (never loses src).
    let sameVolume: boolean | null = null;
    if (!mods.ctrlKey && !mods.shiftKey) {
      sameVolume = await invoke<boolean>("same_volume", { a: paths[0], b: dest }).catch(() => false);
    }
    const copy = resolveEffect(mods, sameVolume) === "copy";

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

    // COPY → the transfer engine (CPE-671), mirroring paste: tracked progress in the operations panel,
    // the transfer://done listener refreshes + reports, and "keepboth" auto-renames on collision. (Copies
    // aren't undoable.)
    if (copy) {
      try {
        await startTransfer(paths, dest, "copy", "keepboth");
      } catch (e) {
        showNotice(String(e), true);
      }
      return;
    }

    // MOVE → synchronous path (fast same-folder-volume renames) so undo + tag-follow stay intact.
    try {
      const results = await invoke<OpResult[]>("move_entries", { paths, dest });
      reportResults(results, "Moved");
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
        retagMoves(moves); // tags follow the moved files (CPE-657)
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

  /** Select every visible entry (CPE-605). A named function so the palette command can reference it
      without textually assigning `selection` inside the reactive `paletteCommands` block — that would
      make Svelte see a write and form a selection ⇄ selectedEntries cycle. */
  function selectAllVisible() {
    selection = selectAll(visible.length);
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
      case "tags": if (selectedEntries.length >= 1) tagEditorFor = [...selectedEntries]; break;
      case "new-folder": newFolder(); break;
      case "new-file": newFile(); break;
      case "select-all": selection = selectAll(visible.length); break;
      case "invert-selection": selection = invertSelection(selection, visible.length); break;
      case "select-pattern": patternSelectOpen = true; break;
      case "color-rules": colorRulesOpen = true; break;
      case "select-type": {
        const e = selectedEntries[0];
        if (e && !e.is_dir) selection = selectIndices(sameTypeIndices(visible, e.extension));
        break;
      }
      case "refresh": refresh(); break;
      case "help-docs": openDocs(currentSection()); break;
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

    // Quick-look owns the keyboard while open (CPE-645).
    if (quickLook) {
      if (event.key === "Escape" || event.key === " ") { event.preventDefault(); quickLook = null; }
      else if (event.key === "ArrowRight") { event.preventDefault(); quickLookMove(1); }
      else if (event.key === "ArrowLeft") { event.preventDefault(); quickLookMove(-1); }
      return;
    }

    const ctrl = event.ctrlKey || event.metaKey;
    // Dual-pane (CPE-677): plain Tab switches the active pane. Single-pane (dualPane off) leaves Tab's
    // default focus traversal untouched.
    if (dualPane && !ctrl && !event.altKey && event.key === "Tab") {
      event.preventDefault();
      activePane = activePane === 0 ? 1 : 0;
      return;
    }
    // Commander keys (CPE-678): F5 copy / F6 move the active selection to the other pane; Ctrl+U swaps.
    if (dualPane && !ctrl && !event.altKey && event.key === "F5") { event.preventDefault(); void commanderCopy(); return; }
    if (dualPane && !ctrl && !event.altKey && event.key === "F6") { event.preventDefault(); void commanderMove(); return; }
    if (dualPane && ctrl && !event.altKey && event.key.toLowerCase() === "u") { event.preventDefault(); void swapPanes(); return; }
    // Space quick-looks the selected image (CPE-645).
    if (!ctrl && !event.altKey && !event.shiftKey && event.key === " " && openQuickLook()) { event.preventDefault(); return; }

    if (ctrl && event.key.toLowerCase() === "l") { event.preventDefault(); editingPath = true; return; }
    if (event.altKey && event.key.toLowerCase() === "d") { event.preventDefault(); editingPath = true; return; }
    if (ctrl && event.key.toLowerCase() === "f") { event.preventDefault(); navToolbar?.focusSearch(); return; }
    if (ctrl && event.shiftKey && event.key.toLowerCase() === "n") { event.preventDefault(); newFolder(); return; }
    if (ctrl && event.shiftKey && event.key.toLowerCase() === "o") { event.preventDefault(); popOutPreview(); return; }
    if (ctrl && event.shiftKey && event.key.toLowerCase() === "t") { event.preventDefault(); reopenClosedTab(); return; }
    if (ctrl && event.shiftKey && event.key.toLowerCase() === "f") { event.preventDefault(); if (!isHome && !archive) contentSearchOpen = true; return; }
    if (ctrl && event.shiftKey && event.key.toLowerCase() === "p") { event.preventDefault(); paletteOpen = true; return; } // command palette (CPE-602)
    if (ctrl && !event.shiftKey && event.key.toLowerCase() === "p") { event.preventDefault(); if (!isHome && !archive) fileSearchOpen = true; return; } // find files by name (CPE-603)
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
      case "F1": event.preventDefault(); openDocs(currentSection()); break; // help for this section (CPE-596)
      case "?": event.preventDefault(); shortcutsOpen = true; break; // keyboard shortcuts (moved off F1)
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
        selection = moveLead(selection, arrowDelta("ArrowDown", currentGridCols()), visible.length, event.shiftKey);
        break;
      case "ArrowUp":
        event.preventDefault();
        selection = moveLead(selection, arrowDelta("ArrowUp", currentGridCols()), visible.length, event.shiftKey);
        break;
      case "ArrowRight":
      case "ArrowLeft": {
        // 2-D grid nav (CPE-769): in icons/gallery, Left/Right move the lead by one tile (moveLead wraps
        // across rows + clamps). In list/details (single column) they're left unhandled — no horizontal move.
        const gcols = currentGridCols();
        if (gcols > 1) {
          event.preventDefault();
          selection = moveLead(selection, arrowDelta(event.key, gcols), visible.length, event.shiftKey);
        }
        break;
      }
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
    colorRules = settings.loadColorRules();
  }

  /** Persist + apply an edited rule set from the color-rules editor (CPE-776). */
  function applyColorRules(rules: ColorRule[]) {
    colorRules = rules;
    settings.saveColorRules(rules);
  }

  /** Capture the current window's tabs as workspace tabs (CPE-788): each open tab's path + the current
      view/sort/filter (which are global in this app's model). */
  function captureCurrentTabs(): WorkspaceTab[] {
    return tabs.map((t) => ({
      path: (current(t.history) ?? HOME) as string,
      view,
      sortKey,
      sortDir,
      filter: search,
    }));
  }

  /** Apply a saved workspace (CPE-788): reopen its tabs and adopt the first tab's view/sort/filter. */
  function switchWorkspace(ws: Workspace) {
    workspacesOpen = false;
    if (ws.tabs.length === 0) return;
    if (archive) exitArchive();
    tabs = ws.tabs.map((wt) => ({ id: nextTabId++, history: createHistory(wt.path) }));
    activeId = tabs[0].id;
    const first = ws.tabs[0];
    if (first.view) { view = first.view; settings.saveView(view); }
    if (first.sortKey) { sortKey = first.sortKey; settings.saveSortKey(sortKey); }
    if (first.sortDir) { sortDir = first.sortDir; settings.saveSortDir(sortDir); }
    search = first.filter ?? "";
    loadPath((current(tabs[0].history) ?? HOME) as string);
  }

  /** Launch-time auto-restore (CPE-789): if enabled and a last session was saved, reopen its tabs —
      dropping any whose path no longer exists (moved/deleted) via `pruneMissing`, so restore never fails.
      Returns whether it actually restored anything (so startup can fall back to the default HOME tab). */
  async function restoreLastSession(): Promise<boolean> {
    if (!autoRestore) return false;
    const saved = settings.loadLastSession();
    if (saved.length === 0) return false;
    const existing = new Set<string>();
    await Promise.all(
      saved.map(async (t) => {
        try {
          await rawInvoke("entry_info", { path: t.path }); // rawInvoke: startup restore shows no busy cursor
          existing.add(t.path);
        } catch {
          // path gone — pruneMissing drops it
        }
      }),
    );
    const pruned = pruneMissing({ id: "last", name: "last", tabs: saved }, (p) => existing.has(p));
    if (pruned.tabs.length === 0) return false;
    switchWorkspace(pruned); // reuses the workspace restore path (sets tabs + view/sort/filter + loads)
    return true;
  }

  /** Fill recursive sizes for the given folder paths on demand (CPE-750). Called by FileList for the
      folders currently on screen that aren't cached yet; dedups in-flight `dir_size` calls and reassigns
      the Map so the column + size-sort react. rawInvoke so the lazy fill never raises the busy cursor. */
  async function fillFolderSizes(paths: string[]) {
    for (const path of paths) {
      if (folderSizes.has(path) || pendingSizes.has(path)) continue;
      pendingSizes.add(path);
      rawInvoke<number>("dir_size", { path })
        .then((size) => {
          folderSizes.set(path, size);
          folderSizes = folderSizes; // trigger Svelte reactivity on the Map
        })
        .catch(() => {
          folderSizes.set(path, 0); // unreadable subtree → 0, so the row stops showing "…"
          folderSizes = folderSizes;
        })
        .finally(() => pendingSizes.delete(path));
    }
  }

  /** Toggle the recursive folder-size column (CPE-750), persisting the choice. */
  function toggleFolderSizes() {
    showFolderSizes = !showFolderSizes;
    settings.saveShowFolderSizes(showFolderSizes);
  }

  /** Enable/disable auto-restore (CPE-789). Turning it on immediately captures the current session so a
      crash/close before the next navigation still has something to restore. */
  function setAutoRestore(on: boolean) {
    autoRestore = on;
    settings.saveAutoRestore(on);
    if (on) settings.saveLastSession(captureCurrentTabs());
  }

  // Continuously persist the open session once startup restore has run — but only while the feature is on,
  // so with it off startup is byte-for-byte unchanged. Referencing the tab/view/sort/filter vars makes this
  // reactive block re-run whenever any of them change.
  $: if (sessionReady && autoRestore) {
    void [tabs, currentPath, view, sortKey, sortDir, search];
    settings.saveLastSession(captureCurrentTabs());
  }

  /** (Re)start or stop the live watched-folder watcher to match the current config (CPE-794). Only the
      sidecar build has the backend; a no-op fails soft elsewhere. */
  async function reconcileWatch() {
    if (watchLive && watchedFolders.length && aiConsoleAvailable) {
      await startFolderWatch(watchedFolders, () => watchRules, (fire) => {
        watchLog = [fire, ...watchLog].slice(0, 50);
        showNotice(`Watch: ${fire.summary}`);
      });
    } else {
      await stopFolderWatch();
    }
  }

  /** Undo a watched-folder rule fire (CPE-794): reverse the move/copy, then drop it from the log. */
  async function undoWatchFire(fire: WatchFire) {
    try {
      await undoFire(fire);
      watchLog = watchLog.filter((f) => f.id !== fire.id);
      showNotice(`Undid: ${fire.rule}`);
    } catch (e) {
      showNotice(String(e), true);
    }
  }

  /** Persist + apply watched-folder config from the editor (CPE-794). */
  function applyWatchConfig(folders: string[], live: boolean) {
    watchedFolders = folders;
    watchLive = live;
    settings.saveWatchedFolders(folders);
    void reconcileWatch();
  }

  /** Open the file-attributes editor (CPE-786) for the single selected entry. */
  function openAttributes() {
    if (selectedEntries.length === 0) {
      showNotice("Select one or more items to edit their attributes.");
      return;
    }
    attrTargets = selectedEntries.map((e) => ({ path: e.path, name: e.name, modifiedMs: e.modified }));
    attributesOpen = true;
  }

  /** Open the folder-compare view (CPE-779). Pre-fills the two paths when exactly two folders are
      selected; otherwise the user types/pastes them in the dialog. */
  function openCompare() {
    const dirs = selectedEntries.filter((e) => e.is_dir);
    if (selectedEntries.length === 2 && dirs.length === 2) {
      compareLeft = dirs[0].path;
      compareRight = dirs[1].path;
    } else {
      compareLeft = "";
      compareRight = "";
    }
    compareOpen = true;
  }

  /** Save an audit-log export (CPE-801) to a user-chosen file, reusing the tags-export save flow. */
  async function exportAuditToFile(payload: { format: string; ext: string; content: string }) {
    try {
      const path = await saveFileDialog({
        defaultPath: `audit.${payload.ext}`,
        filters: [{ name: payload.format.toUpperCase(), extensions: [payload.ext] }],
      });
      if (!path) return;
      await invoke("write_file_text", { path, contents: payload.content });
      showNotice(`Exported ${path.split(/[\\/]/).pop()}.`);
    } catch (e) {
      showNotice(String(e), true);
    }
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
      case "documents": openDocs(currentSection()); break;
      case "diagnostics": diagnostics = !diagnostics; settings.saveDiagnostics(diagnostics); break;
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

  /** Export the whole tag store to a JSON file (CPE-654). */
  async function exportTagsToFile() {
    try {
      const path = await saveFileDialog({ defaultPath: "tags.json", filters: [{ name: "JSON", extensions: ["json"] }] });
      if (!path) return;
      await invoke("write_file_text", { path, contents: exportTags() });
      showNotice(`Tags exported to ${path.split(/[\\/]/).pop()}.`);
    } catch (e) {
      showNotice(String(e), true);
    }
  }

  /** Import a tag store JSON file, merged into the current tags (CPE-654). */
  async function importTagsFromFile() {
    try {
      const picked = await openFolderDialog({ directory: false, multiple: false, filters: [{ name: "JSON", extensions: ["json"] }] });
      if (!picked || typeof picked !== "string") return;
      const json = await invoke<string>("read_file_text", { path: picked, maxBytes: 16 * 1024 * 1024 });
      await importTags(json);
      showNotice("Tags imported.");
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
    // Opt-in integrity monitor (CPE-872): if enabled, verify all baselined folders once, a beat after
    // startup so it never blocks first paint. Reuses the tested verify + summary-notice path.
    if (verifyOnStartup && Object.keys(integrityBaselines).length > 0) {
      setTimeout(() => { void verifyAllBaselines(); }, 1500);
    }
    // …and re-check periodically while the app stays open (CPE-875) — same opt-in toggle, so a long-running
    // session still catches silent corruption without a restart.
    if (verifyOnStartup) {
      verifyTimer = setInterval(() => {
        if (Object.keys(integrityBaselines).length > 0) void verifyAllBaselines();
      }, VERIFY_INTERVAL_MS);
    }
    // Reveal the Agent Deck button only when the sidecar platform is present (CPE-351).
    platformActive().then((v) => (aiConsoleAvailable = v)).catch(() => {});
    // Listen for coding-agent sessions launched from the console so the explorer can surface
    // them (Agent Watch, CPE-396). Idle until a session announces itself; unlistened on teardown.
    initAgentSessions().then((un) => (unlistenSessions = un)).catch(() => {});

    // Transfer manager (CPE-613): consume progress events, and on completion refresh the current
    // folder (a copy may have landed here) + report the outcome. Idle until a transfer starts.
    initTransfers().catch(() => {});
    // Tag store (CPE-636): load persisted tags/labels once so rows can show chips + tints. Idle
    // (empty) until something is actually tagged, so the plain explorer is unaffected.
    initTags().catch(() => {});
    listen<TransferReport>("transfer://done", (e) => {
      const r = e.payload;
      loadPath(currentPath).catch(() => {});
      if (r.cancelled) showNotice("Copy cancelled.");
      else if (r.failed > 0) showNotice(`Copied ${r.transferred}, ${r.failed} failed.`, true);
      else showNotice(`Copied ${r.transferred} item${r.transferred === 1 ? "" : "s"}.`);
    }).then((un) => (unlistenTransferDone = un)).catch(() => {});

    // Open the regular Documents dialog when another window (e.g. the Agent Deck's area "?" help) asks
    // for a specific doc section, instead of that window showing its own inline help panel (CPE-929).
    // Bring the main window forward so the dialog is actually visible.
    listen<{ slug?: string }>("open-docs", (e) => {
      const slug = e.payload?.slug;
      if (slug) openDocsSlug(slug);
      getCurrentWindow().setFocus().catch(() => {});
    }).then((un) => (unlistenOpenDocs = un)).catch(() => {});

    // OS file drop-in (CPE-670): files dragged from the desktop/Explorer onto the window are copied into
    // the folder under the cursor (else the current folder). A themed overlay shows while dragging over.
    // Guarded: outside a Tauri webview (e.g. the jsdom test env) this API is absent — drop-in is then
    // simply unavailable and must not break startup.
    try {
      getCurrentWebview()
        .onDragDropEvent((e) => {
          const p = e.payload;
          if (p.type === "enter" || p.type === "over") osDragActive = true;
          else if (p.type === "leave") osDragActive = false;
          else if (p.type === "drop") {
            osDragActive = false;
            importDroppedFiles(p.paths, p.position);
          }
        })
        .then((un) => (unlistenOsDrop = un))
        .catch(() => {});
    } catch {
      /* no webview API available — OS drop-in unavailable */
    }

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

    const restored = await restoreLastSession();
    if (!restored) await loadPath(HOME);
    sessionReady = true; // from here on, session changes are captured (CPE-789)
    // Dual-pane (CPE-679): when the split was last active, restore pane B to its persisted folder so the
    // layout comes back where the user left it (pane A is covered by restoreLastSession above). Reuses the
    // `paneBPath` persistence from CPE-677.
    if (dualPane) {
      await tick(); // pane B's `{#if dualPane}` block is rendered → explorerPaneB is bound
      void navigateB(paneBPath || homePath);
    }
    checkForUpdates();

    // Auto-mirror scheduler (CPE-497): a 60s tick + a window-focus check. Both funnel through
    // maybeAutoSync, which no-ops unless the current repo opted in and its interval has elapsed.
    autoMirrorTimer = setInterval(maybeAutoSync, 60_000);
    window.addEventListener("focus", maybeAutoSync);

    // Drive-connect scheduler (CPE-797): starts polling only if a backup job opted into auto-run.
    reconcileDriveScheduler();
  });

  onDestroy(() => {
    if (verifyTimer) clearInterval(verifyTimer); // CPE-875: stop the periodic integrity re-verify
    unlistenSessions?.();
    unlistenTransferDone?.();
    unlistenOpenDocs?.();
    unlistenOsDrop?.();
    unlistenActivity?.();
    if (watchRefreshTimer) clearTimeout(watchRefreshTimer);
    if (autoMirrorTimer) clearInterval(autoMirrorTimer);
    window.removeEventListener("focus", maybeAutoSync);
    stopDriveScheduler();
  });
</script>

<svelte:window on:keydown={handleKeydown} />

<MenuBar {diagnostics} on:select={(e) => onMenuSelect(e.detail)} />

<Toolbar label={$t("tb.application")}>
  <svelte:fragment slot="actions">
    <!-- The out-of-process apps (Agent Board / Repositories / Agent Deck) live in their own toolbar
         section (CPE-857): a `role="group"` cluster delimited by a leading divider, so future
         non-app toolbar buttons stay visibly separate from the apps. -->
    <div class="tb-sidecar-group" role="group" aria-label="Apps">
    <!-- Agent Board — opens the standalone board window (CPE-846). Always shown (the board works in
         every build), and sits just left of the Agent Deck button. -->
    <button
      class="tb-board"
      type="button"
      title={$t("palette.openAgentBoardWindow")}
      on:click={() => openAgentBoard()}
    >
      <Icon name="documents" size={15} /> Agent Board
    </button>
    {#if aiConsoleAvailable}
      <!-- Repositories — the repos sidecar UI (CPE-855). Grouped with the other out-of-process apps;
           shown only when the sidecar platform is active, like the Agent Deck button. -->
      <button
        class="tb-repos"
        type="button"
        title={$t("sidebar.repositories")}
        on:click={() => (showRepos = true)}
      >
        <Icon name="code" size={15} /> {$t("sidebar.repositories")}
      </button>
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
    </div>
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
    <span>{$t("cmd.folderSizes")}</span>
    <input type="checkbox" data-testid="folder-sizes-toggle" bind:checked={showFolderSizes}
      on:change={() => settings.saveShowFolderSizes(showFolderSizes)} />
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
  on:help={() => openDocs(currentSection())}
  on:diskusage={() => { if (inFolder()) spacePath = currentPath; }}
  on:navigate={(e) => onCrumbNavigate(e.detail)}
  on:search={(e) => { search = e.detail; selection = emptySelection(); }}
  on:searchDocs={() => openDocsSlug("12-search")}
  on:searchDeep={(e) => {
    if (isHome) { showNotice("Open a folder first — Search looks inside the current folder and its subfolders.", false); return; }
    deepSearchQuery = e.detail; fileSearchOpen = true;
  }}
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
  style="grid-template-columns: {effectiveGridCols}"
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
      {tagList}
      {selectedTag}
      smartFolders={$smartFolders}
      activeSmartFolder={smartFolder?.id ?? ""}
      on:filterTag={(e) => (selectedTag = selectedTag === e.detail ? "" : e.detail)}
      on:tagMenu={(e) => (tagMenu = e.detail)}
      on:openSmartFolder={(e) => openSmartFolder(e.detail)}
      on:smartFolderMenu={(e) => (smartFolderMenu = e.detail)}
      on:navigate={(e) => { if (archive) exitArchive(); navigate(e.detail); }}
      on:openFile={(e) => openRecent(e.detail)}
      on:home={() => { if (archive) exitArchive(); navigate(HOME); }}
      on:repos={() => (showRepos = true)}
      on:board={() => (showBoard = true)}
      on:workbench={() => (showWorkbench = true)}
      on:openSession={(e) => openSession(e.detail.sessionId, e.detail.cwd)}
      on:agentMenu={(e) => (agentMenu = { x: e.detail.x, y: e.detail.y, label: $t("tb.closeAllConsoles"), sessionId: e.detail.sessionId, sessionLabel: e.detail.sessionLabel })}
      on:drop={(e) => dropInto(e.detail.paths, e.detail.dest, e.detail)}
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
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
  <div class="pane-col" class:pane-active={dualPane && activePane === 0} on:click={() => (activePane = 0)}>
    <ExplorerPane
      bind:this={explorerPane}
      inHome={isHome && !smartFolder}
      {places}
      {drives}
      {pins}
      {recents}
      {favorites}
      {recentFolders}
      {activeWatchCwd}
      {watchedAgentName}
      {recentChanges}
      bind:showTimeline
      bind:entries
      smartOverride={smartFolder ? smartEntries : null}
      archiveOverride={archive ? archiveChildren(archive) : null}
      {search}
      {fileFilter}
      {foldersFirst}
      bind:visible
      bind:shown
      bind:selectedTag
      bind:error
      bind:loading
      {cutPaths}
      {colorRules}
      {showFolderSizes}
      {folderSizes}
      on:needSizes={(e) => fillFolderSizes(e.detail)}
      bind:renamingPath
      {renameValue}
      canDrag={!archive}
      bind:view
      bind:showHidden
      {folderContexts}
      bind:sortKey
      bind:sortDir
      bind:columnWidths
      bind:selection
      bind:selectedEntries
      bind:draggedPaths
      bind:rowEls
      on:contextAction={(e) => handleContextAction(e.detail)}
      on:navigate={(e) => navigate(e.detail)}
      on:openRecent={(e) => openRecent(e.detail)}
      on:unpin={(e) => { pins = settings.togglePin(pins, e.detail); settings.savePins(pins); }}
      on:unfavorite={(e) => { favorites = favorites.filter((f) => f.path !== e.detail); settings.saveFavorites(favorites); }}
      on:removeRecent={(e) => { recents = settings.removeRecent(recents, e.detail); settings.saveRecents(recents); }}
      on:removeRecentFolder={(e) => { recentFolders = settings.removeRecent(recentFolders, e.detail); settings.saveRecentFolders(recentFolders); }}
      on:clearRecents={() => { recents = []; settings.saveRecents(recents); }}
      on:open={(e) => open(e.detail)}
      on:rowContext={(e) => onRowContext(e.detail)}
      on:contextEmpty={(e) => (ctx = { x: e.detail.x, y: e.detail.y, target: "empty" })}
      on:commitRename={(e) => commitRename(e.detail)}
      on:drop={(e) => dropInto(e.detail.paths, e.detail.dest, e.detail)}
    />
  </div>

  {#if dualPane}
    <!-- Dual-pane (CPE-677): pane B reuses the preview grid slot; inert divider (both columns 1fr). -->
    <div class="resizer" aria-hidden="true"></div>
    <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
    <div class="pane-col" class:pane-active={activePane === 1} on:click={() => (activePane = 1)}>
      <ExplorerPane
        bind:this={explorerPaneB}
        bind:entries={entriesB}
        bind:visible={visibleB}
        bind:shown={shownB}
        bind:loading={loadingB}
        bind:error={errorB}
        bind:selection={selectionB}
        bind:selectedEntries={selectedEntriesB}
        {places}
        {drives}
        {pins}
        {recents}
        {favorites}
        {recentFolders}
        {colorRules}
        {folderContexts}
        {view}
        {sortKey}
        {sortDir}
        {foldersFirst}
        {showHidden}
        canDrag={false}
        on:open={(e) => openB(e.detail)}
        on:navigate={(e) => navigateB(e.detail)}
        on:openRecent={(e) => openRecent(e.detail)}
      />
    </div>
  {:else if showDetails}
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
  on:resolve={() => (conflictDialogPath = currentPath)}
/>

{#if syncDialogPath}
  <SyncDialog
    path={syncDialogPath}
    on:done={() => { refreshGitStatus(currentPath); refresh(); }}
    on:resolve={() => { syncDialogPath = null; conflictDialogPath = currentPath; }}
    on:close={() => (syncDialogPath = null)}
  />
{/if}

{#if conflictDialogPath}
  <ConflictDialog
    path={conflictDialogPath}
    on:done={() => { refreshGitStatus(currentPath); refresh(); }}
    on:close={() => (conflictDialogPath = null)}
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

{#if spacePath}
  <DiskSpaceView
    path={spacePath}
    refreshToken={spaceRefresh}
    on:navigate={(e) => { spacePath = null; navigate(e.detail); }}
    on:reveal={(e) => { spacePath = null; revealFileInApp(e.detail); }}
    on:delete={(e) => spaceDelete(e.detail)}
    on:help={(e) => openDocs(e.detail)}
    on:close={() => (spacePath = null)}
  />
{/if}

<!-- Diagnostics overlay (CPE-758): on-screen timing of every OS call, toggled from Application → Diagnostics. -->
{#if diagnostics}
  <DiagnosticsOverlay version={appVersion} />
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

{#if tagEditorFor}
  <TagEditor
    paths={tagEditorFor.map((e) => e.path)}
    name={tagEditorFor.length === 1 ? tagEditorFor[0].name : ""}
    count={tagEditorFor.length}
    on:close={() => (tagEditorFor = null)}
  />
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
    on:help={() => openDocsSlug("12-search")}
    on:navigate={(e) => { contentSearchOpen = false; revealFileInApp(e.detail); }}
    on:close={() => (contentSearchOpen = false)}
  />
{/if}

{#if fileSearchOpen}
  <FileNameSearchDialog
    root={currentPath}
    initialQuery={deepSearchQuery}
    on:help={() => openDocsSlug("12-search")}
    on:navigate={(e) => { fileSearchOpen = false; revealFileInApp(e.detail); }}
    on:close={() => { fileSearchOpen = false; deepSearchQuery = ""; }}
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

{#if showBoard}
  <BoardView
    root={currentPath}
    on:launch={(e) => openAiConsole({ cwd: currentPath, task: e.detail.task })}
    on:help={(e) => openDocs(e.detail)}
    on:popout={() => { showBoard = false; void openAgentBoard(); }}
    on:close={() => (showBoard = false)}
  />
{/if}

{#if showWorkbench}
  <WorkbenchView
    root={currentPath}
    on:browse={(e) => openBrowserWindow(e.detail)}
    on:edit={(e) => { openRecent(e.detail); showWorkbench = false; }}
    on:help={(e) => openDocs(e.detail)}
    on:close={() => (showWorkbench = false)}
  />
{/if}

{#if showDocs}
  <DocsView initialSlug={docsSlug} on:close={() => (showDocs = false)} />
{/if}

<TransferPanel />

{#if quickLook}
  <QuickLook
    images={quickLook.images}
    index={quickLook.index}
    on:prev={() => quickLookMove(-1)}
    on:next={() => quickLookMove(1)}
    on:close={() => (quickLook = null)}
  />
{/if}

{#if pendingCopy}
  <TransferConflictDialog
    count={pendingCopy.count}
    on:choose={(e) => resolveCopyConflict(e.detail)}
    on:cancel={() => (pendingCopy = null)}
  />
{/if}

{#if paletteOpen}
  <CommandPalette commands={paletteCommands} on:close={() => (paletteOpen = false)} />
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
    on:open={(e) => { openSession(e.detail); agentMenu = null; }}
    on:close={() => (agentMenu = null)}
  />
{/if}

{#if tagMenu}
  <TagMenu
    x={tagMenu.x}
    y={tagMenu.y}
    tag={tagMenu.tag}
    on:rename={(e) => { const old = tagMenu?.tag ?? ""; if (selectedTag === old) selectedTag = e.detail; renameTag(old, e.detail).catch((err) => showNotice(String(err), true)); tagMenu = null; }}
    on:remove={() => { const tg = tagMenu?.tag ?? ""; if (selectedTag === tg) selectedTag = ""; deleteTag(tg).catch((err) => showNotice(String(err), true)); tagMenu = null; }}
    on:saveSmart={() => { const tg = tagMenu?.tag ?? ""; if (tg) { saveSmartFolder(tg, tg); showNotice($t("smart.saved", { name: tg })); } tagMenu = null; }}
    on:close={() => (tagMenu = null)}
  />
{/if}

{#if smartFolderMenu}
  <SmartFolderMenu
    x={smartFolderMenu.x}
    y={smartFolderMenu.y}
    name={smartFolderMenu.name}
    on:rename={(e) => { renameSmartSaved(smartFolderMenu?.id ?? "", e.detail); if (smartFolder && smartFolder.id === smartFolderMenu?.id) smartFolder = { ...smartFolder, name: e.detail }; smartFolderMenu = null; }}
    on:remove={() => { const id = smartFolderMenu?.id ?? ""; if (smartFolder?.id === id) exitSmartFolder(); removeSmartSaved(id); smartFolderMenu = null; }}
    on:close={() => (smartFolderMenu = null)}
  />
{/if}

{#if patternSelectOpen}
  <PatternSelectDialog
    on:submit={(e) => selectByPattern(e.detail)}
    on:cancel={() => (patternSelectOpen = false)}
  />
{/if}

{#if colorRulesOpen}
  <ColorRulesDialog
    rules={colorRules}
    on:change={(e) => (colorRules = e.detail)}
    on:save={(e) => { applyColorRules(e.detail); colorRulesOpen = false; }}
    on:cancel={() => { colorRules = settings.loadColorRules(); colorRulesOpen = false; }}
  />
{/if}

{#if sessionHistoryOpen}
  <SessionHistoryDialog
    home={homePath}
    on:export={(e) => exportAuditToFile(e.detail)}
    on:cancel={() => (sessionHistoryOpen = false)}
  />
{/if}

{#if compareOpen}
  <CompareDialog
    initialLeft={compareLeft}
    initialRight={compareRight}
    on:cancel={() => (compareOpen = false)}
  />
{/if}

{#if selectByOpen}
  <SelectByDialog
    on:submit={(e) => applySelectBy(e.detail)}
    on:cancel={() => (selectByOpen = false)}
  />
{/if}

{#if watchRulesOpen}
  <WatchRulesDialog
    rules={watchRules}
    {watchedFolders}
    {watchLive}
    {watchLog}
    watchAvailable={aiConsoleAvailable}
    on:save={(e) => { watchRules = e.detail; settings.saveWatchRules(watchRules); void reconcileWatch(); watchRulesOpen = false; }}
    on:watchConfig={(e) => applyWatchConfig(e.detail.folders, e.detail.live)}
    on:undo={(e) => void undoWatchFire(e.detail)}
    on:cancel={() => (watchRulesOpen = false)}
  />
{/if}

{#if workspacesOpen}
  <WorkspacesDialog
    {workspaces}
    {autoRestore}
    currentTabs={captureCurrentTabs()}
    on:change={(e) => { workspaces = e.detail; settings.saveWorkspaces(workspaces); }}
    on:switch={(e) => switchWorkspace(e.detail)}
    on:autoRestore={(e) => setAutoRestore(e.detail)}
    on:cancel={() => (workspacesOpen = false)}
  />
{/if}

{#if backupOpen}
  <BackupDashboard
    jobs={backupJobs}
    history={backupHistory}
    on:change={(e) => { backupJobs = e.detail; settings.saveBackupJobs(backupJobs); reconcileDriveScheduler(); }}
    on:run={(e) => recordBackupRun(e.detail.jobId, e.detail.status)}
    on:cancel={() => (backupOpen = false)}
  />
{/if}

{#if attributesOpen}
  <AttributesDialog
    targets={attrTargets}
    on:applied={() => refresh()}
    on:cancel={() => (attributesOpen = false)}
  />
{/if}

{#if integrityOpen}
  <IntegrityDialog
    initialPath={isHome || archive ? "" : currentPath}
    baselines={integrityBaselines}
    {verifyOnStartup}
    on:baseline={(e) => {
      integrityBaselines = { ...integrityBaselines, [e.detail.path]: e.detail.entries };
      settings.saveIntegrityBaselines(integrityBaselines);
    }}
    on:setVerifyOnStartup={(e) => { verifyOnStartup = e.detail; settings.saveVerifyOnStartup(verifyOnStartup); }}
    on:cancel={() => (integrityOpen = false)}
  />
{/if}

{#if osDragActive}
  <!-- OS file drop-in overlay (CPE-670): shown while files are dragged in from the desktop/Explorer. -->
  <div class="os-drop-overlay" aria-hidden="true">
    <div class="os-drop-card">
      <Icon name="folder" size={30} />
      <span>{$t("dnd.dropToImport")}</span>
    </div>
  </div>
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
  /* Dual-pane (CPE-677): the focused pane gets an accent inset ring so it's clear which pane the
     toolbar/keyboard acts on. The ::after is pointer-events:none so it never blocks clicks. */
  .pane-col {
    position: relative;
  }
  .pane-active::after {
    content: "";
    position: absolute;
    inset: 0;
    pointer-events: none;
    box-shadow: inset 0 0 0 2px var(--accent, #4a8cff);
    z-index: 5;
  }

  /* OS file drop-in overlay (CPE-670): a themed full-window affordance while dragging files in. */
  .os-drop-overlay {
    position: fixed;
    inset: 0;
    z-index: 300;
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    border: 3px dashed var(--accent);
    pointer-events: none;
  }
  .os-drop-card {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 14px 22px;
    border-radius: 12px;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.25);
    color: var(--text);
    font-size: 15px;
    font-weight: 600;
  }

  /* The out-of-process apps form one delimited toolbar section (CPE-857): a leading divider marks the
     section boundary so future non-app toolbar buttons stay visibly separate from the apps. The first
     button keeps its own margin-left; the divider + padding give the section its edge. */
  .tb-sidecar-group {
    display: inline-flex;
    align-items: center;
    padding-left: 8px;
    margin-left: 6px;
    border-left: 1px solid var(--border-strong);
  }

  /* Out-of-process app buttons on the Application toolbar — Agent Deck (CPE-351), Agent Board
     (CPE-846), Repositories (CPE-855) — all share one toolbar-action style. */
  .tb-console,
  .tb-board,
  .tb-repos {
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
  .tb-console:hover,
  .tb-board:hover,
  .tb-repos:hover {
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
