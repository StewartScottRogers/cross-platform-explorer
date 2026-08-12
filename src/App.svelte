<script lang="ts">
  import { onMount, onDestroy, tick } from "svelte";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { invoke, rawInvoke, createChannel, unwrap } from "./lib/invoke";
  // Generated typed command client (CPE-953). First migrated call site — proves the typed surface works
  // end-to-end (routes through the busy-cursor invoke); the broader migration is incremental.
  import { commands } from "./lib/bindings.gen";
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
  import { initAgentSessions, agentSessions, watchTargetFor, watchTargets, currentSessions, normalizePath, clearAgentSessions, ingestSessionState } from "./lib/agentSessions";
  import { startAgentWatch, stopAgentWatch, type FsActivity, type AgentSession } from "./lib/sidecar";
  import { initAgentActivity, fsActivity, recentActivities, agentTimeline, affectsListing, ingestActivity } from "./lib/agentActivity";
  import { initAgentDiffs } from "./lib/agentDiffs";
  import { initAgentCost, ingestCost } from "./lib/agentCost";
  import { clearAgentSessionMetrics, flushSession, flushAllSessions, flushAllSessionsForcibly } from "./lib/agentSessionMetrics";
  import AgentTimeline from "./lib/components/AgentTimeline.svelte";
  import DiskSpaceView from "./lib/components/DiskSpaceView.svelte";
  import DiagnosticsOverlay from "./lib/components/DiagnosticsOverlay.svelte";
  import TestModeOverlay from "./lib/components/TestModeOverlay.svelte";
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
  import TrashView from "./lib/components/TrashView.svelte";
  import DocsView from "./lib/components/DocsView.svelte";
  import { docSlugForSection, type Section } from "./lib/sectionDocs";
  import CommandPalette from "./lib/components/CommandPalette.svelte";
  // Navigation Mode (CPE-1556, epic CPE-1487 — opt-in vim-modal layer over the file list). All four
  // building blocks below landed inert in CPE-1552-1555; this file is the single, Settings-gated
  // integration point that makes them reachable.
  import NavCommandLine from "./lib/components/NavCommandLine.svelte";
  import NavModeIndicator from "./lib/components/NavModeIndicator.svelte";
  import NavCheatsheet from "./lib/components/NavCheatsheet.svelte";
  import { initialNavState, reduceNavKey, type NavState, type NavIntent } from "./lib/navMode";
  import { applyNavIntent, type NavLayout } from "./lib/navMotion";
  import UserCommandsDialog from "./lib/components/UserCommandsDialog.svelte";
  import RunCommandConfirm from "./lib/components/RunCommandConfirm.svelte";
  import { commandsForSurface, resolveCommand, type UserCommand } from "./lib/userCommands";
  import type { Command } from "./lib/commandPalette";
  import MacrosDialog from "./lib/components/MacrosDialog.svelte";
  import MacroParamPrompt from "./lib/components/MacroParamPrompt.svelte";
  import MacroRunConfirm from "./lib/components/MacroRunConfirm.svelte";
  import { bindingsForSurface, matchHotkey, hotkeyFromEvent, type MacroBinding } from "./lib/macroBindings";
  import { extractAskLabels, resolveAskParams } from "./lib/macroParams";
  import type { ActionMacro, MacroSummary } from "./lib/bindings.gen";
  import AgentMenu from "./lib/components/AgentMenu.svelte";
  import NetworkConnectionMenu from "./lib/components/NetworkConnectionMenu.svelte";
  import NetworkConnectionForm from "./lib/components/NetworkConnectionForm.svelte";
  import NetworkSecretPrompt from "./lib/components/NetworkSecretPrompt.svelte";
  import {
    connectionLocation,
    secretAlwaysRequired,
    formFromConnection,
    blankConnectionForm,
    mergeDiscovered,
    type ConnState,
    type ConnectionFormInput,
  } from "./lib/network";
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
  import PasswordPromptDialog from "./lib/components/PasswordPromptDialog.svelte";
  import NewLinkDialog from "./lib/components/NewLinkDialog.svelte";
  import RepairLinkDialog from "./lib/components/RepairLinkDialog.svelte";
  import ShredConfirmDialog from "./lib/components/ShredConfirmDialog.svelte";
  import VaultBanner from "./lib/components/VaultBanner.svelte";
  import VaultCreateDialog from "./lib/components/VaultCreateDialog.svelte";
  import ArchiveSafetyDialog from "./lib/components/ArchiveSafetyDialog.svelte";
  import CreateCertDialog from "./lib/components/CreateCertDialog.svelte";
  import SignCertDialog from "./lib/components/SignCertDialog.svelte";
  import InspectCryptoDialog from "./lib/components/InspectCryptoDialog.svelte";
  import SplitFileDialog from "./lib/components/SplitFileDialog.svelte";
  import JoinPartsDialog from "./lib/components/JoinPartsDialog.svelte";
  import { canSplitFile, canJoinFile } from "./lib/splitJoin";
  import type { SplitManifest } from "./lib/bindings.gen";
  import {
    vaults,
    unlockVault,
    lockVault,
    isUnlocked,
    sessionDirFor,
    vaultOfSessionPath,
    vaultDisplayName,
    classifyUnlockError,
  } from "./lib/vaultStore";
  import ShortcutsDialog from "./lib/components/ShortcutsDialog.svelte";
  import ContentSearchDialog from "./lib/components/ContentSearchDialog.svelte";
  import ContentIndexSearchDialog from "./lib/components/ContentIndexSearchDialog.svelte";
  import CopilotDialog from "./lib/components/CopilotDialog.svelte";
  import FileNameSearchDialog from "./lib/components/FileNameSearchDialog.svelte";
  import InstantSearch from "./lib/components/InstantSearch.svelte";
  import Spotlight from "./lib/components/Spotlight.svelte";
  import type { ResultKind } from "./lib/bindings.gen";
  import TransferPanel from "./lib/components/TransferPanel.svelte";
  import DropStackPanel from "./lib/components/DropStackPanel.svelte";
  import { initDropStack, addToDropStack, dropStackEntries, removeFromDropStack } from "./lib/dropStack";
  import TerminalPanel from "./lib/components/TerminalPanel.svelte";
  import TransferConflictDialog from "./lib/components/TransferConflictDialog.svelte";
  import { initTransfers, startTransfer, startArchiveCompress, startArchiveExtract, collidingNames, type TransferReport, type ConflictPolicy } from "./lib/transfers";
  import DuplicatesDialog from "./lib/components/DuplicatesDialog.svelte";
  import SimilarImagesDialog from "./lib/components/SimilarImagesDialog.svelte";
  import NearDuplicatesDialog from "./lib/components/NearDuplicatesDialog.svelte";
  import FileHealthDialog from "./lib/components/FileHealthDialog.svelte";
  import DeclutterDialog from "./lib/components/DeclutterDialog.svelte";
  import { namesList, detailList, csvList } from "./lib/listing";
  import { parentDir as parentOfPath, baseName } from "./lib/contentSearch";
  import PropertiesDialog from "./lib/components/PropertiesDialog.svelte";
  import MetadataStudioDialog from "./lib/components/MetadataStudioDialog.svelte";
  import BatchRenameDialog from "./lib/components/BatchRenameDialog.svelte";
  import BatchMediaDialog from "./lib/components/BatchMediaDialog.svelte";
  import { partitionEligible, canBatchTransform } from "./lib/batchMedia";
  import type { CheckpointPartial } from "./lib/batchMedia";
  import type { BatchReport } from "./lib/bindings.gen";
  import TagEditor from "./lib/components/TagEditor.svelte";
  import { initTags, tags, retagPath, renameTag, deleteTag, importTags, exportTags } from "./lib/tags";
  import { migratePathList } from "./lib/pathMigrate";
  import { ZIP_FAMILY_EXTS, ARCHIVE_EXTS, EXTRACT_EXTS, ARCHIVE_SAFETY_EXTS } from "./lib/archiveExts";
  import { resolveArchivePreviewEntry, createArchivePreviewResolver } from "./lib/archivePreview";
  import { resolveEffect } from "./lib/dnd";
  import {
    smartFolders,
    smartFolderPaths,
    saveSmartFolder,
    renameSaved as renameSmartSaved,
    removeSaved as removeSmartSaved,
    moveSaved as moveSmartSaved,
    type SmartFolder,
  } from "./lib/smartFolders";
  import { evaluateSavedSearch, flattenTree, resolveSavedSearchRoot, type SavedSearch } from "./lib/savedSearch";
  import {
    watchPathsForScope,
    batchTouchesScope,
    TrailingDebounce,
    type SmartFolderScope,
  } from "./lib/smartFolderLiveRefresh"; // CPE-1230: recompute an open smart folder on disk change
  import {
    savedSearches,
    addSavedSearch,
    renameSavedSearch,
    removeSavedSearch,
    moveSavedSearch,
  } from "./lib/savedSearchStore";
  import TagMenu from "./lib/components/TagMenu.svelte";
  import SmartFolderMenu from "./lib/components/SmartFolderMenu.svelte";
  import { tagCounts } from "./lib/tagFilter";
  import type { RenameItem } from "./lib/batchRename";

  import { t } from "./lib/i18n";
  import { friendlyError, splitPath, formatPathsForClipboard } from "./lib/format";
  import { uniqueName, uniqueNameWithExt } from "./lib/naming";
  import { NEW_FILE_TYPE_BY_EXT, type NewFileType } from "./lib/newFileTypes";
  import { validateFileName } from "./lib/filename";
  import { matchesGlob } from "./lib/glob";
  import PatternSelectDialog from "./lib/components/PatternSelectDialog.svelte";
  import { firstMatchIndex } from "./lib/typeahead";
  import { clampWidth, maxSidePaneWidth, fitSidePanes, PANE_DIVIDER_W } from "./lib/resize";
  import { MID_MIN, NAME_COL_MIN, clampMetaWidths, type ActiveMetaColumn } from "./lib/columns";
  import ColumnPickerDialog from "./lib/components/ColumnPickerDialog.svelte";
  import { metaColumnCatalog } from "./lib/metaColumnCatalog";
  import {
    createHistory, visit, back, forward, canGoBack, canGoForward, current, recentPaths,
    type History,
  } from "./lib/history";
  import { pushClosedTab, keepOnly, keepThroughRight } from "./lib/tabs";
  import TabMenu from "./lib/components/TabMenu.svelte";
  import {
    emptySelection, click as selClick, selectOnly, selectAll, moveLead,
    selectedIndices, selectedCount, remapByPath, invertSelection, selectIndices,
    pickActivePane, snapshotConfirmTarget, type Selection, type ConfirmTarget,
  } from "./lib/selection";
  import { arrowDelta, pageDelta } from "./lib/gridnav";
  import {
    emptyClipboard, stage, isEmpty as clipEmpty, canPaste as clipCanPaste,
    type Clipboard,
  } from "./lib/clipboard";
  import { detectContexts, type FolderAction } from "./lib/folderContext";
  import { isExecutable, iconFor, sameTypeIndices, isImage } from "./lib/filetypes";
  import QuickLook from "./lib/components/QuickLook.svelte";
  import MediaQuickLook from "./lib/components/MediaQuickLook.svelte";
  import { buildMediaPlaylist, mediaQuickLookAction } from "./lib/mediaQuickLook";
  import * as settings from "./lib/settings";
  import { keymapStore } from "./lib/settings";
  // Remappable built-in shortcuts (CPE-1557, epic CPE-1484): handleKeydown resolves the pressed chord
  // against the effective keymap so a user remap takes effect live. `chordFromEvent` is the permissive
  // matcher (bare F5/F2/Delete/… included); `actionForChord` is exact-match, first-wins.
  import { actionForChord, chordFromEvent, type ActionId } from "./lib/keymap";
  import type { ColorRule } from "./lib/colorRules";
  import ColorRulesDialog from "./lib/components/ColorRulesDialog.svelte";
  import SessionHistoryDialog from "./lib/components/SessionHistoryDialog.svelte";
  import CompareDialog from "./lib/components/CompareDialog.svelte";
  import IntegrityDialog from "./lib/components/IntegrityDialog.svelte";
  import TemplatesDialog from "./lib/components/TemplatesDialog.svelte";
  import CheckpointDialog from "./lib/components/CheckpointDialog.svelte";
  import OrganizeDialog from "./lib/components/OrganizeDialog.svelte";
  import type { ChecksumEntry, IntegrityReport } from "./lib/integrity";
  import SelectByDialog from "./lib/components/SelectByDialog.svelte";
  import { selectMatching } from "./lib/selectMatch";
  import type { Condition } from "./lib/colorRules";
  import WatchRulesDialog from "./lib/components/WatchRulesDialog.svelte";
  import type { WatchRule } from "./lib/watchRules";
  import {
    startFolderWatch,
    stopFolderWatch,
    undoFire,
    type WatchFire,
    type FolderWatchEvent,
  } from "./lib/folderWatch";
  import WorkspacesDialog from "./lib/components/WorkspacesDialog.svelte";
  import { pruneMissing, type Workspace, type WorkspaceTab } from "./lib/workspaces";
  import BackupDashboard from "./lib/components/BackupDashboard.svelte";
  import { planBackup, unattendedBackupArgs, type BackupJob } from "./lib/backup";
  import type { CompareNode } from "./lib/treeDiff";
  import { startDriveScheduler, stopDriveScheduler } from "./lib/driveScheduler";
  import { startDriveWatch, stopDriveWatch, pokeDriveWatch } from "./lib/driveWatch";
  import AttributesDialog from "./lib/components/AttributesDialog.svelte";
  import {
    pushUndo, popUndo, canUndo, peekLabel, invert, deletedPaths, type UndoEntry,
  } from "./lib/undo";
  import type { DirEntry, Place, SortKey, SortDir, ViewMode, RecentFile, Favorite, NetShare, Connection, DensityMode } from "./lib/types";

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

  // `--test-mode` automation badge (CPE-1046): the backend injects a synchronous `window.__CPE_TEST_MODE__`
  // global (set before this script runs — same mechanism as `--open`'s `__CPE_OPEN_DIR__`, CPE-1043), so
  // reading it here at init needs no command/gate. In a plain browser / test env the global is simply
  // absent and this is false — zero cost when the app isn't launched in test mode.
  const testMode =
    typeof window !== "undefined" &&
    (window as unknown as { __CPE_TEST_MODE__?: boolean }).__CPE_TEST_MODE__ === true;

  // CPE-1130: a test-mode-only hook that lets the headless gui-smoke suite seed a SYNTHETIC Agent
  // Watch session announcement (the same `session:<json>` wire shape a real sidecar emits over the
  // `ai-console://session` event, decoded by `agentSessions.ts#ingestSessionState`) without a real
  // agent/sidecar running. Opening the Agent Watch drawer (and its cost-History tab, CPE-1114) is
  // gated behind `activeWatchCwd` being non-empty — there's no other way to reach it in a smoke
  // harness that never launches a real agent. Mirrors the existing `__CPE_OPEN_DIR__`/
  // `__CPE_TEST_MODE__` convention: only attached when `testMode` is true, so it's absent (zero
  // cost, zero attack surface) from every normal launch.
  if (testMode) {
    (window as unknown as { __CPE_TEST_INGEST_SESSION__?: (state: string) => void }).__CPE_TEST_INGEST_SESSION__ =
      ingestSessionState;
  }

  // CPE-1135: a test-mode-only hook mirroring the one above — lets the headless gui-smoke suite seed
  // SYNTHETIC filesystem-activity items directly into the live `agentTimeline` store (the same shape
  // an `ai-console://fs-activity` batch decodes to via `agentActivity.ts#ingestActivity` /
  // `sidecar.ts#normalizeFsActivity`) without a real watched folder or agent ever producing them. The
  // Agent Watch drawer's Replay tab (`AgentTimeline.svelte`, CPE-1094) only renders its scrubber
  // transport/slider (`.rp-transport`/`.rp-slider`) once `sliderRange(entries)` is non-null, which
  // needs >=2 timeline entries — there is no other way to reach that render in a harness that never
  // watches real filesystem activity. `at` lets the caller control each batch's timestamp explicitly
  // (default `Date.now()`), so two calls can land distinct, ordered entries instead of racing the
  // same millisecond. Mirrors the `__CPE_TEST_INGEST_SESSION__`/`__CPE_OPEN_DIR__` convention: only
  // attached when `testMode` is true, so it's absent (zero cost, zero attack surface) outside
  // `--test-mode`.
  if (testMode) {
    (window as unknown as { __CPE_TEST_INGEST_ACTIVITY__?: (payload: string, at?: number) => void }).__CPE_TEST_INGEST_ACTIVITY__ =
      (payload: string, at?: number) => ingestActivity(JSON.parse(payload), at);
  }

  // CPE-1173: a test-mode-only hook mirroring the two above — lets the headless gui-smoke suite seed a
  // SYNTHETIC per-session usage snapshot directly into the live `agentCost` store (the same shape an
  // `ai-console://agent-cost` event decodes to via `agentCost.ts#ingestCost`) without a real sidecar's
  // PTY usage scrape ever producing one. The Agent Watch drawer's Cost tab (`AgentTimeline.svelte`,
  // CPE-1098) only renders `.cl-card` rows once the `agentCost` store has an entry — there is no other
  // way to reach that render in a harness that never runs a real agent. Mirrors the
  // `__CPE_TEST_INGEST_SESSION__`/`__CPE_TEST_INGEST_ACTIVITY__` convention: only attached when
  // `testMode` is true, so it's absent (zero cost, zero attack surface) outside `--test-mode`.
  if (testMode) {
    (window as unknown as { __CPE_TEST_INGEST_COST__?: (payload: string) => void }).__CPE_TEST_INGEST_COST__ =
      (payload: string) => ingestCost(JSON.parse(payload));
  }

  let notice = "";
  let noticeIsError = false;
  let noticeTimer: ReturnType<typeof setTimeout> | undefined;

  let selection: Selection = emptySelection();

  // Dual-pane / commander mode (CPE-677, epic CPE-617). Pane B is a second <ExplorerPane> rendered beside
  // pane A when `dualPane` is on, navigating independently via navigateB/openB. Single-pane (default) is
  // unchanged. `activePane` drives the focus ring + Tab switch.
  // Row/chrome density (CPE-1526, foundation slice of epic CPE-1488 "compact/dense view mode"):
  // "comfortable" is today's spacing and the default — this ticket only persists the value and
  // threads it as a prop into the panes/chrome below; CPE-1527/1528/1529 are what actually read it.
  let density = settings.loadDensity();
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
  // CPE-1140: no fixed maximums — the side panes may grow arbitrarily wide. Their effective max is
  // dynamic (sidebarMaxWidth()/rightMaxWidth() below): whatever is left of the window once the OTHER
  // side pane, the grid dividers, and the middle pane's own minimum (MID_MIN, derived from the
  // file-list column mins in lib/columns.ts) are accounted for — the middle's floor always wins.
  const SIDEBAR_MIN = 160;
  const RIGHT_MIN = 220;
  let sidebarWidth = 220;
  let rightWidth = 300;
  let resizing: null | "left" | "right" = null;
  let resizeStartX = 0;
  let resizeStartW = 0;

  $: gridCols = showDetails
    ? `${sidebarWidth}px ${PANE_DIVIDER_W}px minmax(${MID_MIN}px, 1fr) ${PANE_DIVIDER_W}px ${rightWidth}px`
    : `${sidebarWidth}px ${PANE_DIVIDER_W}px minmax(${MID_MIN}px, 1fr)`;

  // Dual-pane (CPE-677): two equal file columns, preview suppressed; reuses the preview grid slot.
  // Each column keeps its own floor at the Name column's minimum (never collapses below it, CPE-1140).
  $: effectiveGridCols = dualPane
    ? `${sidebarWidth}px ${PANE_DIVIDER_W}px minmax(${NAME_COL_MIN}px, 1fr) ${PANE_DIVIDER_W}px minmax(${NAME_COL_MIN}px, 1fr)`
    : gridCols;

  /** The sidebar's current dynamic maximum (CPE-1140): as wide as the window allows without pushing
   *  the middle pane (or, in dual-pane, its two file columns) below its minimum. */
  function sidebarMaxWidth(): number {
    const midMin = dualPane ? 2 * NAME_COL_MIN : MID_MIN;
    const dividerCount = dualPane ? 2 : showDetails ? 2 : 1;
    const otherPanesWidth = dualPane ? 0 : showDetails ? rightWidth : 0;
    return maxSidePaneWidth(window.innerWidth, otherPanesWidth, PANE_DIVIDER_W, dividerCount, midMin, SIDEBAR_MIN);
  }
  /** The right pane's current dynamic maximum (CPE-1140): as wide as the window allows without
   *  pushing the middle file-list pane below MID_MIN. Only meaningful outside dual-pane. */
  function rightMaxWidth(): number {
    return maxSidePaneWidth(window.innerWidth, sidebarWidth, PANE_DIVIDER_W, 2, MID_MIN, RIGHT_MIN);
  }

  /** The keyboard-active pane's own `.rows` element (CPE-1370): `.pane-active .rows` in dual-pane mode
      (pane A only carries `.pane-active` when dualPane is on — see the pane-col markup below — so this
      resolves to pane B's grid when it's focused), plain `.rows` in single-pane, which is always pane
      A's — so single-pane behaviour is untouched. Shared base query for `currentGridCols()` and
      `visibleRowCount()` so both agree on which pane's DOM they're reading. */
  function activeRowsEl(): HTMLElement | null {
    return document.querySelector<HTMLElement>(dualPane ? ".pane-active .rows" : ".rows");
  }

  /** Live column count of the file grid, for 2-D arrow-key nav (CPE-769) and Page-key paging
      (CPE-1374). 1 for list/details; for the icons/gallery grid, read the resolved
      `grid-template-columns` off the active pane's live `.rows.grid` (the same source of truth
      FileList windows against), so it tracks pane width / view without extra plumbing. */
  function currentGridCols(): number {
    if (view !== "icons" && view !== "gallery") return 1;
    const el = activeRowsEl();
    if (!el) return 1;
    const tracks = getComputedStyle(el).gridTemplateColumns.split(" ").filter((s) => s && s !== "none").length;
    return Math.max(1, tracks);
  }

  /** Rows visible in the active pane's viewport, for PageUp/PageDown (CPE-1374): the row list's
      scrollable ancestor (`.filelist-pane`) height divided by one row/tile's measured height — grid-
      aware since it reads the same `.rows` element `currentGridCols()` does. Falls back to a
      conservative default when nothing is measurable yet (empty folder, first paint, a headless test
      harness with no real layout) so a Page key always moves the lead by *something* rather than 0. */
  function visibleRowCount(): number {
    const rowsEl = activeRowsEl();
    const scroller = rowsEl?.closest<HTMLElement>(".filelist-pane") ?? null;
    const firstRow = rowsEl?.querySelector<HTMLElement>(".row");
    const viewportH = scroller?.getBoundingClientRect().height ?? 0;
    const rowH = firstRow?.getBoundingClientRect().height ?? 0;
    if (viewportH <= 0 || rowH <= 0) return 10;
    return Math.max(1, Math.floor(viewportH / rowH));
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
      sidebarWidth = clampWidth(resizeStartW + dx, SIDEBAR_MIN, sidebarMaxWidth());
    } else if (resizing === "right") {
      // The right pane grows as the divider moves left, so subtract dx.
      rightWidth = clampWidth(resizeStartW - dx, RIGHT_MIN, rightMaxWidth());
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
  // Home "Shared" tab (CPE-1163): user-added network locations (persisted) + the combined share list
  // the backend returns (enumerated mapped drives/mounts merged with the user's), loaded on demand.
  let networkLocations: string[] = [];
  let shared: NetShare[] = [];
  let sharedLoading = false;
  // Network sidebar section (CPE-1513, epic CPE-1498): saved SFTP/WebDAV connection profiles + their
  // live, client-tracked connect state (see `lib/network.ts` module docs — there is no backend
  // session-status query yet, so this resets to "disconnected" every app restart).
  let connections: Connection[] = [];
  let connectionStates: Record<string, ConnState> = {};
  let connectionErrors: Record<string, string> = {};
  // Windows-native "Discovered on your network" tier (CPE-1519): WNet-enumerated shares, loaded once at
  // startup alongside `shared` (see `loadDiscovered`'s doc comment below for why fire-and-forget).
  let discoveredShares: NetShare[] = [];
  let networkForm: { x: number; y: number; editing: Connection | null; prefill: ConnectionFormInput | null } | null =
    null;
  let networkContextMenu: { x: number; y: number; conn: Connection } | null = null;
  let networkSecretPrompt: { x: number; y: number; conn: Connection } | null = null;
  let columnWidths: number[] = settings.loadColumnWidths();
  /** Active metadata columns for the CURRENT folder (CPE-1146, epic CPE-707): id + width, in display
      order. Loaded/saved per-folder in `loadPath` below; empty (Home, or a folder with none saved) is
      the default — the pane behaves exactly as before CPE-1146. */
  let activeMetaColumns: ActiveMetaColumn[] = [];
  /** Pane B's own active metadata columns (CPE-1382 follow-up to CPE-1378), keyed by `paneBPath` — mirrors
      `activeMetaColumns` above but derived from pane B's folder, not pane A's. Loaded/saved per-folder in
      `navigateB` below. Without this, pane B displayed pane A's active columns regardless of its own
      folder's saved config. */
  let activeMetaColumnsB: ActiveMetaColumn[] = [];
  let columnPickerOpen = false;
  /** CPE-1388: which pane's `on:openColumnPicker` opened the dialog — captured at open time (pane B's
   *  binding passes `true`) so the dialog loads FROM and saves TO the originating pane's own column set
   *  (`activeMetaColumnsB`/`paneBPath` vs `activeMetaColumns`/`currentPath`), mirroring CPE-1382's
   *  per-pane READ fix on the WRITE side too. Meaningless while `columnPickerOpen` is false. */
  let columnPickerInPaneB = false;
  /** Active rule-based coloring rule set (CPE-776, epic CPE-709); empty ⇒ rows unstyled. */
  let colorRules: ColorRule[] = settings.loadColorRules();
  let colorRulesOpen = false;
  let sessionHistoryOpen = false;
  let compareOpen = false;
  let compareLeft = "";
  let compareRight = "";
  let integrityOpen = false;
  let templatesOpen = false;
  let checkpointOpen = false;
  let organizeOpen = false;
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
      // Generated return is Partial<{[k]: IntegrityReport}>; every key we read was just sent in
      // `integrityBaselines`, so narrow the Partial back to a dense Record for the callers below.
      const reports = unwrap(await commands.verifyAllBaselines(integrityBaselines)) as Record<string, IntegrityReport>;
      const flagged = Object.values(reports).filter((r) => r.corrupted.length > 0 || r.missing.length > 0);
      if (flagged.length === 0) {
        showNotice($t(paths.length === 1 ? "notice.baselineCleanOne" : "notice.baselineCleanMany", { count: paths.length }), false);
      } else {
        const bad = flagged.reduce((n, r) => n + r.corrupted.length + r.missing.length, 0);
        showNotice($t(bad === 1 ? "notice.baselineIssuesOne" : "notice.baselineIssuesMany", { flagged: flagged.length, total: paths.length, bad }), true);
      }
    } catch (e) {
      showNotice($t("notice.verifyAllFailed", { error: String(e) }), true);
    }
  }
  let selectByOpen = false;
  /** Open the "Select by…" dialog straight into its "Save search…" name field (CPE-1229's own
      command-palette entry), vs. the default criterion picker for "Select by…" itself. */
  let selectByAutoSave = false;
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
      // CPE-1664: every argument — including the `confirmed` consent flag, which is the per-job auto-run
      // opt-in the user ticked — is built by `unattendedBackupArgs`, which `backup.test.ts` pins with
      // BOTH a ticked and an unticked job. It cannot be pinned from here: the scheduler only ever hands
      // this function `autoRun: true` jobs, so no test reaching it can distinguish the real value from a
      // constant. See that function for exactly what is and is not covered.
      await rawInvoke("apply_backup_plan_stream", { ...unattendedBackupArgs(job, p), onResult: channel });
      const failed = results.filter((r) => !r.ok).length;
      recordBackupRun(job.id, { when: Date.now(), ok: results.length - failed, failed, label: "auto" });
      showNotice(failed ? $t("notice.autoBackupDoneWithFailures", { name: job.name, copied: results.length - failed, failed }) : $t("notice.autoBackupDone", { name: job.name, copied: results.length - failed }));
    } catch (e) {
      showNotice($t("notice.autoBackupFailed", { name: job.name, error: String(e) }), true);
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
  /** Pane B's own Tags filter (CPE-1376) — kept separate from pane A's `selectedTag`, per-pane like the
   *  CPE-1370 `selectionB`/`visibleB` split, so filtering by tag in one pane doesn't silently also
   *  filter the other. Routed to by the (single, shared) Sidebar's `filterTag` event via `activePane`. */
  let selectedTagB = "";
  /** Right-click menu for a sidebar tag (rename/delete), or null (CPE-653). */
  let tagMenu: { x: number; y: number; tag: string } | null = null;
  /** Right-click menu for a sidebar smart folder (rename/delete), or null (CPE-667). */
  let smartFolderMenu: { x: number; y: number; id: string; name: string } | null = null;
  /** Right-click menu for a sidebar saved (structured) search (rename/delete), or null (CPE-1229). */
  let structuredSearchMenu: { x: number; y: number; id: string; name: string } | null = null;
  /** Full-screen quick-look of images (Space), or null (CPE-645). */
  let quickLook: { images: { path: string; name: string }[]; index: number } | null = null;

  /** Open quick-look on the selected image, seeding the folder's images. Returns false if not applicable.
   *  `inPaneB` (CPE-1432): Space is a global keyboard shortcut, so `handleKeydown` passes the live active
   *  pane (the same `dualPane && activePane === 1` flag it already computes for the other pane-aware keys)
   *  through `paneStateFor`, mirroring `askVaultCreate`/`askCertCreate`. `archive`/`isHome` are pane-A-only
   *  concepts (pane B is always a plain real folder), so pane B's "no folder" guard is `paneBPath === HOME`
   *  instead — default `inPaneB = false` keeps single-pane and pane-A-active behavior byte-for-byte
   *  unchanged. */
  function openQuickLook(inPaneB = false): boolean {
    const pane = paneStateFor(inPaneB);
    if ((inPaneB ? paneBPath === HOME : (isHome || archive)) || pane.selectedEntries.length !== 1) return false;
    const sel = pane.selectedEntries[0];
    if (sel.is_dir || !isImage(sel.name)) return false;
    const images = pane.visible.filter((e) => !e.is_dir && isImage(e.name)).map((e) => ({ path: e.path, name: e.name }));
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

  /** Full-screen quick-look media player (Space over an audio/video file), or null (CPE-1430, epic
   *  CPE-720). Navigation is driven by the pure `Playlist` mirror (repeat off/one/all + shuffle); the
   *  `render` counter forces a re-read of the mutable playlist's current track/position after every step
   *  or mode change (the class mutates in place, so a bump is what re-renders the overlay). */
  let mediaQuickLook: {
    playlist: ReturnType<typeof buildMediaPlaylist>;
    render: number;
  } | null = null;

  /** Open the media quick-look on the selected media file, seeding the folder's media playlist. Returns
   *  false if not applicable (not a single media selection), so Space can fall through to its other uses.
   *  `inPaneB` (CPE-1432): same active-pane routing as `openQuickLook` above — pane B's selection + its
   *  own listing feed `buildMediaPlaylist` when pane B is active, so the folder-stepping (◄/►) also stays
   *  within pane B's folder. */
  function openMediaQuickLook(inPaneB = false): boolean {
    const pane = paneStateFor(inPaneB);
    if ((inPaneB ? paneBPath === HOME : (isHome || archive)) || pane.selectedEntries.length !== 1) return false;
    const playlist = buildMediaPlaylist(pane.visible, pane.selectedEntries[0].path);
    if (!playlist) return false; // selection isn't a media file
    mediaQuickLook = { playlist, render: 0 };
    return true;
  }
  /** Step to the previous/next folder media item, honouring the playlist's repeat + shuffle. */
  function mediaQuickLookStep(delta: number) {
    if (!mediaQuickLook) return;
    if (delta > 0) mediaQuickLook.playlist!.next();
    else mediaQuickLook.playlist!.prev();
    mediaQuickLook = { ...mediaQuickLook, render: mediaQuickLook.render + 1 };
  }
  function mediaQuickLookRepeat() {
    if (!mediaQuickLook) return;
    mediaQuickLook.playlist!.cycleRepeat();
    mediaQuickLook = { ...mediaQuickLook, render: mediaQuickLook.render + 1 };
  }
  function mediaQuickLookShuffle() {
    if (!mediaQuickLook) return;
    const pl = mediaQuickLook.playlist!;
    pl.setShuffle(!pl.isShuffled, Date.now());
    mediaQuickLook = { ...mediaQuickLook, render: mediaQuickLook.render + 1 };
  }
  let editingPath = false;

  let renamingPath = "";
  let renameValue = "";
  /** Pane B's own inline-rename state (CPE-1377): pane B is a second `<ExplorerPane>` instance, so it
   *  needs its own `renamingPath`/`renameValue` pair — reusing pane A's would let a pane-A rename-in-
   *  progress leak an editor into pane B (or vice versa). `beginRename`/`commitRename` route to this
   *  pair via their `inPaneB` parameter, exactly like `selectionB`/`selectedEntriesB` already do. */
  let renamingPathB = "";
  let renameValueB = "";
  /** Path of a freshly-created folder, so we can auto-rename it once listed. */
  let pendingRenamePath = "";
  let pendingSelectPath = ""; // select (no rename) a just-created item after reload

  let undoStack: UndoEntry[] = [];
  /** Whether THIS platform can restore from the trash (false on macOS). */
  let canRestoreTrash = false;
  /** Paths currently being dragged, shared with the sidebar as a drop target. */
  let draggedPaths: string[] = [];
  /** `inPaneB` (CPE-1377): which pane the menu was opened OVER, set by `onRowContext`/`onDriveContext`/
   *  `onHomeItemContext`/pane B's `contextEmpty` handler. Deliberately NOT derived from live `activePane`
   *  — a right-click doesn't focus a pane (only a plain click does, per the `pane-col`'s `on:click`), so
   *  right-clicking pane B while pane A is still "active" must still target pane B. `runAction`/the
   *  `<ContextMenu>` props below read it via `ctx?.inPaneB` (menu-open-time) rather than `activePane`
   *  (focus-time) — the same reasoning `snapshotConfirmTarget` already applies to a delete confirm. */
  let ctx: { x: number; y: number; target: "item" | "empty" | "drive" | "home-item"; inPaneB?: boolean } | null = null;
  /** True when the single item the currently-open `target: "item"` menu is for is a broken symlink
   *  (CPE-1209, epic CPE-715) — resolved async in `onRowContext` via `commands.linkStatus` (the menu
   *  itself can't do async), gating the "Repair link…" row. Reset to false whenever a new item menu
   *  opens so a stale broken flag never survives onto a different row. */
  let ctxLinkBroken = false;
  /** Folder a `NewLinkDialog` creates its link in (CPE-1207, epic CPE-715), or null when closed. Scope
   *  is deliberately narrow (unlike `createNewItem`'s per-folder targeting): only the empty-area context
   *  menu and command palette open it, both always acting on `currentPath`. */
  let newLinkDialogFor: string | null = null;
  /** The broken symlink a `RepairLinkDialog` is open for (CPE-1209, epic CPE-715), or null when closed. */
  let repairLinkFor: DirEntry | null = null;
  /** Target paths + display label for an open `ShredConfirmDialog` (CPE-1240, epic CPE-738), or null
   *  when closed. Set by `askShred`, cleared on close/done. `inPaneB`/`dir` (CPE-1386): SNAPSHOT at
   *  invocation time — before the dialog opens — mirroring `snapshotConfirmTarget`/`ConfirmTarget`
   *  (CPE-1370): `paths` is already frozen the same way (the dialog's `paths` prop is bound to this
   *  object, which is never mutated while open), so the destructive shred itself was already safe; `dir`
   *  lets `onShredDone` refresh the pane the shred actually ran in even if the active pane changed while
   *  the confirm dialog was open. */
  let shredConfirmFor: { paths: string[]; what: string; inPaneB: boolean; dir: string } | null = null;
  /** The folder an open `VaultCreateDialog` is sealing (CPE-1250, epic CPE-738), or null when closed.
   *  Set by the "vault-create" action, cleared on close/created. `inPaneB`/`dir` (CPE-1386): SNAPSHOT at
   *  invocation time, same reasoning as `shredConfirmFor` above — the dialog's `folderPath` prop is bound
   *  to this object (never mutated while open), so the create call (including its optional destructive
   *  shred-original path) was already safe; `dir` lets `onVaultCreated` refresh the right pane. */
  let vaultCreateFor: { folderPath: string; folderName: string; inPaneB: boolean; dir: string } | null = null;
  /** The archive path an open `ArchiveSafetyDialog` is scanning (CPE-1318, epic CPE-1002), or null when
   *  closed. Set by the "archive-safety" context-menu action, cleared on close. */
  let archiveSafetyFor: string | null = null;
  /** Prefill + pane/dir context for an open `CreateCertDialog` (CPE-1423/1424, epic CPE-1417), or null
   *  when closed. `dir` is the pane's currently-displayed folder — SNAPSHOT at open time (mirrors
   *  `vaultCreateFor`) so a pane that navigates away while the dialog is open doesn't retarget the
   *  refresh. `outDir` is the dialog's own default output folder: the clicked folder (a folder-row
   *  "Create certificate here…") or `dir` itself (empty space / the command palette). */
  let certCreateFor: { dir: string; outDir: string; inPaneB: boolean } | null = null;
  /** Prefill + pane/dir context for an open `SignCertDialog` (CPE-1423/1424), or null when closed.
   *  `csrPath`/`caCertPath` prefill from the clicked file ("Issue cert from this CSR…" / "Sign with this
   *  as CA…"); both "" when opened from the command palette. `dir` mirrors `certCreateFor.dir` — the
   *  refresh target once a certificate is issued. */
  let certSignFor: { dir: string; inPaneB: boolean; csrPath: string; caCertPath: string } | null = null;
  /** Path + viewer kind for an open `InspectCryptoDialog` (CPE-1438, epic CPE-1417), or null when
   *  closed. Only used in DUAL-PANE mode, where the inline preview slot is occupied by pane B so the
   *  "Inspect" / "Inspect JWT" action can't fall through to the preview pane the way single-pane does —
   *  the overlay reuses JwtPreview/CertPreview to decode the file anyway. `path` is snapshot from the
   *  clicked pane's selection; `kind` picks the viewer. */
  let cryptoInspectFor: { path: string; kind: "jwt" | "cert" } | null = null;
  /** Path + pane/dir context for an open `SplitFileDialog` (CPE-1509, parent CPE-1491), or null when
   *  closed. `dir` is SNAPSHOT at open time (mirrors `certCreateFor`) so `onSplitDone`'s refresh targets
   *  the pane that was showing the file when the dialog opened, not wherever it's navigated to since. */
  let splitFileFor: { path: string; dir: string; inPaneB: boolean } | null = null;
  /** Path + pane/dir context for an open `JoinPartsDialog` (CPE-1509), or null when closed. Same
   *  snapshot reasoning as `splitFileFor`. */
  let joinPartsFor: { path: string; dir: string; inPaneB: boolean } | null = null;
  // The drive root + display name for an open "drive" context menu (CPE-1158). All drive-menu actions
  // target this path, so the menu works identically from a Home tile and a sidebar row — and from Home,
  // where there is no FileList selection to piggy-back on.
  let driveCtxPath = "";
  let driveCtxName = "";
  // The clicked Home row's path + type + source-view for an open "home-item" menu (CPE-1162). Stored
  // independently of the FileList selection (Home has none), exactly like `driveCtxPath` — every
  // home-* action in runAction targets THIS path. `homeCtxStale` is set by a best-effort async
  // existence check when the menu opens: true ⇒ the on-disk rows are disabled but "Remove from <view>"
  // stays live so a dead pointer can still be pruned.
  let homeCtxPath = "";
  let homeCtxName = "";
  let homeCtxIsDir = false;
  let homeCtxView: "recent" | "favorites" | "folders" | "shared" = "recent";
  // For a Shared row (CPE-1163): its kind ("mapped" | "mount" | "user"), so the menu offers the right
  // action — Disconnect for a mapped drive, Remove for a user-added location.
  let homeCtxKind = "";
  let homeCtxStale = false;
  let confirm: { title: string; message: string; label: string; onYes: () => void } | null = null;
  /** Password prompt state (CPE-1182), mirroring `confirm` above: set to show `PasswordPromptDialog`,
   *  cleared to null to dismiss (Cancel/Escape) or on a successful `onSubmit`. `onSubmit` re-sets this
   *  itself (with `error` filled in) to re-prompt after a wrong password instead of closing. */
  let passwordPrompt: {
    title: string;
    message: string;
    confirmLabel: string;
    error: string;
    onSubmit: (password: string) => void | Promise<void>;
  } | null = null;
  /** Compress/extract ops queued through the transfer engine (CPE-1184), keyed by transfer id, so the
   *  global `transfer://done` listener knows what to do once the queued run actually finishes — the
   *  same `onSuccess` shape `extractWithPasswordFallback` used to run inline before archive ops became
   *  async/queued. `cancelledNotice`/`failedNotice` are the fallback messages when the report itself
   *  doesn't have a more specific error to show. Entries are consumed (deleted) once handled.
   *  `dir` (CPE-1386): the folder the op actually landed in/pulled from — a compress/extract queued from
   *  a pane-B context menu can target pane B's own folder now, so the listener can no longer hard-code a
   *  pane-A refresh. `onSuccess` still owns the op-specific notice + (pane-A-only) `pendingSelectPath`;
   *  the listener does the actual refresh via `refreshBatchApplyTarget(dir)` — reused as-is rather than
   *  adding a separate `inPaneB` flag, since it already matches `dir` against BOTH panes' live folders
   *  (CPE-1371/1387's both-can-match reasoning) and simply no-ops for a pane renavigated elsewhere while
   *  the op was in flight, exactly like a batch-rename/batch-media apply. */
  const pendingArchiveOps = new Map<
    number,
    { onSuccess: () => void | Promise<void>; cancelledNotice: string; failedNotice: string; dir: string }
  >();
  let propsFor: DirEntry[] | null = null;
  let studioFor: DirEntry[] | null = null;
  /** CPE-1384: which pane + folder a batch-rename dialog targets, SNAPSHOT at open time (mirroring
   *  `ConfirmTarget`/`snapshotConfirmTarget`, CPE-1370) — the dialog stays open while the user edits
   *  names, and the active pane must not be able to change underneath it and have `applyBatchRename`
   *  silently rename the OTHER pane's files. Null when closed. */
  let batchRenameFor: { entries: DirEntry[]; inPaneB: boolean; dir: string } | null = null;
  /** Eligible (image-only) entries for the Batch-Media dialog (CPE-1093), or null when closed. `inPaneB`
   *  (CPE-1384) is snapshot at open time the same way, so the post-apply refresh always targets the pane
   *  the dialog was actually opened for, even if the active pane changes while it's open. `dir` (CPE-1387)
   *  is snapshot too, mirroring `batchRenameFor`, so the refresh reloads the folder actually operated on
   *  even if that pane gets renavigated elsewhere while the dialog is still open. */
  let batchMediaFor: { entries: DirEntry[]; inPaneB: boolean; dir: string } | null = null;
  /** The entry whose tags/label are being edited (CPE-637), or null when the editor is closed. */
  let tagEditorFor: DirEntry[] | null = null;

  // ---- Application menu (CPE-229) ----
  const REPO_URL = "https://github.com/StewartScottRogers/cross-platform-explorer";
  let showAbout = false;
  let showSettings = false;

  // User-defined commands (CPE-783): the persisted list, the manager dialog, and the confirm-before-launch
  // state. Running a command always goes through RunCommandConfirm — nothing spawns without an explicit OK.
  let userCommands: UserCommand[] = [];
  let showUserCommands = false;
  let runConfirm: { title: string; commands: string[]; cwd: string } | null = null;
  function persistUserCommands(list: UserCommand[]) {
    userCommands = list;
    settings.saveUserCommands(list);
  }
  function openRunCommand(cmd: UserCommand) {
    runConfirm = { title: cmd.name, commands: resolveCommand(cmd, selectedEntries), cwd: isHome ? "" : currentPath };
  }
  // Context/Toolbar surfaces (CPE-1577): the id+name pairs each surface needs to render its rows, kept
  // in list order like the Palette surface already does via `commandsForSurface`.
  $: userCommandsContext = commandsForSurface(userCommands, "context").map((c) => ({ id: c.id, name: c.name }));
  $: userCommandsToolbar = commandsForSurface(userCommands, "toolbar").map((c) => ({ id: c.id, name: c.name }));
  /** Run a user command by id — the single dispatch target for the Context menu's "Run command ▸"
   *  submenu AND the Toolbar's per-command buttons (both route the `uc:<id>` action here via
   *  `runAction`). An id that no longer resolves (a command removed between render and click) is a
   *  silent no-op, matching the macro-hotkey precedent elsewhere in this file. */
  function runUserCommandById(id: string) {
    const cmd = userCommands.find((c) => c.id === id);
    if (cmd) openRunCommand(cmd);
  }

  // Scriptable macros (CPE-1189/1190/1191, epic CPE-739): the library dialog, the persisted
  // surface/hotkey bindings, and the run flow — {ask:label} prompt (if any) -> dry-run confirm
  // (macro_plan) -> execute (macro_run) -> offer Undo (macro_undo). `macroSummaries` mirrors
  // `userCommands`'s in-memory list but is refreshed from the CPE-1188 backend catalog (the source of
  // truth is `macro_save`/`macro_delete`/`macro_import`, not local state) rather than persisted here.
  let macrosOpen = false;
  let macroSummaries: MacroSummary[] = [];
  async function refreshMacroSummaries() {
    try {
      macroSummaries = unwrap(await commands.macroList());
    } catch {
      macroSummaries = [];
    }
  }
  let macroBindings: MacroBinding[] = [];
  function persistMacroBindings(list: MacroBinding[]) {
    macroBindings = list;
    settings.saveMacroBindings(list);
  }
  /** Bound macro names still present in the catalog, per surface — a binding for a since-deleted
   *  macro is silently skipped rather than showing a dead menu row. */
  $: macroContextNames = bindingsForSurface(macroBindings, "context")
    .map((b) => b.name)
    .filter((name) => macroSummaries.some((m) => m.name === name));
  $: macroPaletteBindings = bindingsForSurface(macroBindings, "palette").filter((b) =>
    macroSummaries.some((m) => m.name === b.name),
  );

  let macroParamPromptFor: { macro: ActionMacro; labels: string[] } | null = null;
  let macroRunConfirmFor: { macro: ActionMacro; inputs: string[]; root: string } | null = null;

  function beginMacroRun(macro: ActionMacro) {
    macroRunConfirmFor = {
      macro,
      inputs: selectedEntries.map((e) => e.path),
      root: isHome || archive ? "" : currentPath,
    };
  }

  /** `MacroParamPrompt`'s `submit` handler: resolve the {ask:label} tokens against the answered
   *  values, then hand off to the dry-run confirm. A plain function (not inlined in the template) so
   *  it can null-check `macroParamPromptFor` once — an inline arrow in the template loses Svelte's
   *  `{#if}` narrowing on read. */
  function submitMacroParams(values: Record<string, string>) {
    if (!macroParamPromptFor) return;
    const resolved = resolveAskParams(macroParamPromptFor.macro, values);
    macroParamPromptFor = null;
    beginMacroRun(resolved);
  }

  /** Load a saved macro by name and start its run flow: prompt for `{ask:label}` params first (if
   *  any), otherwise go straight to the dry-run confirm. The only entry point for running a macro —
   *  from the context menu, the palette, or a bound hotkey. */
  async function startMacro(name: string) {
    try {
      const macro = unwrap(await commands.macroLoad(name));
      if (!macro) {
        showNotice($t("notice.macroGone", { name }), true);
        await refreshMacroSummaries();
        return;
      }
      const labels = extractAskLabels(macro);
      if (labels.length > 0) macroParamPromptFor = { macro, labels };
      else beginMacroRun(macro);
    } catch (e) {
      showNotice($t("notice.macroLoadFailed", { error: e instanceof Error ? e.message : String(e) }), true);
    }
  }

  let shortcutsOpen = false;
  /** "Search in files" content-search overlay (Ctrl+Shift+F), scoped to the current folder (CPE-417). */
  let contentSearchOpen = false;
  /** "Find files by name" recursive name-search overlay (Ctrl+P), scoped to the current folder (CPE-603). */
  let fileSearchOpen = false;
  /** Instant Search overlay (Ctrl+K) — keyboard-first cross-volume search over the resident index
   *  (CPE-1139, epic CPE-703). Global: works from any folder, or the Home screen. */
  let instantSearchOpen = false;
  /** File-content search overlay (palette only — no shortcut free) — ranked hits from the local content
   *  index built by `content_index_build` (CPE-1263, epic CPE-976). Scoped to the current folder. */
  let contentIndexSearchOpen = false;
  /** AI file copilot overlay (palette only) — instruction → whitelisted plan preview → explicit Confirm
   *  → execute → undo, scoped to the current folder (CPE-1276, epic CPE-977). */
  let copilotOpen = false;
  /** Query the toolbar Search hands to the recursive find dialog on Enter (CPE-866). */
  let deepSearchQuery = "";
  /** "Find duplicate files" overlay, scoped to the current folder (CPE-421). */
  let duplicatesOpen = false;
  /** "Find similar images" overlay — near-duplicate image review + safe cleanup (CPE-1202). */
  let similarImagesOpen = false;
  /** "Find similar documents" / "Find near-identical folders" overlay — read-only near-dup review over
   *  the SimHash text / Jaccard folder cores (CPE-1204, epic CPE-997 stretch). One shared dialog; `kind`
   *  picks the engine. */
  let similarDocsOpen = false;
  let similarFoldersOpen = false;
  /** File Health panel overlay (CPE-1315/CPE-1316/CPE-1317, epic CPE-1002) — a tabbed dialog surfacing
   *  the file-inspection detectors; slice 1 wired the streaming dangling/cyclic-links tab, slice 2 adds
   *  type-mismatch + orphan-sidecar tabs, slice 3 adds the non-streaming empty-folders tab. */
  let fileHealthOpen = false;
  /** Which File Health tab to land on when it opens (CPE-1316) — set right before flipping
   *  `fileHealthOpen`, so each Tools-menu / palette entry that targets one detector opens straight to it. */
  let fileHealthTab: "dangling" | "mismatch" | "orphan" | "empty" = "dangling";
  /** Bumped every time a File-Health entry is invoked (CPE-1317) — lets `FileHealthDialog` jump to the
   *  requested tab even when the panel is ALREADY OPEN (a one-time `activeTab = initialTab` initializer
   *  can't see a later `fileHealthTab` change, since `{#if fileHealthOpen}` never remounts once open),
   *  and even when the SAME entry is invoked again while the user has since clicked to a different tab
   *  manually (a plain `$: activeTab = initialTab` can't tell that apart from "no change"). See
   *  `openFileHealth` below, the single call site that bumps it. */
  let fileHealthNonce = 0;
  /** Declutter overlay — surfaces `organize_clutter`'s rules-based junk findings (empty files,
   *  installers, temp/partial downloads, backups) for safe review + move-to-bin (CPE-1329, epic
   *  CPE-979). Read-only until the user selects + confirms; the AI classifier is a separate, gated
   *  concern this dialog does not touch. */
  let declutterOpen = false;

  /** Open the File Health panel scoped to `tab` (CPE-1316/CPE-1317) — the single call site every
   *  Tools-menu / command-palette File-Health entry uses, so the nonce bump can never be forgotten at a
   *  new call site. */
  function openFileHealth(tab: "dangling" | "mismatch" | "orphan" | "empty") {
    fileHealthTab = tab;
    fileHealthNonce++;
    fileHealthOpen = true;
  }
  let patternSelectOpen = false;
  /** Repositories browser overlay (CPE-434/435) — browse GitHub & other forges in-app. */
  let showRepos = false;
  /** Agent Board (CPE-521) — Kanban over the current folder's Ticketing/. */
  let showBoard = false;
  /** Integrated workbench (CPE-526) — git diff of the current folder. */
  let showWorkbench = false;
  /** Browsable Trash overlay (CPE-1560, epic CPE-1486 final slice) — opened from the Sidebar's Trash
   *  section; only reachable when `canRestoreTrash` gates it on (Windows/Linux). */
  let showTrash = false;
  /** Embedded terminal dock (CPE-1243, epic CPE-714) — an xterm.js pane rooted at the current folder.
   *  Mounted only while true, so a never-opened terminal costs nothing (no PTY, no dock tab, no xterm). */
  let showTerminal = false;
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
    if (compareOpen) return "compare";
    if (showWorkbench) return "workbench";
    if (showTerminal) return "terminal";
    if (showTrash) return "trash";
    // CPE-1604: the Agent Watch strip (ExplorerPane) shows whenever `activeWatchCwd` is non-empty — F1/
    // the toolbar "?" should jump straight to its own page while it's on screen, ahead of the plain
    // explorer fallback below.
    if (activeWatchCwd) return "agent-watch";
    return isHome ? "home" : "explorer";
  }
  /** Every documented section + a friendly label, for per-section jump-links (palette, menus) — CPE-764. */
  const DOC_SECTIONS: { section: Section; label: string }[] = [
    { section: "home", label: "Overview" },
    { section: "explorer", label: "Explorer" },
    { section: "disk-usage", label: "Disk usage" },
    { section: "workbench", label: "Workbench" },
    { section: "terminal", label: "Terminal" },
    { section: "agent-board", label: "Agent Board" },
    { section: "ai-console", label: "Agent Deck" },
    { section: "agent-grid", label: "Agent Grid" },
    { section: "repositories", label: "Repositories" },
    { section: "swarms", label: "Swarms" },
    { section: "trash", label: "Trash" },
    { section: "agent-watch", label: "Agent Watch" },
  ];

  // Command Palette (CPE-602): Ctrl+Shift+P. The command list reuses existing handlers — nothing is
  // duplicated; `enabled` closures read live state so context-invalid commands grey out.
  let paletteOpen = false;
  // Navigation Mode (CPE-1556, epic CPE-1487): an opt-in vim-modal layer over the file list. `enabled`
  // defaults to FALSE (loadNavigationModeEnabled) so a fresh install behaves exactly as before; it's
  // re-read whenever the Settings dialog closes (see the SettingsDialog mount). `navState` holds the
  // current mode + pending chord/count; it's reset on every tab/pane switch (reactive block below) so a
  // half-typed `g`/count or a lingering visual mode never leaks across panes.
  let navigationModeEnabled = settings.loadNavigationModeEnabled();
  let navState: NavState = initialNavState();
  let navCommandLineOpen = false;
  let navCheatsheetOpen = false;
  let navContextKey = "";
  $: {
    const key = `${activeId}:${activePane}`;
    if (key !== navContextKey) {
      navContextKey = key;
      navState = initialNavState();
    }
  }
  // Spotlight overlay (CPE-1216, epic CPE-704): a global quick-launch overlay sectioned across
  // actions/folders/files/recents. Opened by the backend `spotlight:open` Tauri event (CPE-1215's OS
  // hotkey, listened for below) AND the in-app "Spotlight (search everywhere)…" palette command, so
  // it's reachable — and gui-smoke-testable — without the OS-level shortcut.
  let spotlightOpen = false;
  const inFolder = () => !isHome && !archive && !smartFolder && !structuredSearch;
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
    { id: "file.newLink", group: $t("palette.groupFile"), label: $t("palette.newLink"), keywords: "symlink hardlink shortcut", run: () => (newLinkDialogFor = currentPath), enabled: inFolder },
    { id: "file.copy", group: $t("palette.groupFile"), label: $t("palette.copy"), shortcut: "Ctrl+C", run: doCopy, enabled: hasSelection },
    { id: "file.cut", group: $t("palette.groupFile"), label: $t("palette.cut"), shortcut: "Ctrl+X", run: doCut, enabled: hasSelection },
    { id: "file.addDropStack", group: $t("palette.groupFile"), label: "Add to Drop Stack", keywords: "shelf stack collect gather move copy later", shortcut: "Ctrl+Shift+D", run: () => doAddToDropStack(), enabled: hasSelection },
    { id: "file.paste", group: $t("palette.groupFile"), label: $t("palette.paste"), shortcut: "Ctrl+V", run: doPaste, enabled: inFolder },
    { id: "file.copyPath", group: $t("palette.groupFile"), label: $t("palette.copyPath"), shortcut: "Ctrl+Shift+C", run: doCopyPath, enabled: hasSelection },
    { id: "file.copyName", group: $t("palette.groupFile"), label: $t("palette.copyName"), run: doCopyName, enabled: hasSelection },
    { id: "file.rename", group: $t("palette.groupFile"), label: $t("palette.rename"), shortcut: "F2", run: renameSelected, enabled: oneSelected },
    { id: "file.duplicate", group: $t("palette.groupFile"), label: $t("palette.duplicate"), shortcut: "Ctrl+D", run: doDuplicate, enabled: hasSelection },
    { id: "file.delete", group: $t("palette.groupFile"), label: $t("palette.delete"), keywords: "recycle bin trash remove", shortcut: "Delete", run: () => askDelete(false), enabled: hasSelection },
    { id: "file.deletePermanent", group: $t("palette.groupFile"), label: $t("palette.deletePermanent"), keywords: "remove", shortcut: "Shift+Delete", run: () => askDelete(true), enabled: hasSelection },
    { id: "file.selectAll", group: $t("palette.groupFile"), label: $t("palette.selectAll"), shortcut: "Ctrl+A", run: selectAllVisible, enabled: inFolder },
    { id: "file.properties", group: $t("palette.groupFile"), label: $t("palette.properties"), shortcut: "Alt+Enter", run: openProperties, enabled: hasSelection },
    { id: "file.metadataStudio", group: $t("palette.groupFile"), label: $t("studio.menu"), run: openMetadataStudio, enabled: hasSelection },
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
    { id: "tool.instantSearch", group: $t("palette.groupTools"), label: $t("palette.instantSearch"), shortcut: "Ctrl+K", run: () => (instantSearchOpen = true) },
    { id: "tool.contentIndexSearch", group: $t("palette.groupTools"), label: $t("palette.contentIndexSearch"), keywords: "content semantic meaning embedding embedder offline index snippet ranked", run: () => (contentIndexSearchOpen = true), enabled: inFolder },
    { id: "tool.copilot", group: $t("palette.groupTools"), label: $t("palette.copilot"), keywords: "ai copilot organize instruction plan llm assistant natural language move rename", run: () => (copilotOpen = true), enabled: inFolder },
    { id: "tool.spotlight", group: $t("palette.groupTools"), label: $t("palette.spotlight"), keywords: "quick launch omnibox everywhere actions folders files recent", run: () => (spotlightOpen = true) },
    { id: "tool.findDuplicates", group: $t("palette.groupTools"), label: $t("palette.findDuplicates"), run: () => (duplicatesOpen = true), enabled: inFolder },
    { id: "tool.findSimilarImages", group: $t("palette.groupTools"), label: $t("palette.findSimilarImages"), keywords: "near duplicate similar images photos perceptual dhash reclaim", run: () => (similarImagesOpen = true), enabled: inFolder },
    { id: "tool.findSimilarDocuments", group: $t("palette.groupTools"), label: $t("palette.findSimilarDocuments"), keywords: "near duplicate similar documents text notes readme simhash", run: () => (similarDocsOpen = true), enabled: inFolder },
    { id: "tool.findSimilarFolders", group: $t("palette.groupTools"), label: $t("palette.findSimilarFolders"), keywords: "near identical similar folders jaccard", run: () => (similarFoldersOpen = true), enabled: inFolder },
    { id: "tool.findDanglingLinks", group: $t("palette.groupTools"), label: $t("palette.findDanglingLinks"), keywords: "dangling broken cyclic symlink link file health", run: () => openFileHealth("dangling"), enabled: inFolder },
    { id: "tool.findTypeMismatches", group: $t("palette.groupTools"), label: $t("palette.findTypeMismatches"), keywords: "type mismatch extension disguised renamed wrong file health", run: () => openFileHealth("mismatch"), enabled: inFolder },
    { id: "tool.findOrphanSidecars", group: $t("palette.groupTools"), label: $t("palette.findOrphanSidecars"), keywords: "orphan sidecar srt xmp companion file health", run: () => openFileHealth("orphan"), enabled: inFolder },
    { id: "tool.findEmptyDirs", group: $t("palette.groupTools"), label: $t("palette.findEmptyDirs"), keywords: "empty folder cascade cleanup file health", run: () => openFileHealth("empty"), enabled: inFolder },
    { id: "tool.findClutter", group: $t("palette.groupTools"), label: $t("palette.findClutter"), keywords: "declutter junk clutter empty installer temp partial backup clean up review bin", run: () => (declutterOpen = true), enabled: inFolder },
    { id: "tool.colorRules", group: $t("palette.groupTools"), label: $t("palette.colorRules"), keywords: "color rules highlight label", run: () => (colorRulesOpen = true) },
    { id: "tool.sessionHistory", group: $t("palette.groupTools"), label: $t("palette.sessionHistory"), keywords: "audit log history export sessions activity", run: () => (sessionHistoryOpen = true) },
    { id: "tool.compareFolders", group: $t("palette.groupTools"), label: $t("palette.compareFolders"), keywords: "diff compare folders directories tree", run: openCompare },
    { id: "tool.integrity", group: $t("palette.groupTools"), label: $t("palette.integrity"), keywords: "integrity checksum bitrot corruption verify baseline", run: () => (integrityOpen = true) },
    // Certificate management (CPE-1423/1424, epic CPE-1417): the same two dialogs the pane-aware
    // context menu opens, reachable here with no file context needed — both target pane A's own folder.
    { id: "tool.certCreate", group: $t("palette.groupTools"), label: "Create certificate…", keywords: "cert certificate tls ssl x509 self-signed ca create keypair", run: () => askCertCreate(), enabled: inFolder },
    { id: "tool.certSign", group: $t("palette.groupTools"), label: "Sign / issue certificate…", keywords: "cert certificate csr sign issue ca x509", run: () => askCertSign(), enabled: inFolder },
    { id: "tool.templates", group: $t("palette.groupTools"), label: $t("palette.templates"), keywords: "folder templates scaffold capture stamp new from template boilerplate", run: () => (templatesOpen = true) },
    { id: "tool.checkpoint", group: $t("palette.groupTools"), label: $t("palette.checkpoint"), keywords: "checkpoint rollback revert restore snapshot undo agent watch", run: () => (checkpointOpen = true), enabled: inFolder },
    { id: "tool.organize", group: $t("palette.groupTools"), label: $t("palette.organize"), keywords: "organize auto organize sort files by kind extension year size declutter clean up", run: () => (organizeOpen = true), enabled: inFolder },
    { id: "tool.columns", group: $t("palette.groupTools"), label: $t("palette.manageColumns"), keywords: "columns metadata dimensions duration pages track year picker details view add remove reorder", run: () => { columnPickerInPaneB = false; columnPickerOpen = true; }, enabled: inFolder },
    { id: "tool.verifyAll", group: $t("palette.groupTools"), label: $t("palette.verifyAll"), keywords: "integrity verify all baselined folders bitrot corruption monitor check", run: verifyAllBaselines, enabled: () => Object.keys(integrityBaselines).length > 0 },
    { id: "tool.selectBy", group: $t("palette.groupTools"), label: $t("palette.selectBy"), keywords: "select by criteria extension size date filter", run: () => (selectByOpen = true), enabled: inFolder },
    // CPE-1229 (epic CPE-978): opens the SAME dialog straight into "Save search…" — capture the current
    // structured search as a named SavedSearch instead of applying it to the selection.
    { id: "tool.saveSearch", group: $t("palette.groupTools"), label: $t("palette.saveSearch"), keywords: "save search smart folder condition filter saved query", run: () => { selectByOpen = true; selectByAutoSave = true; }, enabled: inFolder },
    { id: "tool.watchRules", group: $t("palette.groupTools"), label: $t("palette.watchRules"), keywords: "watch rules folder automation move copy tag rename", run: () => (watchRulesOpen = true) },
    { id: "tool.workspaces", group: $t("palette.groupGo"), label: $t("palette.workspaces"), keywords: "workspace layout tabs save session restore", run: () => (workspacesOpen = true) },
    { id: "tool.backup", group: $t("palette.groupTools"), label: $t("palette.backup"), keywords: "backup jobs copy mirror restore sync", run: () => (backupOpen = true) },
    { id: "tool.attributes", group: $t("palette.groupTools"), label: $t("palette.attributes"), keywords: "attributes permissions readonly hidden mode chmod", run: openAttributes },
    { id: "tool.aiConsole", group: $t("palette.groupTools"), label: $t("palette.openAiConsole"), run: () => openAiConsole(), enabled: () => aiConsoleAvailable },
    { id: "tool.agentBoardWindow", group: $t("palette.groupTools"), label: $t("palette.openAgentBoardWindow"), keywords: "agent board kanban tickets window pop out", run: () => openAgentBoard() },
    { id: "app.settings", group: $t("palette.groupApp"), label: $t("palette.settings"), run: () => (showSettings = true) },
    { id: "app.userCommands", group: $t("palette.groupApp"), label: "Manage user commands…", keywords: "custom command run external", run: () => (showUserCommands = true) },
    { id: "app.macros", group: $t("palette.groupApp"), label: "Manage macros…", keywords: "macro library rename move tag convert steps scriptable", run: () => (macrosOpen = true) },
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
    // Palette-surfaced user commands (CPE-783): run each via the confirm-before-launch dialog.
    ...commandsForSurface(userCommands, "palette").map((c) => ({
      id: `uc:${c.id}`, group: "Commands", label: c.name, keywords: c.template, run: () => openRunCommand(c),
    })),
    // Palette-surfaced saved macros (CPE-1191): run each via the {ask} prompt (if any) -> dry-run
    // confirm -> execute flow, same gate as the context-menu "Run macro ▸" submenu.
    ...macroPaletteBindings.map((b) => ({
      id: `macro:${b.name}`, group: "Macros", label: `Run macro: ${b.name}`, keywords: "macro run rename move tag convert",
      run: () => startMacro(b.name),
    })),
  ] satisfies Command[];

  /** Open a URL in a dedicated browser webview window (CPE-527) — safe under the strict CSP since it's
      a separate webview, not an iframe in the main window. The URL is validated http/https in-view. */
  function openBrowserWindow(url: string) {
    try {
      new WebviewWindow(`workbench-browser-${Date.now()}`, { url, title: url, width: 1000, height: 720 });
    } catch {
      showNotice($t("notice.browserWindowFailed"), true);
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
      const s = (await commands.forgeRepoStatus(path, loadSyncPolicy(path))) as unknown as typeof gitStatus;
      gitStatus = s && (s as { is_repo?: boolean }).is_repo ? s : null;
    } catch {
      gitStatus = null; // plain build (command absent) or git unavailable
    }
  }
  $: refreshGitStatus(currentPath);

  /** Run a safe sync step (Pull = ff-only, Push = no-force) via the host, then re-list (CPE-462). */
  async function doSync(action: "pull" | "push") {
    try {
      unwrap(await commands.forgeSync(currentPath, action));
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
      const plan = (await commands.forgeRepoStatus(path, loadSyncPolicy(path))) as unknown as typeof gitStatus;
      if (!plan || !(plan as { is_repo?: boolean }).is_repo) return;
      const actions = autoSyncActions(plan as Parameters<typeof autoSyncActions>[0]);
      if (actions.length === 0) {
        const reason = pausedReason(plan as Parameters<typeof pausedReason>[0]);
        if (reason) showNotice($t("notice.autoSyncPaused", { reason }), false);
        return; // nothing safe to do (or diverged) — don't hammer; wait the interval out
      }
      for (const action of actions) {
        unwrap(await commands.forgeSync(path, action));
      }
      lastAutoSync.set(path, Date.now());
      if (currentPath === path) { await refreshGitStatus(path); refresh(); }
      showNotice($t("notice.autoSynced", { time: new Date().toLocaleTimeString() }), false);
    } catch (e) {
      // A failed background sync must never nag repeatedly: back off by marking it "done" for this
      // interval, and surface once.
      lastAutoSync.set(path, Date.now());
      showNotice($t("notice.autoSyncFailed", { error: e instanceof Error ? e.message : String(e) }), true);
    } finally {
      autoSyncRunning = false;
    }
  }
  /** Right-click "close the Agent Deck" menu (CPE-457), shown from an Agents leaf or the AI
      Console button. `label` differs per source; confirming stops the console + clears the leaves. */
  let agentMenu: { x: number; y: number; label: string; sessionId?: string; sessionLabel?: string } | null = null;

  /** Ask before closing the Agent Deck entirely (CPE-1621): now that `closeAllConsoles` genuinely
   *  terminates every running agent (see its own doc below — it used to only hide them), a single
   *  accidental click on "Close all consoles" can silently kill in-progress work across every session
   *  at once. The in-console "Close all" button already warns "Any running agents will be terminated"
   *  before acting (`sidecar/ai-console/src/launcher.html`); this makes the main-window entry point say
   *  the same thing rather than acting instantly. Per-session close (`closeOneConsole`) stays
   *  unconfirmed and unchanged — a single, deliberately-targeted kill is the same risk it always was,
   *  and this ticket's scope is explicit that that path must not change. (Not gated by a backend
   *  `confirmed` boolean the way `shred_paths`/`vault_create` are — see the CPE-1621 work log: this
   *  destructive action is reachable ONLY via a deliberate right-click, never silently or at launch, so
   *  a UI-level confirm here is judged sufficient, and adding a backend gate just for this path while
   *  leaving the identical-risk per-session path ungated would be an inconsistent asymmetry.) */
  function confirmCloseAllConsoles() {
    confirm = {
      title: "Close all consoles?",
      message: "Every running agent will be terminated. Any work an agent hasn't saved elsewhere will be lost.",
      label: "Close all",
      onYes: closeAllConsoles,
    };
  }
  /** Close the Agent Deck entirely (all running agents) and clear the Agents leaves — but ONLY when the
   *  close genuinely happened (CPE-1621 F1 review fix). CPE-1621's original cut ran the leaf-clear
   *  unconditionally, so the UI could claim a kill it never performed: `sidecarCloseAllSessions()`
   *  no-ops (`Ok(CloseAllOutcome::Nothing)`) whenever `state.url` is `None` — after `sidecar_repair`, or
   *  whenever the console sidecar crashed/exited without an explicit stop — and the Agents leaves are a
   *  client-side store that persists independently of the live connection (the whole point of the
   *  CPE-461 reattach design), so leaves showing daemon-backed sessions with no live URL is a normal
   *  state, not a corner case. Treat that no-op as success ONLY if there were no leaves to lose in the
   *  first place; otherwise — same as an outright POST failure/timeout — leave the leaves in place and
   *  tell the user, rather than silently wiping evidence of agents that are, for all we know, still
   *  running.
   *  First reach `/api/close-all` on the console's OWN loopback UI server — the same endpoint the
   *  in-console "Close all" button already uses — so every session's PTY is actually killed, including
   *  one held by the separate, host-owned session daemon (`state.daemon` in `src-tauri/src/lib.rs`),
   *  which `sidecarStop` below never touches by design (it survives a UI sidecar restart on purpose).
   *  This MUST run before `sidecarStop`: that call drops the host's connection/URL to the console
   *  (CPE-464), and once it's gone there is nothing left to reach — ordering it after would silently
   *  no-op and leave every agent running, which was exactly this ticket's bug (F2: order pinned by
   *  `App.closeAllConsoles.test.ts`). `sidecarStop` still runs regardless of the close outcome —
   *  it only tears down the local console UI process, a separate concern from whether the sessions
   *  inside it were reached.
   *  CPE-1626: on the genuine-success path, flush every session's metrics row FIRST, forcibly for
   *  anything still running (no `ended` will ever come now the process is gone) — otherwise
   *  `clearAgentSessions()` below empties `$agentSessions`, and the reactive full-stop reconcile wipes
   *  the whole accumulator store with a still-running session's activity never persisted anywhere. A
   *  forced row is honestly marked `endedCleanly: false` rather than fabricated as a clean end (see
   *  `flushAllSessionsForcibly`). */
  async function closeAllConsoles() {
    agentMenu = null;
    confirm = null;
    let closedGenuinely: boolean;
    try {
      const outcome = unwrap(await commands.sidecarCloseAllSessions());
      // "nothing" only counts as success if there was truly nothing to close — otherwise it means we
      // couldn't REACH the sessions the leaves still show, not that they don't exist (F1).
      closedGenuinely = outcome === "closed" || currentSessions().length === 0;
    } catch (e) {
      console.debug("close all sessions failed:", e);
      closedGenuinely = false;
    }
    try {
      unwrap(await commands.sidecarStop("ai-console"));
    } catch (e) {
      console.debug("close consoles failed:", e);
    }
    if (!closedGenuinely) {
      showNotice($t("notice.closeAllConsolesUnreachable"), true);
      return;
    }
    await flushAllSessionsForcibly();
    clearAgentSessions();
  }
  /** Close a single agent session (CPE-489) — routes to the Agent Deck's per-session close endpoint
      via the host. The console emits an `ended` for it, which prunes the leaf; the others keep running. */
  async function closeOneConsole(sessionId: string) {
    agentMenu = null;
    try {
      unwrap(await commands.sidecarCloseSession(sessionId));
    } catch (e) {
      console.debug("close session failed:", e);
    }
  }
  /** True in sidecar-platform builds — gates the Agent Deck toolbar button (CPE-351). */
  let aiConsoleAvailable = false;
  /** Keep the out-of-process app buttons ALPHABETICAL within their toolbar section (CPE-933): a CSS
   *  `order` per app derived from its (localised) label, so the section stays sorted regardless of markup
   *  order. To add an app: add its label here and set `style="order: {appOrder.<key>}"` on its button. */
  $: appOrder = (() => {
    const labels: Record<string, string> = {
      board: "Agent Board",
      console: $t("tb.aiConsole"),
      repos: $t("sidebar.repositories"),
    };
    const order: Record<string, number> = {};
    Object.keys(labels)
      .sort((a, b) => labels[a].localeCompare(labels[b]))
      .forEach((k, i) => (order[k] = i));
    return order;
  })();
  /** Teardown for the Agent Watch session listener (CPE-396). */
  let unlistenSessions: (() => void) | null = null;
  let unlistenTransferDone: (() => void) | null = null;
  let unlistenOpenDocs: (() => void) | null = null;
  let unlistenSpotlightOpen: (() => void) | null = null;
  /** Teardown for the system-tray quick-access jump listener (CPE-1272). */
  let unlistenTrayOpen: (() => void) | null = null;
  // OS file drop-in (CPE-670): overlay shown while OS files are dragged over the window.
  let osDragActive = false;
  let unlistenOsDrop: (() => void) | null = null;
  /** A copy-paste awaiting the user's conflict choice (CPE-624). `inPaneB` (CPE-1380) is the destination
   *  pane the paste targeted — carried through to `resolveCopyConflict` so the eventual copy still lands
   *  wherever the paste was actually invoked, even though the dialog itself has no pane concept. */
  let pendingCopy: { sources: string[]; count: number; inPaneB: boolean } | null = null;
  /** CPE-1380 (extended by CPE-1384): transfer ids for an in-flight COPY that targeted pane B — set by
   *  `startCopyWithPolicy(sources, policy, true)` (clipboard paste) and `copyMoveToFolder(false, true)`
   *  ("Copy to…"), consumed by the shared `transfer://done` listener below so it refreshes pane B (not
   *  pane A) once that specific transfer finishes. Every other copy source (drag-drop, Home copy, quick
   *  actions, a pane-A "Copy to…") never adds an id here and keeps refreshing pane A exactly as before. */
  const pasteCopyPaneB = new Set<number>();
  /** CPE-1533 (epic CPE-1489 finale): a Drop-Stack "Copy all here" awaiting the CPE-624 conflict choice
   *  — a Drop-Stack-flavoured twin of `pendingCopy` above, kept separate because `resolveCopyConflict`
   *  is wired to clipboard-paste's pane routing (`inPaneB`), which the (single, pane-less) Drop Stack
   *  panel has no equivalent of. */
  let pendingDropStackCopy: { sources: string[]; count: number } | null = null;
  /** CPE-1533: transfer ids for an in-flight Drop-Stack "Copy all here", mapped to the exact paths that
   *  transfer captured — consumed by the shared `transfer://done` listener below so it can clear just
   *  those paths off the Drop Stack once the transfer finishes cleanly. `TransferReport` only carries
   *  aggregate counts (no per-path result, see transfers.ts), so a partial failure leaves the whole
   *  captured batch shelved rather than guessing which paths actually landed. */
  const dropStackTransferOps = new Map<number, string[]>();
  // Agent Watch view (CPE-399): the Project folder currently being watched (or ""), and the
  // teardown for its activity listener. Watching turns on only while the explorer is inside a
  // running agent's project, and off the moment it leaves — off means off (AGENT-WATCH.md).
  let activeWatchCwd = "";
  let unlistenActivity: (() => void) | null = null;
  /** Teardown for the before/after diff listener (CPE-744); paired with the activity listener. */
  let unlistenDiffs: (() => void) | null = null;
  /** Teardown for the live cost/usage listener (CPE-1098); paired with the activity listener. */
  let unlistenCost: (() => void) | null = null;
  /** Whether the Agent Watch activity timeline drawer is open (CPE-400). */
  let showTimeline = false;
  /** The read-only reconstructed listing to show in the main file pane instead of the live listing
   *  while Replay mode is on (CPE-1112, epic CPE-728 slice e) — `null` (default, "off") means the pane
   *  shows the live listing exactly as before. Set entirely by `AgentTimeline`'s `replayOverlay` event
   *  (a pure derivation, see `lib/replayOverlay.ts`); App never computes or mutates it itself, just
   *  forwards it straight into `ExplorerPane`. */
  let replayOverlayEntries: DirEntry[] | null = null;

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
          const results = await commands.deleteToTrash([item.path]);
          reportResults(results, "moveToBin");
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

  /** Sessions currently armed (being watched): sessionId → the cwd we started the watch on. Lets the
   *  reconcile diff the desired running-session set against what's already watching and touch only the
   *  delta. Off means off: when this empties, the shared listeners below are all torn down (CPE-1099). */
  const armedWatches = new Map<string, string>();
  /** Serialise overlapping reconcile runs (a burst of session announcements) so the armed-set diff
   *  never races its own awaits; a run that arrives mid-flight re-runs once against the latest set. */
  let reconcileInFlight = false;
  let reconcilePending = false;

  /** Reconcile the live filesystem watches against the CURRENT watch target (CPE-1606, revised by
   *  CPE-1626): start a watch for each newly-armed (or re-homed) session, stop the ones no longer
   *  desired, and keep exactly ONE shared fs-activity + fs-diff (+ cost) listener pair alive while
   *  anything is armed — never one per session. A session the explorer has never navigated into is never
   *  in `desired`, so it's never armed at all; a session the explorer HAS since navigated away from is
   *  disarmed too (a pause, not an end — see `flushSession`'s doc, CPE-1626) — "off means off" holds
   *  literally in both cases. When the armed set returns to empty, every listener is removed. */
  async function reconcileAgentWatch(sessions: AgentSession[], current: string) {
    if (reconcileInFlight) { reconcilePending = true; return; }
    reconcileInFlight = true;
    try {
      const desired = new Map<string, string>();
      for (const s of watchTargets(sessions, current)) if (s.cwd) desired.set(s.sessionId, s.cwd);

      // Start the delta: a new session, or one whose cwd changed (re-arming drops the old watch).
      for (const [id, cwd] of desired) {
        if (armedWatches.get(id) !== cwd) {
          await startAgentWatch(id, cwd);
          armedWatches.set(id, cwd);
        }
      }
      // Stop the removed: a session no longer desired — either it genuinely ended, or it's still
      // running and was merely disarmed because the explorer navigated away from its folder (a pause).
      // Call `flushSession` at this seam either way (CPE-1113/CPE-1626) as a REDUNDANT backstop — the
      // primary flush trigger for a real end is now `agentSessions.ts`'s `ingestSessionState`, which
      // fires the instant the `ended` announcement lands, independent of arm state (fixes the latency gap
      // where an ended-while-paused session with an armed sibling used to sit unflushed until some later
      // reconcile drained the armed set). `flushSession` tells a pause from a real end itself via the
      // accumulator's `endedAt`, so calling it here too is always a safe no-op either way.
      for (const id of [...armedWatches.keys()]) {
        if (!desired.has(id)) {
          await flushSession(id);
          await stopAgentWatch(id);
          armedWatches.delete(id);
        }
      }
      // Exactly one shared listener pair, gated on there being anything armed — independent of N.
      if (armedWatches.size > 0) {
        if (!unlistenActivity) unlistenActivity = await initAgentActivity(onAgentBatch);
        if (!unlistenDiffs) unlistenDiffs = await initAgentDiffs();
        if (!unlistenCost) unlistenCost = await initAgentCost();
      } else {
        // Nothing currently armed. Tear down the shared listeners regardless — "off means off" for the
        // OS-level watcher applies whether every session ended OR every session is merely paused
        // (unarmed while still running; no backend watch means no events would arrive anyway). Only
        // reset the METRICS STORE when no session is running at all (CPE-1626): a paused-but-still-alive
        // session's accumulator must survive so a later re-visit resumes it instead of starting a fresh,
        // incomplete row. `flushAllSessions` is a no-op for any session still running (see
        // `flushSession`'s `endedAt` gate) — belt-and-suspenders for genuinely-ended ones the loop above
        // already flushed.
        await flushAllSessions();
        unlistenActivity?.(); unlistenActivity = null;
        unlistenDiffs?.(); unlistenDiffs = null;
        unlistenCost?.(); unlistenCost = null;
        if (sessions.length === 0) clearAgentSessionMetrics(); // CPE-1107: true full-stop only
        if (watchRefreshTimer) { clearTimeout(watchRefreshTimer); watchRefreshTimer = null; }
      }
    } finally {
      reconcileInFlight = false;
      if (reconcilePending) { reconcilePending = false; reconcileAgentWatch(currentSessions(), currentPath); }
    }
  }

  // Watch only the session(s) at the CURRENT deepest project match (CPE-1606/CPE-1625/CPE-1626); re-run
  // on any session change or navigation. Navigating away disarms a still-running session (a pause, not
  // an end) rather than retaining it for the rest of its life — see `reconcileAgentWatch`'s doc for why
  // retention is no longer needed now that a pause can never lose data.
  $: reconcileAgentWatch($agentSessions, currentPath);
  // The on-screen drawer describes just the project the explorer is inside (CPE-399); leaving the
  // watched project closes the timeline (CPE-400) — and, since CPE-1626, stops the underlying watcher.
  $: activeWatchCwd = watchTargetFor($agentSessions, currentPath);
  $: if (!activeWatchCwd) showTimeline = false;
  $: watchedAgentName =
    $agentSessions.find((s) => normalizePath(s.cwd) === normalizePath(activeWatchCwd))?.agentName || "agent";
  /** sessionId of the currently-watched agent, if any — lets the cost ledger (CPE-1098) point at the
   *  session whose Project folder is on screen when several sessions report usage. */
  $: watchedSessionId =
    $agentSessions.find((s) => normalizePath(s.cwd) === normalizePath(activeWatchCwd))?.sessionId || "";
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
          const s = unwrap(await commands.diskSpace(d.path));
          driveUsage = { ...driveUsage, [d.path]: s };
        } catch {
          /* skip a drive we can't stat (e.g. an empty card reader) */
        }
      }),
    );
  }

  /** Which drives are REMOVABLE, so only those rows show an eject button (CPE-1278). Non-blocking; a
   *  failed probe just leaves the drive non-ejectable (the safe default). */
  let driveRemovable: Record<string, boolean> = {};
  async function loadDriveRemovable(list: Place[]) {
    await Promise.all(
      list.map(async (d) => {
        try {
          const removable = await commands.driveEjectable(d.path);
          driveRemovable = { ...driveRemovable, [d.path]: removable };
        } catch {
          /* treat an unknowable drive as non-removable — never offer eject we can't vouch for */
        }
      }),
    );
  }

  /** Apply a freshly-enumerated drive list to the sidebar: reassign `drives`, drop usage/eject state for
   *  drives that vanished, and (re)probe the current set. Shared by the eject refresh (CPE-1278) and the
   *  live drive watcher (CPE-1280) — keeping the surviving drives' bars avoids a flicker on every change. */
  function applyDriveList(d: Place[]) {
    drives = d;
    const present = new Set(d.map((x) => x.path));
    driveUsage = Object.fromEntries(Object.entries(driveUsage).filter(([p]) => present.has(p)));
    driveRemovable = Object.fromEntries(Object.entries(driveRemovable).filter(([p]) => present.has(p)));
    loadDriveUsage(d); // fire-and-forget: (re)fill the sidebar usage bars (CPE-406)
    loadDriveRemovable(d); // fire-and-forget: which drives get an eject button (CPE-1278)
  }

  /** Safely eject a removable drive (CPE-1278). The backend refuses anything non-removable, so this can
   *  only ever act on a USB/removable volume. Toasts the outcome and refreshes the drive list so an
   *  ejected drive drops out of the sidebar. */
  async function ejectDrive(path: string, name: string) {
    try {
      unwrap(await commands.ejectDrive(path));
      showNotice($t("notice.safeToRemove", { name }));
    } catch (e) {
      showNotice(typeof e === "string" ? e : $t("notice.ejectFailed", { name }), true);
    }
    // Refresh drives + removable flags regardless: on success the drive is gone; on failure state is fresh.
    try {
      applyDriveList(await commands.listDrives());
    } catch {
      /* a refresh failure is cosmetic — the toast already told the user the real outcome */
    }
  }
  $: updateDiskSpace(currentPath, isHome, !!archive);
  async function updateDiskSpace(path: string, home: boolean, inArchive: boolean) {
    if (home || inArchive || !path) { diskFree = null; diskTotal = null; return; }
    try {
      const d = unwrap(await commands.diskSpace(path));
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
      if (ctx.session) showNotice($t("tb.consoleAlreadyOpenSession"), false);
      else if (ctx.cwd) showNotice($t("tb.consoleAlreadyOpenCwd"), false);
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
    if (!base) { showNotice($t("tb.consoleStartFailed"), true); return; }
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
      win.once("tauri://error", () => showNotice($t("tb.consoleWindowFailed"), true));
    } catch {
      showNotice($t("tb.consoleWindowFailed"), true);
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
          win.once("tauri://error", () => showNotice($t("tb.boardWindowFailed"), true));
          return;
        } catch {
          showNotice($t("tb.boardWindowFailed"), true);
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
      win.once("tauri://error", () => showNotice($t("tb.boardWindowFailed"), true));
    } catch {
      showNotice($t("tb.boardWindowFailed"), true);
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
  // ZIP_FAMILY_EXTS / ARCHIVE_EXTS / EXTRACT_EXTS live in ./lib/archiveExts (imported above) so their
  // membership is a single source of truth + unit-tested (CPE-1181): iso is browsable but NOT
  // extractable, and EXTRACT_EXTS must not silently inherit browse-only formats.
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
      smartEntries = await commands.entriesForPaths(paths);
    } catch {
      smartEntries = [];
    }
  }
  function openSmartFolder(sf: SmartFolder) {
    smartFolder = sf;
    structuredSearch = null; // the two virtual-folder kinds are mutually exclusive (CPE-1229)
    archive = null;
    selectedTag = "";
    search = "";
    selection = emptySelection();
  }
  function exitSmartFolder() {
    smartFolder = null;
    selection = emptySelection();
  }

  // Active saved STRUCTURED search (CPE-1229, epic CPE-978): a `Condition[]` query opened as a virtual,
  // read-only listing — the same shape as the tag-only smart folder above, but driven by
  // `evaluateSavedSearch` instead of the tag store. There's no whole-computer index yet (CPE-703's index
  // engine is a separate, unbuilt epic), so "cutting across the tree" means recursively from the ONE
  // folder the search was captured under (`search.root`, captured by "Save search…" at save time) rather
  // than truly everywhere the way a tag can appear anywhere. A search saved before `root` existed (or
  // whose captured folder no longer resolves) falls back to the currently-open folder at open time.
  let structuredSearch: SavedSearch | null = null;
  let structuredSearchEntries: DirEntry[] = [];
  $: void loadStructuredSearchEntries(structuredSearch);
  async function loadStructuredSearchEntries(s: SavedSearch | null) {
    if (!s) { structuredSearchEntries = []; return; }
    const root = resolveSavedSearchRoot(s, currentPath);
    if (!root) { structuredSearchEntries = []; return; }
    try {
      const tree = await commands.scanTree(root, 12).then(unwrap);
      structuredSearchEntries = evaluateSavedSearch(flattenTree(tree, root), s, Date.now());
    } catch {
      structuredSearchEntries = [];
    }
  }
  function openStructuredSearch(s: SavedSearch) {
    structuredSearch = s;
    smartFolder = null;
    archive = null;
    selectedTag = "";
    search = "";
    selection = emptySelection();
  }
  function exitStructuredSearch() {
    structuredSearch = null;
    selection = emptySelection();
  }

  const isArchiveFile = (e: DirEntry) => !e.is_dir && ARCHIVE_EXTS.has(e.extension);

  const isExtractable = (e: DirEntry) => !e.is_dir && EXTRACT_EXTS.has(e.extension);

  /** True for a ZIP-family archive the `analyze_archive_safety` scan can actually score (CPE-1318) — see
   *  `ARCHIVE_SAFETY_EXTS`'s doc comment for why this is narrower than {@link isArchiveFile}. */
  const isArchiveSafetyEligible = (e: DirEntry) => !e.is_dir && ARCHIVE_SAFETY_EXTS.has(e.extension);

  /** Cert-family extensions the pane-aware context menu offers "Sign with this as CA…" for (CPE-1424,
   *  epic CPE-1417) — deliberately narrower than CertPreview's own auto-decode set (no `.pub`/`.key`,
   *  which aren't cert-shaped enough to sign with as a CA). */
  const CERT_SIGN_EXTS = new Set(["pem", "crt", "cer", "der"]);

  /** Kind of a clicked cert/CSR-family file for the context menu's cert-management rows (CPE-1424):
   *  `"csr"` offers "Issue cert from this CSR…", `"cert"` offers "Sign with this as CA…" instead, `""`
   *  hides both (and the shared "Inspect" row). */
  const certKindOf = (e: DirEntry): "csr" | "cert" | "" => {
    if (e.is_dir) return "";
    if (e.extension === "csr") return "csr";
    if (CERT_SIGN_EXTS.has(e.extension)) return "cert";
    return "";
  };

  /** True for a `.jwt`/`.jws` file — matches the preview pane's own JWT auto-decode eligibility, so
   *  "Inspect JWT" (CPE-1424) only ever appears where the preview can actually show something. */
  const isJwtFile = (e: DirEntry) => !e.is_dir && (e.extension === "jwt" || e.extension === "jws");

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
        is_symlink: false,
      });
    }
    return [...seen.values()];
  }

  async function enterArchive(entry: DirEntry) {
    try {
      const entries = unwrap(await commands.readArchiveEntries(entry.path));
      archive = { zipPath: entry.path, zipName: entry.name, entries, inner: "" };
      selection = emptySelection();
      search = "";
    } catch (e) {
      // AES-encrypted zips can't be LISTED without the password either (the `zip` crate needs it just
      // to construct the per-entry reader) — there's no password-aware entry lister, only a
      // password-aware full extract (CPE-1182). So "entering" one prompts for its password and extracts
      // it to a sibling folder instead, the closest equivalent to opening it that the backend supports.
      if (isPasswordError(e)) {
        const { dest, name } = extractHereDest(entry);
        promptForExtractPassword(entry, dest, currentPath, () => {
          pendingSelectPath = dest;
          showNotice($t("notice.archivePwProtectedExtracted", { archiveName: entry.name, destName: name }));
        });
        return;
      }
      showNotice($t("notice.archiveOpenFailed", { name: entry.name }), true);
    }
  }

  function exitArchive() {
    archive = null;
    selection = emptySelection();
  }

  // Extract-then-preview inside an archive (CPE-1360). A selected inner entry carries a VIRTUAL path
  // (see `archiveChildren`) that doesn't exist on disk, so the preview pane's loaders can't read it.
  // When exactly one non-directory inner entry is selected we extract it to a temp file and feed the
  // preview pane a DirEntry whose `.path` is that temp path (name/extension preserved so the provider is
  // unchanged). The resolver's request-id guard supersedes a stale extraction if the selection changes
  // mid-flight; a directory selection (or no archive) resolves to null so nothing is previewed.
  let archivePreviewEntry: DirEntry | null = null;
  const archivePreviewResolver = createArchivePreviewResolver((e) => { archivePreviewEntry = e; });
  $: archivePreviewResolver.update(archive, selectedEntries);

  /** Guard file-mutating actions: the in-archive view, a smart folder, and Replay mode's reconstructed
   *  overlay are all read-only. CPE-1112 rework (independent review + UAT both flagged this): the file
   *  pane's rows while `replayOverlayEntries` is set come from the PAST, reconstructed from the audit
   *  journal — they don't correspond 1:1 with what's actually on disk right now, and a stale/mismatched
   *  index into the live listing could otherwise make a mutating action hit a completely different real
   *  file than the one on screen. Every mutator below already funnels through this one chokepoint, so
   *  adding the check here — rather than re-deriving it at each call site — closes the hole everywhere
   *  at once (selection-dependent ops are additionally covered by `selectedEntries` being forced empty
   *  in `ExplorerPane.svelte` while `inReplay`; this also covers the selection-INDEPENDENT ones:
   *  new-folder, new-file, paste, ...). */
  function blockedInArchive(): boolean {
    if (smartFolder) {
      showNotice($t("smart.blockedNotice"), true);
      return true;
    }
    if (structuredSearch) {
      showNotice($t("smart.searchBlockedNotice"), true);
      return true;
    }
    if (archive) {
      showNotice($t("archive.blockedNotice"), true);
      return true;
    }
    if (replayOverlayEntries !== null) {
      showNotice($t("replay.blockedNotice"), true);
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
      const zipExt = zipPath.includes(".") ? zipPath.split(".").pop()!.toLowerCase() : "";
      let temp: string;
      if (ZIP_FAMILY_EXTS.has(zipExt)) {
        temp = unwrap(await commands.extractArchiveEntry(zipPath, entry.path));
      } else {
        // CPE-1181/CPE-1180: non-zip formats (tar/tar.gz/tgz/7z/iso) route through the
        // format-agnostic `extractArchiveEntryAny`. Feature-detect at runtime since that
        // command may not exist yet in `bindings.gen.ts` on this branch (CPE-1180 lands
        // separately) — fall back to a clear notice instead of a broken/typed call.
        const anyCmds = commands as unknown as {
          extractArchiveEntryAny?: (zip: string, inner: string) => Promise<{ status: "ok"; data: string } | { status: "error"; error: unknown }>;
        };
        if (typeof anyCmds.extractArchiveEntryAny !== "function") {
          showNotice($t("notice.archiveTypeUnsupported", { name: entry.name }), true);
          return;
        }
        temp = unwrap(await anyCmds.extractArchiveEntryAny(zipPath, entry.path));
      }
      unwrap(await commands.openExternal(temp));
    } catch {
      showNotice($t("notice.archiveEntryOpenFailed", { name: entry.name }), true);
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

  // ---- Smart-folder live-refresh on filesystem change (CPE-1230, epic CPE-978) ----
  // `smartPaths`/`loadStructuredSearchEntries` (declared above) already recompute reactively when the
  // TAG store or the saved query itself changes; this covers the other half of the DoD — a
  // create/delete/rename on DISK for a path the open smart folder cares about. Reuses the existing
  // `folder-watch` FS-event bus (CPE-794) instead of a second `notify` watcher: `reconcileWatch` folds
  // the open smart folder's scope into the SAME path set it already arms for the watched-folder-rules
  // feature, and the listener here is a second, independent consumer of that one event stream (rule
  // execution in `folderWatch.ts` is unaffected — it still only *acts* on a landed file that matches a
  // configured watch rule, a no-op for most users who have configured none).
  $: smartFolderScope = smartFolder
    ? ({ kind: "paths", paths: smartPaths } as const)
    : structuredSearch
      ? ({ kind: "root", root: resolveSavedSearchRoot(structuredSearch, currentPath) } as const)
      : null;

  let smartRefreshUnlisten: (() => void) | null = null;
  const smartRefreshDebounce = new TrailingDebounce(300); // mirrors the backend pumps' own debounce window

  /** Re-run whichever smart-folder recompute is live, reading the CURRENT reactive state (not a value
      captured when the listener was registered) so a debounced fire after a rapid open/close/switch
      always recomputes the folder that's actually open — or does nothing if none is. */
  function recomputeOpenSmartFolder() {
    if (smartFolder) void loadSmartEntries(smartFolder, smartPaths);
    else if (structuredSearch) void loadStructuredSearchEntries(structuredSearch);
  }

  /** Keep the live-refresh listener armed exactly while a smart folder is open (CPE-1230: "no always-on
      cost when no smart folder is open", "unsubscribe on exit") and, independently, re-arm the shared
      `folder-watch` backend watcher whenever the open folder's scope changes (opened, closed, or its
      match set/root moved) so the OS watcher actually covers it. */
  $: void manageSmartFolderLiveRefresh(smartFolderScope);
  async function manageSmartFolderLiveRefresh(scope: SmartFolderScope) {
    if (!scope) {
      smartRefreshDebounce.cancel();
      if (smartRefreshUnlisten) {
        smartRefreshUnlisten();
        smartRefreshUnlisten = null;
      }
    } else if (!smartRefreshUnlisten) {
      smartRefreshUnlisten = await listen<FolderWatchEvent[]>("folder-watch", (e) => {
        if (!smartFolderScope || !batchTouchesScope(e.payload, smartFolderScope)) return;
        smartRefreshDebounce.schedule(recomputeOpenSmartFolder);
      });
    }
    await reconcileWatch();
  }

  // Display-only Home preview (CPE-1132): single-clicking a Recent/Favorite file on the Home screen
  // drives the right preview/detail pane, matching every other middle-pane view. Home has no
  // `<FileList>` (see `ExplorerPane`'s `inHome` branch), so `selectedEntries` — which is derived from
  // `visible`/`selection` there and also feeds file OPERATIONS (delete/rename/copy/run/…) — is always
  // empty in Home and stays that way on purpose: this is a SEPARATE, read-only path that never touches
  // `selectedEntries`, so a Home-previewed file can never become an accidental op target. Cleared below
  // the instant it would go stale (leaving Home, or a real FileList selection landing).
  let homePreview: DirEntry | null = null;
  $: if (!isHome || selectedEntries.length > 0) homePreview = null;

  function showNotice(message: string, isError = false) {
    notice = message;
    noticeIsError = isError;
    clearTimeout(noticeTimer);
    noticeTimer = setTimeout(() => (notice = ""), 5000);
  }

  function setHistory(h: History) {
    tabs = tabs.map((t) => (t.id === activeId ? { ...t, history: h } : t));
  }

  /** Merge a resize event's widths (only the columns FileList actually rendered — i.e. resolved
   *  against the catalog) back into the full active set by id, so a column not yet resolved (catalog
   *  still loading) keeps its previous width instead of being dropped from the persisted set (CPE-1146). */
  function applyMetaColumnWidths(active: ActiveMetaColumn[], resized: { id: string; width: number }[]): ActiveMetaColumn[] {
    const widths = new Map(resized.map((r) => [r.id, r.width]));
    return active.map((c) => ({ id: c.id, width: widths.get(c.id) ?? c.width }));
  }

  async function loadPath(path: string, keepSelection = false, useCache = false) {
    const previouslySelected = keepSelection
      ? selectedIndices(selection).map((i) => visible[i]?.path).filter(Boolean)
      : [];

    smartFolder = null; // navigating to a real folder exits any open smart folder (CPE-667)
    structuredSearch = null; // ...or a saved structured search (CPE-1229)
    // ...or an open archive browse-view (CPE-1366). loadPath is real-filesystem navigation, so it must
    // exit the archive here — the single chokepoint that covers Back/Forward and every tab operation
    // (which call loadPath directly), not just the manual nav paths that guard with exitArchive(). Without
    // this, `archive` stranded while currentPath moved to a real folder (in-archive contents bleeding into
    // an unrelated tab / Back leaving you "inside" a zip at Home). enterArchive + in-archive navigation
    // never call loadPath, so this can't clear an archive we're legitimately entering/browsing.
    archive = null;

    if (!keepSelection) {
      selection = emptySelection();
      search = "";
      selectedTag = ""; // a tag filter is folder-scoped; leaving the folder clears it (CPE-639)
      fileFilter = "all"; // ...as is the file-type filter (CPE-1367): it's transient view state (not a
      // persisted preference like sort/view), so like `search`/`selectedTag` it must clear on real
      // navigation — otherwise a filter set in one tab/folder bleeds into every other tab (the CPE-1366
      // class of leak: a top-level view-var loadPath forgot to reset).
    }
    error = "";

    // Metadata columns (CPE-1146, epic CPE-707): restore THIS folder's saved column set + widths,
    // re-clamping widths on load (CPE-1140-style guard) so a stale/corrupt width can't paint a
    // too-narrow column. Home has no listing, so it never carries a column set.
    activeMetaColumns = path === HOME ? [] : clampMetaWidths(settings.loadMetaColumnsForFolder(path));

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

  /** Navigate pane B independently of pane A (dual-pane, CPE-677); persists its folder.
   *  `path === HOME` (CPE-1378, once pane B could actually land on Home): mirrors `loadPath`'s own HOME
   *  short-circuit above — HOME is the abstract landing view, not a real filesystem path, so there is no
   *  listing to fetch. Skipping `loadListing` here avoids issuing a bogus `list_dir(" home")` backend
   *  call (Home's `<HomeView>` reads `places`/`pins`/`recents`/etc., never `entries`/`visible`, so that
   *  call's result would go unused anyway) and the dev-only perf-mark instrumentation that comes with it. */
  async function navigateB(path: string, useCache = true) {
    paneBPath = path;
    settings.savePaneBPath(path);
    selectedTagB = ""; // a tag filter is folder-scoped (CPE-639); mirrors pane A's loadPath reset
    // Restore THIS folder's saved column set + widths (CPE-1382, follow-up to CPE-1378's shared-columns
    // gap) — mirrors `loadPath`'s `activeMetaColumns` derivation for pane A, but keyed by `paneBPath`
    // instead of `currentPath`, so pane B shows its own folder's active columns rather than pane A's.
    activeMetaColumnsB = path === HOME ? [] : clampMetaWidths(settings.loadMetaColumnsForFolder(path));
    if (path === HOME) {
      entriesB = [];
      loadingB = false;
      return;
    }
    await explorerPaneB?.loadListing(path, useCache);
  }

  /** Open an entry in pane B: descend into a folder, or open a file with the OS default (CPE-677). */
  async function openB(entry: DirEntry) {
    if (entry.is_dir) { await navigateB(entry.path); return; }
    try {
      unwrap(await commands.openExternal(entry.path));
      recents = settings.addRecent(recents, { path: entry.path, name: entry.name });
      settings.saveRecents(recents);
    } catch {
      showNotice($t("notice.noAssociatedApp", { name: entry.name }), true);
    }
  }

  /** Set row/chrome density (CPE-1526, epic CPE-1488); persists. No renderer reads this value yet —
   *  this ticket is the foundation seam CPE-1527/1528/1529 build the visible compact styling on. */
  function setDensity(d: DensityMode) {
    density = d;
    settings.saveDensity(density);
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
      const results = await commands.moveEntries(sources, to);
      reportResults(results, "move");
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

  /** Spotlight's `activate` event (CPE-1216): a "file" hit is revealed (folder opened + the file
   *  selected, like every other search dialog's `navigate` event); a "folder"/"recent" hit navigates
   *  straight there. Closes the overlay either way. */
  function onSpotlightActivate(detail: { path: string; kind: ResultKind }) {
    spotlightOpen = false;
    if (detail.kind === "file") revealFileInApp(detail.path);
    else navigateToTyped(detail.path);
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
      const parent = await commands.parentDir(currentPath);
      await navigate(parent ?? HOME);
    } catch {
      await navigate(HOME);
    }
  }

  async function refresh() {
    if (archive) {
      try {
        const entries = unwrap(await commands.readArchiveEntries(archive.zipPath));
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
      // Verify the path exists through the typed client (CPE-958). `listDir` returns a Result, so an
      // unreadable/missing path comes back as `status: "error"` rather than throwing — handle both.
      const r = await commands.listDir(expanded);
      if (r.status !== "ok") throw new Error(r.error);
      await navigate(expanded);
    } catch {
      showNotice($t("notice.pathNotFound", { path: raw }), true);
    }
  }

  let homePath = "";

  async function open(entry: DirEntry) {
    // CPE-1112 rework: `selectedEntries`/the FileList dispatch that normally feeds `entry` are already
    // neutralized while Replay mode's overlay is showing (`ExplorerPane.svelte` forces `selectedEntries`
    // empty and no-ops its `open` dispatch), but `open()` is also reachable via the command-palette/
    // Enter-key paths above — guard here too so opening a file/navigating a folder can never act on a
    // stale reference into whatever the LIVE listing happens to hold at that moment, not what's on
    // screen. Deliberately NOT routed through `blockedInArchive()` — browsing INSIDE an archive is
    // allowed (that's the very next branch below); only Replay mode's read-only reconstruction blocks.
    if (replayOverlayEntries !== null) return;
    if (archive) { await openInArchive(entry); return; }
    if (entry.is_dir) {
      await navigate(entry.path);
      return;
    }
    if (entry.extension === "cpevault") { await tryUnlockVault(entry); return; }
    if (isArchiveFile(entry)) { await enterArchive(entry); return; }
    try {
      // open_external runs it through the OS shell — reliably launches the
      // default app, and executes .exe/.cmd/.bat (CPE-240).
      unwrap(await commands.openExternal(entry.path));
      recents = settings.addRecent(recents, { path: entry.path, name: entry.name });
      settings.saveRecents(recents);
    } catch (e) {
      console.debug("open failed:", e);
      showNotice($t("notice.noAssociatedApp", { name: entry.name }), true);
    }
  }

  /** A row was clicked in the preview pane's FOLDER PEEK (CPE-1426, `FolderBrowser.svelte`'s `pick`
   *  event): descend exactly one level into `parent` (the folder currently highlighted/peeked) with
   *  `entry` pre-selected, so the preview then re-points at `entry`'s own preview (its own peek, if
   *  `entry` is itself a folder — the same click handler fires again next time, so this is how the
   *  whole tree gets walked one click at a time). Reuses the exact `pendingSelectPath` + `navigate()`
   *  mechanism `revealFileInApp` already uses for search-hit reveals — not a forked nav path, per the
   *  ticket. Back/Forward/breadcrumb/sidebar all update as a normal navigation because `navigate()` is
   *  the real one. */
  async function onFolderPeekPick(e: CustomEvent<{ parent: string; entry: DirEntry }>) {
    pendingSelectPath = e.detail.entry.path;
    await navigate(e.detail.parent);
  }

  /** A FILE row was double-clicked in the folder peek (`FolderBrowser.svelte`'s `open` event — never
   *  fired for a subfolder row, see its own doc comment). Lands the main pane on `parent` with the file
   *  selected first (so the selection/preview are consistent even if the open itself fails or the file
   *  needs a picker), then hands it to the normal open flow (`open()` — external app / archive-enter /
   *  vault-unlock, whichever applies), exactly like double-clicking that same file in the main list. */
  async function onFolderPeekOpen(e: CustomEvent<{ parent: string; entry: DirEntry }>) {
    pendingSelectPath = e.detail.entry.path;
    await navigate(e.detail.parent);
    await open(e.detail.entry);
  }

  // ---- Encrypted vaults (CPE-1249, epic CPE-738) ------------------------------------------------
  // Activating a `.cpevault` file (double-click / Enter) confirms it's a real vault, prompts for the
  // passphrase, decrypts it into a private session dir, and navigates INTO that dir so the tree is
  // browsable as a normal location. The unlocked-vault banner (below the toolbar) offers Lock, which
  // navigates back out and securely wipes the session dir. See vaultStore.ts.

  /** The blob path of the unlocked vault we're currently browsing inside, or `null` (drives the banner). */
  $: activeVaultBlob = vaultOfSessionPath($vaults, currentPath);

  /** Activation of a `.cpevault` row. If it's ALREADY unlocked, navigate straight back into its existing
   *  session dir — never re-unlock, which would allocate a fresh session dir and orphan the old plaintext
   *  on disk (review #1). Otherwise confirm it's really a vault via `vault_is` (magic header, not just the
   *  extension) and prompt for the passphrase. A `vault_is` I/O/permission error must NOT fall through to
   *  opening the ENCRYPTED blob externally (review #4). A `.cpevault`-named file that is genuinely not a
   *  vault opens with the OS default, so a mis-named file is never a dead end. */
  async function tryUnlockVault(entry: DirEntry) {
    if (isUnlocked($vaults, entry.path)) {
      const dir = sessionDirFor($vaults, entry.path);
      if (dir) {
        await navigate(dir);
        return;
      }
    }
    let isVault: boolean;
    try {
      isVault = unwrap(await commands.vaultIs(entry.path));
    } catch (e) {
      // Transient read failure — surface it, but never open the encrypted blob externally as a fallback.
      showNotice($t("notice.vaultReadFailed", { name: entry.name, error: String(e) }), true);
      return;
    }
    if (!isVault) {
      try {
        unwrap(await commands.openExternal(entry.path));
      } catch {
        showNotice($t("notice.cantOpen", { name: entry.name }), true);
      }
      return;
    }
    promptForVaultPassphrase(entry);
  }

  /** Show the passphrase prompt for a vault; on submit, unlock + navigate in. A failed unlock re-prompts
   *  with distinct copy (wrong password vs damaged file) and records NO state (vaultStore records only on
   *  success), so there's never a half-open vault. The passphrase stays in memory only — never logged. */
  function promptForVaultPassphrase(entry: DirEntry, error = "") {
    // A fresh object reference each attempt → the `{#key passwordPrompt}` template block remounts the
    // dialog (clean empty field + refocus + re-armed submit guard), so a wrong-password re-prompt never
    // reuses the stale masked value (CPE-1249 review #3).
    passwordPrompt = {
      title: `Unlock ${entry.name}`,
      message:
        `Enter the passphrase to unlock and browse "${entry.name}". While unlocked, its contents are ` +
        `decrypted into a private temporary folder; locking it wipes that folder.`,
      confirmLabel: "Unlock",
      error,
      onSubmit: async (passphrase) => {
        try {
          const sessionDir = await unlockVault(entry.path, passphrase);
          passwordPrompt = null;
          await navigate(sessionDir);
          showNotice($t("notice.vaultUnlocked", { name: entry.name }));
        } catch (e) {
          promptForVaultPassphrase(entry, classifyUnlockError(e));
        }
      },
    };
  }

  /** Lock the vault we're browsing: navigate OUT of the session dir FIRST (it's about to be wiped), then
   *  ask the backend to lock (shred + remove the session dir) and update the store. On a wipe FAILURE the
   *  vault stays unlocked (retryable) — navigate BACK INTO the session dir so the banner + Lock button
   *  reappear and the user can retry, rather than stranding the plaintext with no in-app affordance
   *  (review #2). */
  async function lockActiveVault(blobPath: string) {
    const sessionDir = sessionDirFor($vaults, blobPath);
    const back = parentOfPath(blobPath) || HOME;
    await navigate(back);
    try {
      await lockVault(blobPath);
      showNotice($t("notice.vaultLocked", { name: vaultDisplayName(blobPath) }));
    } catch {
      if (sessionDir) await navigate(sessionDir); // re-expose the banner's Lock button for a retry
      showNotice($t("notice.vaultLockFailed", { name: vaultDisplayName(blobPath) }), true);
    }
  }

  // ---- Create encrypted vault (CPE-1250, epic CPE-738) ------------------------------------------
  // Seal a selected folder into a `.cpevault`. The context menu offers this only for a single folder in a
  // real filesystem location (see the `vaultable` prop); the dialog owns passphrase entry, the optional
  // destructive shred-original confirm, and the backend `vault_create` call. On success we refresh the
  // listing and select the new blob (it lands as a sibling of the folder, i.e. in the current folder).

  /** Open the create-vault dialog for the single selected folder. Guarded to a real folder (never Home/
   *  archive) — the same condition ContextMenu gates the menu item on. `inPaneB` (CPE-1386): a
   *  context-menu invocation targets whichever pane the menu was opened OVER (`runAction`'s `inPaneB`
   *  local) — the folder + pane are SNAPSHOT into `vaultCreateFor` right now, mirroring
   *  `beginBatchRename`'s reasoning (CPE-1384): the dialog stays open for as long as it takes to type a
   *  passphrase, and a pane switch underneath it must never retarget the eventual (possibly destructive
   *  shred-original) create. `archive`/`isHome` are pane-A-only concepts (pane B is always a plain real
   *  folder), so they only gate a pane-A vault-create. */
  function askVaultCreate(inPaneB = false) {
    const pane = paneStateFor(inPaneB);
    const entry = pane.selectedEntries[0];
    const dir = inPaneB ? paneBPath : currentPath;
    if (!entry?.is_dir || (inPaneB ? dir === HOME : (isHome || archive))) return;
    vaultCreateFor = { folderPath: entry.path, folderName: entry.name, inPaneB, dir };
  }

  // ---- Check archive safety (CPE-1318, epic CPE-1002) --------------------------------------------
  // Surfaces the built-but-unwired `analyze_archive_safety` backend command behind a right-click
  // action on a ZIP-family archive file. The context menu gates on `archiveSafetyEligible` (same
  // guard, re-checked here defense-in-depth like `askVaultCreate` above); the dialog itself owns the
  // call + result rendering.

  /** Open the archive-safety dialog for the single selected ZIP-family archive. `inPaneB` (CPE-1386):
   *  the dialog is a read-only, single-shot scan (no follow-up refresh needed), so this only needs to
   *  read the right pane's selection — no snapshot object required beyond the plain `path` it already
   *  passes down. */
  function askArchiveSafety(inPaneB = false) {
    const pane = paneStateFor(inPaneB);
    const entry = pane.selectedEntries[0];
    const dir = inPaneB ? paneBPath : currentPath;
    if (!entry || (inPaneB ? dir === HOME : (isHome || archive)) || !isArchiveSafetyEligible(entry)) return;
    archiveSafetyFor = entry.path;
  }

  /** `VaultCreateDialog`'s `created` handler: the new `.cpevault` blob path. Refresh whichever pane(s)
   *  currently show the SNAPSHOT `dir` `askVaultCreate` captured (CPE-1386, mirrors
   *  `refreshBatchApplyTarget`'s both-can-match reasoning) — not live `currentPath`/`paneBPath`, so a
   *  pane renavigated away while the dialog was open is left alone instead of getting a wrong refresh.
   *  The pane-A branch keeps the exact pre-CPE-1386 call (`loadPath(currentPath, true)`, `pendingSelectPath`
   *  set first) so single-pane behavior is byte-for-byte unchanged; pane B has no equivalent
   *  post-load "select the new item" hook yet (same as batch-rename/batch-media), so it's a plain refresh. */
  async function onVaultCreated(dest: string) {
    const target = vaultCreateFor;
    vaultCreateFor = null;
    if (!target) return;
    const norm = normalizePath(target.dir);
    if (norm === normalizePath(currentPath)) {
      pendingSelectPath = dest; // the post-load hook selects it; the reactive block scrolls to it
      await loadPath(currentPath, true);
    }
    if (dualPane && paneBPath && norm === normalizePath(paneBPath)) await explorerPaneB?.loadListing(paneBPath, false);
    showNotice($t("notice.vaultCreated", { name: vaultDisplayName(dest) }));
  }

  // ---- Certificate management (CPE-1423/1424, epic CPE-1417) ------------------------------------
  // Two dialogs (CreateCertDialog / SignCertDialog) wired behind the pane-aware context menu (CPE-1424)
  // + a command-palette entry each. `inPaneB` (CPE-1377/1384 pattern): a context-menu invocation targets
  // whichever pane the menu was opened OVER (`runAction`'s `inPaneB` local); the command palette always
  // targets pane A's `currentPath`.

  /** Open CreateCertDialog (CPE-1424): "Create certificate here…" on a folder row targets that folder as
   *  the default output location; on empty space / the command palette it targets the clicked/active
   *  pane's own current folder. `dir` (the pane's displayed folder) is SNAPSHOT now, mirroring
   *  `askVaultCreate` — the dialog can stay open for a while, and a pane navigating away underneath it
   *  must not retarget the eventual refresh. */
  function askCertCreate(inPaneB = false) {
    const pane = paneStateFor(inPaneB);
    const entry = pane.selectedEntries[0];
    const dir = inPaneB ? paneBPath : currentPath;
    if (inPaneB ? dir === HOME : (isHome || archive)) return;
    const outDir = entry?.is_dir ? entry.path : dir;
    certCreateFor = { dir, outDir, inPaneB };
  }

  /** Open SignCertDialog (CPE-1424): `prefill.csrPath`/`prefill.caCertPath` carry the clicked file for
   *  "Issue cert from this CSR…" / "Sign with this as CA…"; both omitted for the command-palette entry
   *  (no file context). `dir` is SNAPSHOT the same way `askCertCreate` does. */
  function askCertSign(inPaneB = false, prefill: { csrPath?: string; caCertPath?: string } = {}) {
    const dir = inPaneB ? paneBPath : currentPath;
    if (inPaneB ? dir === HOME : (isHome || archive)) return;
    certSignFor = { dir, inPaneB, csrPath: prefill.csrPath ?? "", caCertPath: prefill.caCertPath ?? "" };
  }

  /** "Inspect" / "Inspect JWT" (CPE-1424): the row is already selected — right-clicking selects first
   *  (`onRowContext`).
   *
   *  SINGLE-PANE: the preview pane auto-decodes a cert/CSR/JWT file on selection (CPE-1422), so this only
   *  needs to make sure the Preview tab (not Details) is what's showing — unchanged behavior.
   *
   *  DUAL-PANE (CPE-1438): that inline preview slot is occupied by pane B's ExplorerPane, so flipping the
   *  flags did NOTHING — a silent no-op. Instead open the decode in an overlay (`InspectCryptoDialog`)
   *  that reuses the same JwtPreview/CertPreview viewers, the "a modal works in dual-pane" pattern the
   *  Create/Sign cert dialogs already use. `inPaneB` (CPE-1377 pattern): inspect the file from whichever
   *  pane the menu was opened OVER, not the live active pane. */
  function inspectCryptoFile(inPaneB = false) {
    if (dualPane) {
      const entry = paneStateFor(inPaneB).selectedEntries[0];
      if (!entry) return;
      cryptoInspectFor = { path: entry.path, kind: isJwtFile(entry) ? "jwt" : "cert" };
      return;
    }
    showDetails = true;
    settings.saveShowDetails(true);
    showPreview = true;
    settings.saveShowPreview(true);
  }

  /** `CreateCertDialog`'s `created` handler: the new certificate's full path. Refresh whichever pane(s)
   *  currently show the SNAPSHOT `dir` `askCertCreate` captured — mirrors `onVaultCreated`'s reasoning
   *  (not live `currentPath`/`paneBPath`, so a pane renavigated away while the dialog was open is left
   *  alone). */
  async function onCertCreated(certPath: string) {
    const target = certCreateFor;
    certCreateFor = null;
    if (!target) return;
    const norm = normalizePath(target.dir);
    if (norm === normalizePath(currentPath)) {
      pendingSelectPath = certPath;
      await loadPath(currentPath, true);
    }
    if (dualPane && paneBPath && norm === normalizePath(paneBPath)) await explorerPaneB?.loadListing(paneBPath, false);
    showNotice($t("notice.certCreated", { name: fileNameOf(certPath) }));
  }

  /** `SignCertDialog`'s `created` handler: the issued certificate's full path. Same refresh reasoning as
   *  `onCertCreated`. */
  async function onCertSigned(certPath: string) {
    const target = certSignFor;
    certSignFor = null;
    if (!target) return;
    const norm = normalizePath(target.dir);
    if (norm === normalizePath(currentPath)) {
      pendingSelectPath = certPath;
      await loadPath(currentPath, true);
    }
    if (dualPane && paneBPath && norm === normalizePath(paneBPath)) await explorerPaneB?.loadListing(paneBPath, false);
    showNotice($t("notice.certIssued", { name: fileNameOf(certPath) }));
  }

  /** Basename of a full path, separator-agnostic — for the create/issue success toasts above. */
  function fileNameOf(path: string): string {
    const idx = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
    return idx < 0 ? path : path.slice(idx + 1);
  }

  // ---- File split/join (CPE-1509, parent CPE-1491) ---------------------------------------------
  // SplitFileDialog / JoinPartsDialog wired behind the pane-aware context menu, same `inPaneB` pattern
  // as the certificate dialogs above: a context-menu invocation targets whichever pane the menu was
  // opened OVER.

  /** Open SplitFileDialog (CPE-1509): "Split file…" on a single selected non-empty regular file.
   *  `dir` (the pane's displayed folder at open time) is SNAPSHOT, mirroring `askCertCreate` — the
   *  dialog's own output-folder field defaults elsewhere and can differ from `dir` entirely, so
   *  `onSplitFileDone` refreshes off the dialog's ACTUAL `outDir`, not this snapshot. */
  function askSplitFile(inPaneB = false) {
    const pane = paneStateFor(inPaneB);
    const entry = pane.selectedEntries[0];
    if (!entry) return;
    const dir = inPaneB ? paneBPath : currentPath;
    splitFileFor = { path: entry.path, dir, inPaneB };
  }

  /** Open JoinPartsDialog (CPE-1509): "Join parts…" on a selected `.NNN` part or manifest file. */
  function askJoinParts(inPaneB = false) {
    const pane = paneStateFor(inPaneB);
    const entry = pane.selectedEntries[0];
    if (!entry) return;
    const dir = inPaneB ? paneBPath : currentPath;
    joinPartsFor = { path: entry.path, dir, inPaneB };
  }

  /** `SplitFileDialog`'s `split` handler: refresh whichever pane(s) currently show the manifest's ACTUAL
   *  output folder (the dialog's own field, which the user may have Browse'd elsewhere — not necessarily
   *  `splitFileFor.dir`, the source file's folder). */
  async function onSplitFileDone(detail: { manifest: SplitManifest; outDir: string }) {
    const target = splitFileFor;
    splitFileFor = null;
    if (!target) return;
    const norm = normalizePath(detail.outDir);
    if (norm === normalizePath(currentPath)) await loadPath(currentPath, true);
    if (dualPane && paneBPath && norm === normalizePath(paneBPath)) await explorerPaneB?.loadListing(paneBPath, false);
    const n = detail.manifest.part_count;
    showNotice($t(n === 1 ? "notice.splitDoneOne" : "notice.splitDoneMany", { name: baseName(target.path), count: n }));
  }

  /** `JoinPartsDialog`'s `joined` handler: `outPath`'s full path — refresh whichever pane(s) currently
   *  show its containing folder and select the newly-joined file. */
  async function onJoinPartsDone(outPath: string) {
    const target = joinPartsFor;
    joinPartsFor = null;
    if (!target) return;
    const norm = normalizePath(parentOfPath(outPath));
    if (norm === normalizePath(currentPath)) {
      pendingSelectPath = outPath;
      await loadPath(currentPath, true);
    }
    if (dualPane && paneBPath && norm === normalizePath(paneBPath)) await explorerPaneB?.loadListing(paneBPath, false);
    showNotice($t("notice.joinedInto", { name: baseName(outPath) }));
  }

  async function openRecent(path: string) {
    try {
      unwrap(await commands.openExternal(path));
    } catch {
      // A recent file that no longer opens is removed rather than nagging forever.
      recents = recents.filter((r) => r.path !== path);
      settings.saveRecents(recents);
      showNotice($t("home.recentFileGone"), true);
    }
  }

  /** Display-only Home selection (CPE-1132): single-clicking a Recent file drives the right
   *  preview/detail pane. Reuses `entries_for_paths` (the same stat-a-path-into-a-`DirEntry` command
   *  smart folders use, CPE-667) rather than `entry_info` — its return type already IS the real
   *  `DirEntry` the preview/details panes read elsewhere, so there's no separate field-mapping to get
   *  wrong. Silently clears `homePreview` if the file has since moved/vanished (self-healing, same as
   *  smart folders) instead of surfacing an error for what is, after all, just a passive preview. */
  async function selectHomeEntry(path: string): Promise<void> {
    try {
      const [found] = await commands.entriesForPaths([path]);
      homePreview = found ?? null;
    } catch {
      homePreview = null;
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
    showNotice($t("notice.openedInNewTab", { name: entry.name }));
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
    selectByAutoSave = false;
    const idx = selectMatching(visible, condition, Date.now());
    // CPE-1373: keep the current lead (scroll position) instead of jumping to the match's max index.
    selection = selectIndices(idx, selection.lead);
    showNotice(
      idx.length === 0
        ? $t("notice.selectByNoneMatch")
        : $t(idx.length === 1 ? "notice.selectedItemsOne" : "notice.selectedItemsMany", { count: idx.length }),
    );
  }

  /**
   * "Save search…" (CPE-1229, epic CPE-978): capture the SAME `Condition` "Select by…" builds — the one
   * structured search this app has — as a named `SavedSearch` via the CPE-1228 store, instead of (or as
   * well as) applying it to the current selection. `match: "all"` is the only sensible choice for a
   * single condition (all vs. any are equivalent with one term). `root` captures the folder open right
   * now, since the open-evaluator later scans recursively from wherever the search was saved (no
   * whole-computer index exists yet to search "everywhere").
   */
  function saveCurrentSearch(payload: { name: string; condition: Condition }) {
    selectByOpen = false;
    selectByAutoSave = false;
    addSavedSearch(payload.name, [payload.condition], "all", currentPath);
    showNotice($t("smart.searchSaved", { name: payload.name }));
  }

  function selectByPattern(pattern: string) {
    patternSelectOpen = false;
    const idx = visible
      .map((e, i) => (matchesGlob(e.name, pattern) ? i : -1))
      .filter((i) => i >= 0);
    // CPE-1373: keep the current lead (scroll position) instead of jumping to the match's max index.
    selection = selectIndices(idx, selection.lead);
    showNotice(
      idx.length === 0
        ? $t("notice.selectByPatternNoneMatch", { pattern })
        : $t(idx.length === 1 ? "notice.selectedItemsMatchingOne" : "notice.selectedItemsMatchingMany", { count: idx.length, pattern }),
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
    : structuredSearch ? structuredSearch.name
    : isHome ? "Home" : (splitPath(currentPath).at(-1)?.name ?? currentPath);

  // The DetailsPane no-selection placeholder hero icon (CPE-1234): a structured saved search or a
  // tag smart folder is a virtual view, not Home, so its placeholder must use that view's own
  // sidebar glyph ("search" / "filter") rather than defaulting to Home's icon — otherwise it
  // contradicts the breadcrumb/search-box/status-bar, which all correctly say "saved search".
  // Archive + real-folder + Home all keep the pre-existing "home" glyph, unchanged.
  $: folderIcon = structuredSearch ? "search" : smartFolder ? "filter" : "home";

  // Folder-context detection (CPE-235): runs on the RAW listing (so hidden
  // markers like `.git` are seen regardless of the show-hidden setting).
  $: folderContexts = (isHome || archive || smartFolder || structuredSearch) ? [] : detectContexts({ path: currentPath, entries });

  // The sort/hidden/search/type/tag pipeline that turns the base listing into `visible` (+ its pre-filter
  // `shown`) now lives in <ExplorerPane> (CPE-676 domino 2). App resolves the base list + archive/smart
  // mode and passes them down; `visible`/`shown` are bound back for the status bar + operations.

  /** All tags with counts, for the sidebar Tags section. */
  $: tagList = tagCounts($tags);


  $: crumbs = archive
    ? [{ name: "Home", path: HOME }, ...splitPath(currentPath), ...archiveCrumbs(archive)]
    : smartFolder
      ? [{ name: "Home", path: HOME }, { name: smartFolder.name, path: "" }]
      : structuredSearch
        ? [{ name: "Home", path: HOME }, { name: structuredSearch.name, path: "" }]
        : isHome
          ? [{ name: "Home", path: HOME }]
          : [{ name: "Home", path: HOME }, ...splitPath(currentPath)];

  // `selectedEntries` is derived and owned by <ExplorerPane> now (bound above); App only consumes it.
  $: selectedSize = selectedEntries.reduce((n, e) => n + (e.is_dir ? 0 : e.size), 0);
  $: itemCount = (isHome && !smartFolder && !structuredSearch) ? places.length + drives.length + pins.length : visible.length;
  // The folder's pre-filter total, so the status bar can read "X of Y items" (CPE-407).
  $: totalCount = ((isHome && !smartFolder && !structuredSearch) || archive) ? itemCount : shown.length;
  $: pasteCheck = clipCanPaste(clipboard, isHome ? "" : currentPath);
  $: cutPaths = clipboard.mode === "cut" ? clipboard.paths : [];
  // CPE-1533: a pure (no-notice) mirror of doPaste's `isHome || blockedInArchive()` guard, so the Drop
  // Stack panel's Move-all/Copy-all buttons can be disabled up front rather than firing and eating a
  // silent no-op — `blockedInArchive()` itself has notice side effects, so it's called for real inside
  // the click handlers below, not here.
  $: dropStackDestBlocked = isHome || !!archive || !!smartFolder || !!structuredSearch || replayOverlayEntries !== null;
  // CPE-1380: the shared `<ContextMenu>`'s "Paste" row must reflect whichever pane it was opened OVER
  // (`ctxInPaneB`), not always pane A's `pasteCheck` — a paste-into-itself/self-descendant refusal (or a
  // plain empty clipboard) has to be evaluated against pane B's OWN folder when the menu is over pane B.
  // `paneBPath === HOME` is pane B's "no real destination" case (mirrors `isHome` for pane A).
  $: ctxPasteCheck = clipCanPaste(clipboard, ctxInPaneB ? (paneBPath === HOME ? "" : paneBPath) : (isHome ? "" : currentPath));
  // CPE-1377: which pane the currently-OPEN `<ContextMenu>` (`ctx`) is FOR — read from `ctx.inPaneB`
  // (menu-open-time), never live `activePane` (focus-time); see the `ctx` declaration's comment. Drives
  // every entry-derived `<ContextMenu>` prop below so a pane-B right-click shows pane B's selection, not
  // pane A's leftover one. `ctxPane` is meaningless while `ctx` is null but harmless (defaults to pane A)
  // — the menu itself only renders `{#if ctx}`.
  //
  // Deliberately NOT `paneStateFor(ctxInPaneB)` here: Svelte's `$:` dependency tracking is static — it
  // only sees identifiers written directly in the reactive statement's own expression, not ones a called
  // function reads internally. `paneStateFor(ctxInPaneB)` would only ever re-run when `ctxInPaneB`
  // itself changes, NOT when `selection`/`selectedEntries`/`selectionB`/`selectedEntriesB` change on
  // their own — leaving the menu showing a stale selection (caught by a real test regression: "Open in
  // new tab" stopped appearing because `folderSelected` was one flush behind the just-updated
  // selection). Referencing every field directly, even in the untaken ternary branch, makes them real
  // dependencies. `paneStateFor` itself stays fine for plain (non-reactive) function calls like
  // `runAction`'s, which always read live values at call time regardless of Svelte's tracking.
  $: ctxInPaneB = ctx?.inPaneB ?? false;
  $: ctxPane = ctxInPaneB
    ? { selection: selectionB, visible: visibleB, selectedEntries: selectedEntriesB }
    : { selection, visible, selectedEntries };

  $: tabList = tabs.map((t) => {
    const p = current(t.history) ?? HOME;
    return { id: t.id, title: p === HOME ? "Home" : (splitPath(p).at(-1)?.name ?? p) };
  });

  $: if (selection.lead >= 0 && rowEls[selection.lead]) {
    rowEls[selection.lead].scrollIntoView({ block: "nearest" });
  }

  // ---- file operations ----
  /** Which file op just ran — drives the translated success/failure wording `reportResults` shows.
   *  Kept as an identifier (not the display text itself, CPE-1634) so every caller stays English-free;
   *  the actual per-language sentences live in the `op.*` catalog keys below. */
  type OpKind = "move" | "moveToBin" | "rename" | "duplicate" | "deletePermanent" | "deleteSecure";
  const OP_SUCCESS_ONE: Record<OpKind, string> = {
    move: "op.moveOne",
    moveToBin: "op.moveToBinOne",
    rename: "op.renameOne",
    duplicate: "op.duplicateOne",
    deletePermanent: "op.deletePermanentOne",
    deleteSecure: "op.deleteSecureOne",
  };
  const OP_SUCCESS_MANY: Record<OpKind, string> = {
    move: "op.moveMany",
    moveToBin: "op.moveToBinMany",
    rename: "op.renameMany",
    duplicate: "op.duplicateMany",
    deletePermanent: "op.deletePermanentMany",
    deleteSecure: "op.deleteSecureMany",
  };
  const OP_FAILED_SINGLE: Record<OpKind, string> = {
    move: "op.failedSingleMove",
    moveToBin: "op.failedSingleMoveToBin",
    rename: "op.failedSingleRename",
    duplicate: "op.failedSingleDuplicate",
    deletePermanent: "op.failedSingleDeletePermanent",
    deleteSecure: "op.failedSingleDeleteSecure",
  };
  function reportResults(results: OpResult[], opKind: OpKind) {
    const failed = results.filter((r) => !r.ok);
    if (failed.length === 0) {
      showNotice($t(results.length === 1 ? OP_SUCCESS_ONE[opKind] : OP_SUCCESS_MANY[opKind], { count: results.length }));
    } else {
      // Never swallow a partial failure — name what went wrong.
      const first = failed[0];
      const name = first.path.split(/[\\/]/).pop() ?? first.path;
      showNotice(
        failed.length === 1
          ? $t(OP_FAILED_SINGLE[opKind], { name, error: first.error })
          : $t("op.failedMany", { failed: failed.length, total: results.length, name, error: first.error }),
        true,
      );
    }
  }

  /** `inPaneB` (CPE-1370 review): pane B is always a plain real folder in v1 — never an archive/smart
   *  folder/saved search/Replay reconstruction, all of which are pane-A-only virtual views — so
   *  `blockedInArchive()` (which reads pane-A-only state) must only gate a pane-A rename. Every caller
   *  except the F2 keyboard path (which can target either pane) targets pane A, so it defaults false. */
  function beginRename(entry: DirEntry, inPaneB = false) {
    if (!inPaneB && blockedInArchive()) return;
    if (inPaneB) {
      renamingPathB = entry.path;
      renameValueB = entry.name;
    } else {
      renamingPath = entry.path;
      renameValue = entry.name;
    }
  }

  /** Open the batch-rename dialog for the current multi-selection (CPE-255). `inPaneB` (CPE-1384): a
   *  context-menu invocation targets whichever pane the menu was opened OVER (`runAction`'s `inPaneB`
   *  local) — the source folder + selection are SNAPSHOT into `batchRenameFor` right now (see its own
   *  doc comment), never re-derived from live state once the dialog is open. `blockedInArchive()` is a
   *  pane-A-only concept (archive/smartFolder/Replay), so it only gates a pane-A batch rename. */
  function beginBatchRename(inPaneB = false) {
    const pane = paneStateFor(inPaneB);
    if ((!inPaneB && blockedInArchive()) || pane.selectedEntries.length < 2) return;
    batchRenameFor = { entries: pane.selectedEntries, inPaneB, dir: inPaneB ? paneBPath : currentPath };
  }

  /** Apply a batch rename: one move_exact within the SNAPSHOT folder `beginBatchRename` captured, pushed
   *  as a single undoable step (CPE-255) — `target` (not live `currentPath`/`activePane`) is what's
   *  replayed here, so a pane switch while the dialog was open can't retarget the rename (CPE-1384,
   *  mirrors `doDelete`'s snapshot replay). */
  async function applyBatchRename(items: RenameItem[]) {
    const target = batchRenameFor;
    batchRenameFor = null;
    if (!target || items.length === 0) return;
    const { dir } = target;
    const pairs: [string, string][] = items.map((it) => [
      joinPath(dir, it.from),
      joinPath(dir, it.to),
    ]);
    try {
      const results = await commands.moveExact(pairs);
      reportResults(results, "rename");
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
      await refreshBatchApplyTarget(dir);
    } catch (e) {
      showNotice(String(e), true);
    }
  }

  /** Open the batch-media dialog for the current multi-selection (CPE-1093): pre-filters out any
   *  non-image/unsupported-extension files (reusing the same `isImage` check the thumbnailer and
   *  Quick-look use) rather than sending them to the backend and having every op fail per-file. `inPaneB`
   *  (CPE-1384): a context-menu invocation targets whichever pane the menu was opened OVER (`runAction`'s
   *  `inPaneB` local) — snapshot into `batchMediaFor` (see its own doc comment) so a later pane switch
   *  can't retarget the refresh. `blockedInArchive()` is a pane-A-only concept, so it only gates pane A. */
  function beginBatchMedia(inPaneB = false) {
    const pane = paneStateFor(inPaneB);
    if ((!inPaneB && blockedInArchive()) || pane.selectedEntries.length < 2) return;
    const { eligible, skipped } = partitionEligible(pane.selectedEntries);
    if (eligible.length < 2) {
      showNotice($t("ctx.batchMediaTooFew"), true);
      return;
    }
    if (skipped > 0) {
      showNotice($t("notice.imagesSkipped", { skipped, total: pane.selectedEntries.length }));
    }
    batchMediaFor = { entries: eligible, inPaneB, dir: inPaneB ? paneBPath : currentPath };
  }

  /** Apply a completed batch-media run (CPE-1093): the dialog itself streams the execute + shows its own
   *  progress (per BUSY-CURSOR.md), so by the time this fires the job has already finished — report the
   *  outcome, refresh the pane `beginBatchMedia` snapshot targeted (CPE-1384), and close. */
  /** CPE-1590: a folder whose checkpoint failed outright before an in-place overwrite has NO recovery net
   *  at all, so that warning outranks the ordinary converted/skipped summary — it must reach the user even
   *  if they dismissed the dialog's own warning panel on reflex (Escape / backdrop click both route here
   *  too). CPE-1599 UAT follow-up: `partial` (a checkpoint that succeeded but left some file(s)
   *  uncaptured) is a **materially better** situation than `failures` (no checkpoint at all) and must read
   *  that way — collapsing them into one softened sentence let a folder with ZERO protection sound like a
   *  minor gap, which is exactly what this warning exists to prevent. Since `showNotice` is a single
   *  banner, an outright failure (worse) leads; a concurrent partial is appended, not dropped, so both
   *  are still surfaced when a run hits both kinds in one go. */
  function noticeCheckpointFailures(failures: string[], partial: CheckpointPartial[] = []): boolean {
    if (failures.length === 0 && partial.length === 0) return false;
    if (failures.length > 0) {
      const name = failures[0].split(/[\\/]/).pop() || failures[0];
      const extra = failures.length - 1;
      const partialCount = partial.length;
      // CPE-1634: a single $t() call per combination — never concatenate separately-translated
      // fragments into one sentence (word order/pluralization differ too much across languages for
      // that to read correctly; see the ticket). "folder(s)"/"file(s)" is a deliberate least-bad
      // shorthand in the English source strings themselves since this i18n layer has no CLDR plural
      // rules (see interpolate() in lib/i18n.ts) — documented in the ticket's work log.
      const key =
        extra === 0
          ? partialCount === 0
            ? "notice.checkpointFailed"
            : "notice.checkpointFailedPartial"
          : partialCount === 0
            ? "notice.checkpointFailedExtra"
            : "notice.checkpointFailedExtraPartial";
      showNotice($t(key, { name, extra, partialCount }), true);
    } else {
      const p = partial[0];
      const name = p.dir.split(/[\\/]/).pop() || p.dir;
      const extra = partial.length - 1;
      showNotice(
        $t(extra === 0 ? "notice.checkpointPartialOnly" : "notice.checkpointPartialOnlyExtra", {
          name,
          extra,
          skippedCount: p.skippedCount,
        }),
        true,
      );
    }
    return true;
  }

  async function applyBatchMedia(
    report: BatchReport,
    checkpointFailures: string[] = [],
    checkpointPartial: CheckpointPartial[] = [],
  ) {
    const target = batchMediaFor;
    batchMediaFor = null;
    const failed = report.skipped.length;
    if (noticeCheckpointFailures(checkpointFailures, checkpointPartial)) {
      // the checkpoint warning stands alone — don't overwrite it with the routine summary
    } else if (failed === 0) {
      showNotice($t(report.written === 1 ? "notice.convertedOne" : "notice.convertedMany", { count: report.written }));
    } else {
      const [firstPath, firstReason] = report.skipped[0];
      const name = firstPath.split(/[\\/]/).pop() ?? firstPath;
      showNotice(
        $t("notice.convertedWithSkipped", { written: report.written, failed, name, reason: firstReason }),
        report.written === 0,
      );
    }
    if (target) await refreshBatchApplyTarget(target.dir);
  }

  /** After a batch-rename/batch-media apply (CPE-1387), refresh whichever pane(s) currently show `dir` —
   *  the SNAPSHOT `beginBatchRename`/`beginBatchMedia` captured as the folder actually operated on — not
   *  live `paneBPath`/`currentPath`. If the targeted pane was renavigated elsewhere while the dialog was
   *  open, live `paneBPath`/`currentPath` no longer names the operated folder, so reloading it would
   *  refresh the WRONG listing; matching against the snapshot instead means a renavigated pane is simply
   *  left alone (it's already showing its own fresh folder) while any pane still ON `dir` gets refreshed.
   *  Mirrors `refreshDropSourcePane`'s both-can-match reasoning (CPE-1371): `dir` can be showing in BOTH
   *  panes at once (a mirrored folder), so refresh every pane that currently matches it. */
  async function refreshBatchApplyTarget(dir: string) {
    const norm = normalizePath(dir);
    if (dualPane && paneBPath && norm === normalizePath(paneBPath)) await explorerPaneB?.loadListing(paneBPath, false);
    if (norm === normalizePath(currentPath)) await loadPath(currentPath);
  }

  /** `inPaneB` (CPE-1377): mirrors `beginRename`'s parameter — reads/clears the right pane's
   *  `renamingPath{,B}`, looks the entry up in the right pane's `visible{,B}`, and reloads the right
   *  pane afterward (matching `doDelete`'s `inPaneB ? explorerPaneB?.loadListing(...) : loadPath(...)`
   *  pattern) so a pane-B rename never touches pane A's listing or vice versa. */
  async function commitRename(newName: string, inPaneB = false) {
    const path = inPaneB ? renamingPathB : renamingPath;
    if (inPaneB) renamingPathB = ""; else renamingPath = "";
    if (!path) return;

    const entry = (inPaneB ? visibleB : visible).find((e) => e.path === path);
    if (!entry || newName.trim() === "" || newName === entry.name) return;

    const invalid = validateFileName(newName);
    if (invalid) {
      showNotice(invalid, true);
      return;
    }

    try {
      const to = unwrap(await commands.renameEntry(path, newName));
      // Carry any tags to the new path so they follow the file (CPE-652); best-effort.
      retagPath(path, to).catch(() => {});
      // Carry favourites/frecency/recents entries too (CPE-1224); best-effort.
      migrateFrontendStores(path, to);
      undoStack = pushUndo(undoStack, {
        kind: "rename",
        moves: [{ from: path, to }],
        label: `Rename to "${newName}"`,
      });
      if (inPaneB) {
        if (paneBPath) await explorerPaneB?.loadListing(paneBPath, false);
      } else {
        await loadPath(currentPath);
      }
    } catch (e) {
      showNotice(String(e), true);
    }
  }

  /** Undo the last rename or move. Copies and deletes are deliberately not
      undoable — see the comment at the top of lib/undo.ts. */
  async function undo() {
    if (blockedInArchive()) return;
    const { entry, rest } = popUndo(undoStack);
    if (!entry) {
      showNotice($t("ctx.undoNothing"));
      return;
    }
    try {
      let results: OpResult[];

      if (entry.kind === "delete") {
        // Only ever pushed onto the stack when the platform can restore, so we
        // never reach here on macOS.
        results = await commands.restoreFromTrash(deletedPaths(entry));
      } else {
        const pairs = invert(entry).map((m) => [m.from, m.to] as [string, string]);
        results = await commands.moveExact(pairs);
      }

      const failed = results.filter((r) => !r.ok);
      if (failed.length > 0) {
        // Do NOT pop the entry on failure — the user can retry once they've
        // cleared whatever is in the way.
        showNotice($t("notice.undoFailed", { error: failed[0].error }), true);
        return;
      }
      undoStack = rest;
      showNotice($t("notice.undone", { label: entry.label }));
      await loadPath(currentPath);
    } catch (e) {
      showNotice(String(e), true);
    }
  }

  /** `inPaneB` (CPE-1377 review): every existing caller targets pane A, so it defaults false — only
   *  `runAction`'s pane-routed "new-folder"/"new-folder-in" (and the file counterparts) pass true, for a
   *  menu opened over pane B. */
  async function newFolder(targetDir: string = currentPath, inPaneB = false) {
    await createNewItem("folder", targetDir, undefined, inPaneB);
  }

  async function newFile(targetDir: string = currentPath, spec?: NewFileType, inPaneB = false) {
    await createNewItem("file", targetDir, spec, inPaneB);
  }

  /** Create a new folder / text file in `targetDir` and inline-rename it (CPE-1156).
   *
   *  `targetDir` defaults to the folder in view (empty-area menu, palette, Ctrl+Shift+N, and a file
   *  right-click all create in the current folder). Right-clicking a FOLDER passes that folder's path so
   *  the new item lands INSIDE it — including a drive root reached that way. UX choice: we create the
   *  item, then navigate INTO the target folder so the user sees it and names it inline (via
   *  `pendingRenamePath`), matching Windows-Explorer intuition. The `(2)` auto-number dedups against the
   *  TARGET folder's real contents — the in-view `entries` when creating in place, or a fresh `listDir`
   *  when creating inside another folder (its listing isn't loaded yet).
   *
   *  `inPaneB` (CPE-1377 review): pane B's empty-area/folder-row context menu can now reach this too, so
   *  every pane-A-only assumption below is mirrored for pane B — the "in place" check is against
   *  `paneBPath` (not `currentPath`), the dedupe list is `entriesB` (not `entries`), the post-create
   *  navigate-into-subfolder uses `navigateB` (with a forced fresh, non-cached load — same reasoning as
   *  pane A's `loadPath(targetDir, false, false)`) instead of pushing pane A's tab history, the in-place
   *  reload hits pane B's own `<ExplorerPane>` directly, and the inline rename is kicked off straight
   *  into `renamingPathB` via `beginRename(entry, true)` rather than the pane-A-only `pendingRenamePath`
   *  hook (which only `loadPath` — pane A's navigation function — ever consults). Pane B is always a
   *  plain real folder in v1 (see `beginRename`'s own comment), so `blockedInArchive()` — which reads
   *  pane-A-only archive state — only applies when targeting pane A. */
  async function createNewItem(kind: "folder" | "file", targetDir: string, spec?: NewFileType, inPaneB = false) {
    // Guard the abstract Home landing (no real path) and read-only archives — but NOT real drive roots,
    // which are ordinary paths (`isHome` is only ever true for the Home landing itself).
    if (targetDir === HOME || (!inPaneB && blockedInArchive())) return;

    const paneRoot = inPaneB ? paneBPath : currentPath;
    const inSubfolder = targetDir !== paneRoot;

    // Names to dedupe against: the in-view listing when creating in place; a fresh listing of the target
    // folder when creating inside a folder we haven't opened (so "New folder (2)" is correct there).
    let existing: string[];
    if (inSubfolder) {
      const res = await commands.listDir(targetDir);
      existing = res.status === "ok" ? res.data.map((e) => e.name) : [];
    } else {
      existing = (inPaneB ? entriesB : entries).map((e) => e.name);
    }

    try {
      let created: string;
      if (kind === "folder") {
        const name = uniqueName("New folder", existing);
        created = unwrap(await commands.createDir(targetDir, name));
      } else {
        // Default (no spec) is the plain Text file (.txt). A spec (CPE-1161) carries its own extension
        // and creation strategy: empty file, a minimal valid stub (RTF), or a valid empty .zip archive.
        const base = spec?.base ?? "New Text Document";
        const ext = spec ? `.${spec.ext}` : ".txt";
        const name = uniqueNameWithExt(base, ext, existing);
        if (spec?.zip) {
          created = unwrap(await commands.createEmptyZip(targetDir, name));
        } else if (spec?.content != null) {
          created = unwrap(await commands.createFileWithContent(targetDir, name, spec.content));
        } else {
          created = unwrap(await commands.createFile(targetDir, name));
        }
      }
      if (inPaneB) {
        // Fresh (non-cached) load either way — a cached listing wouldn't contain the just-created item.
        if (inSubfolder) await navigateB(targetDir, false);
        else await explorerPaneB?.loadListing(targetDir, false);
        const i = visibleB.findIndex((e) => e.path === created);
        if (i >= 0) {
          selectionB = selectOnly(i);
          beginRename(visibleB[i], true);
        }
      } else {
        pendingRenamePath = created; // select + inline-rename it once the target listing loads
        if (inSubfolder) {
          // Navigate INTO the target folder (adds to history like any navigation) with a FRESH load — a
          // cached listing wouldn't contain the just-created item, so the rename wouldn't fire.
          setHistory(visit(activeTab.history, targetDir));
          await loadPath(targetDir, false, false);
        } else {
          await loadPath(currentPath);
        }
      }
    } catch (e) {
      showNotice(String(e), true);
    }
  }

  /** `NewLinkDialog`'s `created` handler (CPE-1207): reload + inline-rename the new link, mirroring
   *  `createNewItem`'s create-then-rename flow — `pendingRenamePath` is the same hook both use. */
  async function onNewLinkCreated(createdPath: string) {
    newLinkDialogFor = null;
    pendingRenamePath = createdPath;
    await loadPath(currentPath);
  }

  /** `NewLinkDialog`'s `error` handler (CPE-1207): surface the backend failure via the app-wide toast —
   *  on Windows this is often the Developer-Mode/elevation error `create_symlink` returns when it lacks
   *  privilege, and it must never be swallowed ([[avoid-modal-permission-popups]]: plain text, no modal).
   *  The dialog itself stays open (it shows the same message inline) so the user can retry or switch to
   *  a hardlink instead of losing their typed target/name. */
  function onNewLinkError(message: string) {
    showNotice(message, true);
  }

  /** `RepairLinkDialog`'s `repaired` handler (CPE-1209): the broken link now points at `newTarget` —
   *  reload so its badge/status catches up, and close the dialog. */
  async function onLinkRepaired(newTarget: string) {
    repairLinkFor = null;
    showNotice($t("notice.linkRepaired", { target: newTarget }));
    await loadPath(currentPath);
  }

  /** `RepairLinkDialog`'s `error` handler (CPE-1209) — same reasoning as `onNewLinkError`: surface via
   *  the toast, dialog stays open (it shows the same message inline) so the user can try another target. */
  function onLinkRepairError(message: string) {
    showNotice(message, true);
  }

  /**
   * Navigation Mode (CPE-1556, epic CPE-1487): map a resolved `NavIntent` onto the app's EXISTING
   * handlers — no new file-op or motion logic lives here, only wiring. `motion` drives CPE-1553's
   * selection bridge (`applyNavIntent`) against whichever pane the modal layer is acting on; the `op`s
   * reuse the very same `doCopy`/`doCut`/`doPaste` the Ctrl+C/Ctrl+X/Ctrl+V branches call (vim maps
   * `d`=cut, `y`=copy, `p`=paste — delete-into-register semantics); `startFilter` reuses the toolbar's
   * existing filter entry point (exactly what Ctrl+F focuses — no second filter UI); `startCommand`
   * opens the `:` command line. `enterVisual`/`exitVisual`/`none` need no side effect here — the caller
   * already applied the new mode/buffers to `navState` via `reduceNavKey`. Called only from the
   * `handleKeydown` guard, which never fires when `navigationModeEnabled` is off. */
  function dispatchNavIntent(intent: NavIntent, inPaneB: boolean) {
    switch (intent.kind) {
      case "motion": {
        const p = paneStateFor(inPaneB);
        const cols = currentGridCols();
        const layout: NavLayout = cols > 1 ? "grid" : "list";
        p.setSelection(applyNavIntent(intent, p.selection, p.visible.length, navState.mode, layout, cols));
        break;
      }
      case "op":
        if (intent.op === "yank") doCopy(inPaneB);
        else if (intent.op === "delete") doCut(inPaneB);
        else void doPaste(inPaneB); // "paste" — async, mirrors the Ctrl+V branch's fire-and-forget
        break;
      case "startFilter":
        navToolbar?.focusSearch(); // same entry point Ctrl+F drives — do not build a second filter UI
        break;
      case "startCommand":
        navCommandLineOpen = true;
        break;
      // enterVisual / exitVisual / none: mode + buffers were already updated by reduceNavKey in the caller.
    }
  }

  /** `inPaneB` (CPE-1380): Ctrl+C/context-menu-copy must stage whichever pane's selection is actually
   *  targeted — `handleKeydown` passes the live active pane (`activePaneState`'s routing), a context-menu
   *  invocation passes `ctx.inPaneB` (menu-open-time, same reasoning as `askDelete`'s override). Defaults
   *  to pane A so every other/legacy caller and all single-pane behavior is unchanged.
   *  `blockedInArchive()` reads pane-A-only archive state, so — like `createNewItem`/`askDelete` — it only
   *  gates a pane-A copy; pane B is always a plain real folder in v1. */
  function doCopy(inPaneB = false) {
    const pane = paneStateFor(inPaneB);
    if ((!inPaneB && blockedInArchive()) || pane.selectedEntries.length === 0) return;
    clipboard = stage(pane.selectedEntries.map((e) => e.path), "copy");
    showNotice($t(clipboard.paths.length === 1 ? "notice.copiedItemsOne" : "notice.copiedItemsMany", { count: clipboard.paths.length }));
  }

  /** Same pane-aware reasoning as `doCopy` (CPE-1380) — the cut set is captured from whichever pane was
   *  actually targeted, at cut time, so a later paste (possibly into the OTHER pane) moves the right
   *  files regardless of which pane is active by then. */
  function doCut(inPaneB = false) {
    const pane = paneStateFor(inPaneB);
    if ((!inPaneB && blockedInArchive()) || pane.selectedEntries.length === 0) return;
    clipboard = stage(pane.selectedEntries.map((e) => e.path), "cut");
    showNotice($t(clipboard.paths.length === 1 ? "notice.cutItemsOne" : "notice.cutItemsMany", { count: clipboard.paths.length }));
  }

  /** Add the pane's current selection to the Drop Stack (CPE-1531, epic CPE-1489) — the persistent
   *  cross-folder "shelf" whose store/persistence landed in CPE-1530. Same pane-aware, archive-gated
   *  shape as `doCopy`/`doCut` above (CPE-1380): a context-menu invocation targets whichever pane the
   *  menu was opened OVER via `inPaneB`, and pane A is blocked while browsing inside an archive (an
   *  archive member has no real on-disk path to shelve). `addedFrom` is the pane's own current folder,
   *  same source-dir reasoning as `copyMoveToFolder`'s `srcDir`. */
  function doAddToDropStack(inPaneB = false) {
    const pane = paneStateFor(inPaneB);
    if ((!inPaneB && blockedInArchive()) || pane.selectedEntries.length === 0) return;
    const from = inPaneB ? paneBPath : currentPath;
    addToDropStack(pane.selectedEntries.map((e) => e.path), from);
    const n = pane.selectedEntries.length;
    showNotice($t(n === 1 ? "notice.addedToDropStackOne" : "notice.addedToDropStackMany", { count: n }));
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
      no cut/navigate/paste dance. A move leaves the source folder, so it reloads and is
      undoable; a copy only reloads when the destination is a pane's own folder in view.
   *  `inPaneB` (CPE-1384): a context-menu invocation targets whichever pane the menu was opened OVER
   *  (`runAction`'s `inPaneB` local). The source selection + its folder are captured into `pane`/
   *  `sources`/`srcDir` BEFORE the native picker opens (mirroring `snapshotConfirmTarget`, CPE-1370) so
   *  a MOVE — destructive: it deletes the source — can never retarget onto the other pane if the active
   *  pane changes while the (OS-modal) picker is up. Pane B is always a plain real folder in v1, so its
   *  "no real destination" case is `paneBPath === HOME` (mirrors `isHome` for pane A). */
  async function copyMoveToFolder(move: boolean, inPaneB = false) {
    const pane = paneStateFor(inPaneB);
    if ((inPaneB ? paneBPath === HOME : (isHome || archive)) || pane.selectedEntries.length === 0) return;
    const sources = pane.selectedEntries.map((e) => e.path);
    const srcDir = inPaneB ? paneBPath : currentPath;
    const n = sources.length;
    let dest: string | string[] | null;
    try {
      dest = await openFolderDialog({
        directory: true,
        multiple: false,
        defaultPath: srcDir,
        title: `${move ? "Move" : "Copy"} ${n} item${n === 1 ? "" : "s"} to…`,
      });
    } catch {
      return; // dialog unavailable / errored — no-op
    }
    if (!dest || typeof dest !== "string") return; // cancelled

    // COPY → the transfer engine (CPE-625): shows the operations panel; the transfer://done listener
    // refreshes + reports. keep-both preserves auto-rename. (Copies aren't undoable.) Tagging the id in
    // `pasteCopyPaneB` (same mechanism `startCopyWithPolicy` uses for a pane-B clipboard paste, CPE-1380)
    // makes the shared listener refresh pane B once the queued copy finishes, instead of always
    // refreshing pane A regardless of which pane's "Copy to…" started it.
    if (!move) {
      try {
        const id = await startTransfer(sources, dest, "copy", "keepboth");
        if (inPaneB) pasteCopyPaneB.add(id);
      } catch (e) {
        showNotice(String(e), true);
      }
      return;
    }

    // MOVE → existing synchronous path (keeps undo).
    try {
      const results = await commands.moveEntries(sources, dest);
      reportResults(results, "move");
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
      // Refresh whichever pane(s) show the source folder (CPE-1371's both-can-match reasoning) — not
      // just the pane the op started from, since pane A/B can mirror the same folder.
      await refreshDropSourcePane(sources);
    } catch (e) {
      showNotice(String(e), true);
    }
  }

  /** After a move, carry each moved file's tags to its new path so they follow it (CPE-657).
      Best-effort + fire-and-forget; an untagged file is a cheap no-op. */
  function retagMoves(moves: { from: string; to: string }[]) {
    for (const m of moves) retagPath(m.from, m.to).catch(() => {});
    for (const m of moves) migrateFrontendStores(m.from, m.to);
  }

  /**
   * Carry favourites, spotlight frecency, and recents/recent-folders entries to a path's new location
   * after an in-app rename/move (CPE-1224, frontend analog of CPE-657/CPE-1222) — exact path + subtree,
   * via the shared `migratePathList` helper. Best-effort: a migration hiccup must never break the
   * rename/move itself, so every step is wrapped and silently swallowed on failure. Favourites/recents/
   * recentFolders are mirrored into App.svelte's own reactive state (so the UI updates immediately);
   * spotlight frecency has no live in-memory copy here (Spotlight.svelte reloads it on open), so it's
   * migrated straight through the settings.ts load/save pair.
   */
  function migrateFrontendStores(from: string, to: string) {
    try {
      favorites = migratePathList(favorites, from, to);
      settings.saveFavorites(favorites);
    } catch { /* best-effort — see doc comment above */ }
    try {
      recents = migratePathList(recents, from, to);
      settings.saveRecents(recents);
    } catch { /* best-effort — see doc comment above */ }
    try {
      recentFolders = migratePathList(recentFolders, from, to);
      settings.saveRecentFolders(recentFolders);
    } catch { /* best-effort — see doc comment above */ }
    try {
      settings.saveSpotlightFrecency(migratePathList(settings.loadSpotlightFrecency(), from, to));
    } catch { /* best-effort — see doc comment above */ }
  }

  /** Start a copy of `sources` into the current folder — or pane B's (`inPaneB`, CPE-1380) — with the
   *  chosen conflict policy (CPE-624). Tags the resulting transfer id in `pasteCopyPaneB` when it targets
   *  pane B so the shared `transfer://done` listener refreshes the right pane once it finishes.
   *  `confirmed` (CPE-1662) is the overwrite consent, defaulting to *not given* — see `startTransfer`. */
  async function startCopyWithPolicy(sources: string[], policy: ConflictPolicy, inPaneB = false, confirmed = false) {
    try {
      const id = await startTransfer(sources, inPaneB ? paneBPath : currentPath, "copy", policy, confirmed);
      if (inPaneB) pasteCopyPaneB.add(id);
    } catch (e) {
      showNotice(String(e), true);
    }
  }

  /** The conflict dialog's choice: run the pending copy with that policy (CPE-624), at the pane
   *  `doPaste` originally captured it for (CPE-1380). This handler — the user having clicked a button in
   *  `TransferConflictDialog` — is one of only two places allowed to pass `confirmed: true` (CPE-1662); the
   *  flag is passed separately from `policy` rather than derived from it, so that consent means "we
   *  asked", not merely "overwrite was selected". */
  function resolveCopyConflict(policy: ConflictPolicy) {
    const p = pendingCopy;
    pendingCopy = null;
    if (p) startCopyWithPolicy(p.sources, policy, p.inPaneB, true);
  }

  /** `inPaneB` (CPE-1380): Ctrl+V/context-menu-paste must target whichever pane is actually meant — the
   *  keyboard path passes the live active pane, a context-menu invocation passes `ctx.inPaneB`
   *  (menu-open-time). The destination is that pane's OWN folder — `paneBPath`, not `currentPath` —
   *  mirroring `newFolder`/`askDelete`'s pane routing. `isHome`/`blockedInArchive()` are pane-A-only
   *  concepts (pane B is always a plain real folder in v1); pane B's equivalent "no real destination" is
   *  `paneBPath === HOME`. */
  async function doPaste(inPaneB = false) {
    if (inPaneB ? paneBPath === HOME : (isHome || blockedInArchive())) return;
    if (clipEmpty(clipboard)) return;
    const destPath = inPaneB ? paneBPath : currentPath;
    const check = clipCanPaste(clipboard, destPath);
    if (!check.allowed) {
      showNotice(check.reason, true);
      return;
    }
    const wasCut = clipboard.mode === "cut";
    const sources = [...clipboard.paths];

    // COPY → the transfer engine (CPE-613): progress shows in the operations panel and the
    // transfer://done listener refreshes the folder + reports. Copies aren't undoable, so there's no
    // undo coupling. If names would collide, ask how to resolve the batch (CPE-624); otherwise
    // "keepboth" preserves the old auto-rename-on-collision behaviour.
    if (!wasCut) {
      const destEntries = inPaneB ? entriesB : entries;
      const collisions = collidingNames(sources, destEntries.map((e) => e.name));
      if (collisions.length > 0) {
        pendingCopy = { sources, count: collisions.length, inPaneB };
        return; // the conflict dialog resumes via startCopyWithPolicy
      }
      startCopyWithPolicy(sources, "keepboth", inPaneB);
      return;
    }

    // MOVE → the existing synchronous path: instant same-volume rename and undo support. Both the
    // source pane(s) (the moved rows disappear) and the destination pane (the moved rows newly appear)
    // must refresh — `refreshPasteAffectedPanes` mirrors `refreshDropSourcePane`'s both-can-match
    // reasoning (CPE-1371), extended to the paste destination since (unlike a drag-drop, which always
    // lands ON a child row) a paste's destination IS the target pane's own current-folder listing.
    //
    // CPE-1385: clear the clipboard SYNCHRONOUSLY, before the `await`, and operate on the local
    // `sources` snapshot taken above — not on `clipboard` again. Two rapid Ctrl+V within the async
    // window both used to read the same non-empty cut clipboard and both call moveEntries with the
    // same sources (a double-move). Clearing before the await means the second doPaste's
    // `clipEmpty(clipboard)` check at the top sees an empty clipboard and no-ops. Copy is unaffected —
    // that branch returns above without ever reaching this clear, so paste-copy can still repeat.
    clipboard = emptyClipboard();
    try {
      const results = await commands.moveEntries(sources, destPath);
      reportResults(results, "move");
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
      // CPE-1385 review (Reviewer + UAT): `moveEntries` resolves with one `OpResult` PER source, index-
      // aligned with `sources` (same correlation `moves` above already relies on) — a partial failure
      // (permission denied / locked file / one item on a dropped network share) must not silently drop
      // that item from the clipboard just because its SIBLINGS moved. Re-stage only the paths that did
      // NOT move as a fresh cut set, so a retry paste only re-attempts what actually failed — the
      // already-moved paths are correctly gone for good. `clipEmpty` guards against clobbering: if the
      // user cut something else (or copied) during this await, the clipboard is no longer empty and must
      // be left alone rather than overwritten with this call's stale, now-irrelevant leftovers.
      // CPE-1385 review (Reviewer + UAT): `moveEntries` resolves with one `OpResult` PER source, index-
      // aligned with `sources` (same correlation `moves` above already relies on) — a partial failure
      // (permission denied / locked file / one item on a dropped network share) must not silently drop
      // that item from the clipboard just because its SIBLINGS moved. Re-stage only the paths that did
      // NOT move as a fresh cut set, so a retry paste only re-attempts what actually failed — the
      // already-moved paths are correctly gone for good. `clipEmpty` guards against clobbering: if the
      // user cut something else (or copied) during this await, the clipboard is no longer empty and must
      // be left alone rather than overwritten with this call's stale, now-irrelevant leftovers.
      const unmoved = sources.filter((_, i) => !results[i]?.ok);
      if (unmoved.length > 0 && clipEmpty(clipboard)) clipboard = stage(unmoved, "cut");
      await refreshPasteAffectedPanes(sources, inPaneB);
    } catch (e) {
      showNotice(String(e), true);
      // CPE-1385 review: the call itself rejected (IPC/backend error) rather than resolving with
      // per-item results — nothing was moved at all, so restore the FULL cut set (same `clipEmpty` guard
      // as above) rather than leaving the user's selection silently gone and forcing a re-cut.
      if (clipEmpty(clipboard)) clipboard = stage(sources, "cut");
    }
  }

  /** CPE-1533 (epic CPE-1489 finale): "Move all here"/"Copy all here" — everything currently shelved on
   *  the Drop Stack (CPE-1530), regardless of which folder each item was picked up from (that's the
   *  whole point of the stack, see dropStack.ts's doc comment), targeted at the CURRENT folder. There's
   *  no pane-B equivalent — CPE-1532's panel is one global dock, not pane-scoped — so these mirror only
   *  `doPaste`'s pane-A guard (`isHome || blockedInArchive()`), never the `inPaneB` branch.
   *
   *  "Move all" reuses the SAME synchronous move path `doPaste`'s cut branch uses (instant same-volume
   *  rename + undo support via `moveEntries`, which resolves with one `OpResult` PER source, index-
   *  aligned with `sources`) rather than the async transfer queue — so a partial failure can precisely
   *  clear only the paths that actually moved and leave the rest shelved for a retry. */
  // CPE-1538 (doPaste's CPE-1385 fix, parity review of CPE-1533): a fast double-click on "Move all here"
  // dispatches two doDropStackMoveAll calls back-to-back, both reading the same non-empty
  // $dropStackEntries before either's `await commands.moveEntries(...)` settles — a double-move. doPaste
  // guards its cut branch by synchronously CLEARING the clipboard before the await; the Drop Stack can't
  // do that the same way because a partial failure needs to re-shelve only the specific un-moved entries
  // (see the per-item handling below) — clearing eagerly would lose each entry's original `addedFrom`/
  // `addedAt` for anything that has to come back. So this uses the ticket's other sanctioned option: a
  // synchronous in-flight FLAG (same pattern as `reconcileInFlight` above), set before the first `await`
  // and released in `finally` — a second click within the same tick just no-ops.
  let dropStackMoveInFlight = false;
  async function doDropStackMoveAll() {
    if (isHome || blockedInArchive()) return;
    if (dropStackMoveInFlight) return;
    const sources = $dropStackEntries.map((e) => e.path);
    if (sources.length === 0) return;
    dropStackMoveInFlight = true;
    try {
      const results = await commands.moveEntries(sources, currentPath);
      reportResults(results, "move");
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
      // Mirrors doPaste's `unmoved` handling (CPE-1385 review): only the paths that actually moved come
      // off the stack — a permission-denied/locked/dropped-network-share item among the batch stays
      // shelved instead of silently vanishing.
      sources.forEach((p, i) => { if (results[i]?.ok) removeFromDropStack(p); });
      await loadPath(currentPath);
    } catch (e) {
      showNotice(String(e), true);
      // Nothing moved at all (IPC/backend rejection) — leave the whole batch shelved.
    } finally {
      dropStackMoveInFlight = false;
    }
  }

  /** "Copy all here" — routed through the transfer queue (CPE-613), same as `doPaste`'s copy branch:
   *  progress shows in the operations panel, the shared `transfer://done` listener does the refresh +
   *  notice, and a name collision against the destination pauses for the same CPE-624 conflict dialog
   *  (`pendingDropStackCopy`). Unlike the move path above, `TransferReport` is aggregate-only counts (no
   *  per-path result — see transfers.ts), so `dropStackTransferOps` (consumed by the `transfer://done`
   *  listener below) only clears the captured paths off the stack on a clean, uncancelled, all-
   *  transferred finish; a partial failure leaves the whole batch shelved rather than guessing which
   *  paths landed.
   *
   *  CPE-1538 review: deliberately NOT given the move path's re-entrancy guard. A double-click here
   *  double-fires `startTransfer` the same way two rapid `doPaste` copies do — and `doPaste`'s own copy
   *  branch is exempt from CPE-1385's guard for the same reason (see its comment): copy doesn't destroy
   *  the source, so the worst case is a redundant transfer / an extra "(2)"-suffixed duplicate via the
   *  keepboth conflict policy, not data loss or a spurious failure notice. Guarding it anyway would mean
   *  losing entries' original `addedFrom`/`addedAt` on the same re-shelve problem `doDropStackMoveAll`'s
   *  comment above describes, for no correctness upside. */
  async function doDropStackCopyAll() {
    if (isHome || blockedInArchive()) return;
    const sources = $dropStackEntries.map((e) => e.path);
    if (sources.length === 0) return;
    const collisions = collidingNames(sources, entries.map((e) => e.name));
    if (collisions.length > 0) {
      pendingDropStackCopy = { sources, count: collisions.length };
      return; // the conflict dialog resumes via startDropStackCopy
    }
    await startDropStackCopy(sources, "keepboth");
  }

  /** `confirmed` (CPE-1662): the overwrite consent, defaulting to *not given* — see `startTransfer`. */
  async function startDropStackCopy(sources: string[], policy: ConflictPolicy, confirmed = false) {
    try {
      const id = await startTransfer(sources, currentPath, "copy", policy, confirmed);
      dropStackTransferOps.set(id, sources);
    } catch (e) {
      showNotice(String(e), true);
    }
  }

  /** The conflict dialog's choice for a pending Drop-Stack copy (CPE-624/CPE-1533). The second (and
   *  last) place allowed to pass CPE-1662's `confirmed: true` — the user clicked a
   *  `TransferConflictDialog` button; see `resolveCopyConflict` for why the flag is separate from
   *  `policy`. */
  function resolveDropStackCopyConflict(policy: ConflictPolicy) {
    const p = pendingDropStackCopy;
    pendingDropStackCopy = null;
    if (p) startDropStackCopy(p.sources, policy, true);
  }

  /** Fetch a text file's contents for the preview pane (size-capped backend). */
  function loadPreviewText(path: string): Promise<string> {
    return commands.readFileText(path, PREVIEW_MAX_BYTES).then(unwrap);
  }

  /** List an archive's entries for the preview pane. */
  function loadArchiveEntries(path: string): Promise<ArchiveEntry[]> {
    return commands.readArchiveEntries(path).then(unwrap);
  }

  /** Read a read-only text summary of a binary file for the preview pane. */
  function loadPreviewInfo(path: string): Promise<string> {
    return commands.readPreviewInfo(path).then(unwrap);
  }

  /** Decode a non-native image (TIFF/PSD) to a data: URL for the preview pane. */
  function loadImageData(path: string): Promise<string> {
    return commands.readImageDataUrl(path).then(unwrap);
  }

  /** Extract a camera-RAW file's (cr2/nef/arw) embedded JPEG preview for the preview pane. */
  function loadRawImageData(path: string): Promise<string> {
    return commands.readRawPreviewDataUrl(path).then(unwrap);
  }

  /** Decode a DICOM file's pixel data to a data: URL for the preview pane. */
  function loadDicomImageData(path: string): Promise<string> {
    return commands.readDicomImageDataUrl(path).then(unwrap);
  }

  /** Read a curated set of DICOM tags for the preview pane. */
  function loadDicomTags(path: string): Promise<[string, string][]> {
    return commands.readDicomTags(path).then(unwrap);
  }

  /** Decode a HEIC/HEIF file to a data: URL (platform image stack) for the preview pane. */
  function loadHeicImageData(path: string): Promise<string> {
    return commands.readHeicPreviewDataUrl(path).then(unwrap);
  }

  /** Structural-validity check for a PDF (CPE-1357), called before the preview pane hands the file to
   *  the WebView2 iframe — a malformed/empty PDF rejects here instead of crashing the renderer. */
  function loadPdfValidity(path: string): Promise<number | null> {
    return commands.readPdfValidity(path).then(unwrap);
  }

  /** Save edited text back to a file, then refresh so size/date update. */
  async function savePreviewText(path: string, contents: string): Promise<void> {
    unwrap(await commands.writeFileText(path, contents));
    await loadPath(currentPath);
  }

  /** Copy the selection's full path(s) to the OS clipboard, quoted, one per line — Explorer's "Copy as
   *  path". `entries` (CPE-1377) defaults to pane A's `selectedEntries` for every existing caller
   *  (keyboard shortcut, command palette); `runAction`'s "copy-path" case passes the context-menu's
   *  pane-aware `pane.selectedEntries` so a pane-B right-click copies pane B's path, not pane A's. */
  async function doCopyPath(entries: DirEntry[] = selectedEntries) {
    if (entries.length === 0) return;
    const text = formatPathsForClipboard(entries.map((e) => e.path));
    try {
      await navigator.clipboard.writeText(text);
      showNotice($t(entries.length === 1 ? "notice.copiedPathsOne" : "notice.copiedPathsMany"));
    } catch {
      showNotice($t("notice.copyPathFailed"), true);
    }
  }

  /** Copy just the selected item's name to the clipboard (CPE-248). `entries` defaults as `doCopyPath`
   *  does above (CPE-1377). */
  async function doCopyName(entries: DirEntry[] = selectedEntries) {
    const entry = entries[0];
    if (!entry) return;
    try {
      await navigator.clipboard.writeText(entry.name);
      showNotice($t("notice.copiedName", { name: entry.name }));
    } catch {
      showNotice($t("notice.copyNameFailed"), true);
    }
  }

  /** Reveal the selected item (or the current folder) in the OS file manager (CPE-247). `entries` +
   *  `fallback` default to pane A (CPE-1377) — `runAction`'s "reveal" case passes pane B's selection +
   *  `paneBPath` when the menu was opened over pane B, so it reveals the right pane's item/folder. */
  async function revealInExplorer(entries: DirEntry[] = selectedEntries, fallback: string = isHome ? "" : currentPath) {
    const target = entries.length === 1 ? entries[0].path : fallback;
    if (!target) return;
    try {
      await revealItemInDir(target);
    } catch {
      showNotice($t("notice.revealFailed"), true);
    }
  }

  /** Open the OS terminal with its working directory set to `path` (CPE-253). */
  async function openTerminal(path: string) {
    if (isHome || archive || !path) return;
    try {
      unwrap(await commands.openTerminal(path));
    } catch {
      showNotice($t("notice.openTerminalFailed"), true);
    }
  }

  /** Pin/unpin the selected folder in the Home view (CPE-249). `entries` defaults to pane A's selection
   *  for every existing caller; `runAction`'s "pin" case passes the context-menu's pane-aware selection
   *  (CPE-1377) so pinning FROM pane B pins the folder actually right-clicked. `pins` itself is a single
   *  shared list either way (Home is not per-pane). */
  function togglePinSelected(entries: DirEntry[] = selectedEntries) {
    const entry = entries[0];
    if (!entry?.is_dir) return;
    const wasPinned = pins.includes(entry.path);
    pins = settings.togglePin(pins, entry.path);
    settings.savePins(pins);
    showNotice(wasPinned ? $t("notice.unpinnedFromHome", { name: entry.name }) : $t("notice.pinnedToHome", { name: entry.name }));
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

  /** Star/unstar the single selected item (file or folder) as a Favorite (CPE-338). `entries` defaults
   *  as `togglePinSelected` does above (CPE-1377). */
  function toggleFavoriteSelected(entries: DirEntry[] = selectedEntries) {
    const entry = entries[0];
    if (!entry) return;
    const wasFav = favorites.some((f) => f.path === entry.path);
    favorites = settings.toggleFavorite(favorites, {
      path: entry.path,
      name: entry.name,
      is_dir: entry.is_dir,
    });
    settings.saveFavorites(favorites);
    showNotice(wasFav ? $t("notice.removedFromFavorites", { name: entry.name }) : $t("notice.addedToFavorites", { name: entry.name }));
  }

  /** Duplicate the selection in place — copy it into the folder it lives in. Not undoable, for the
   *  same reason a copy-paste isn't (see doPaste). `inPaneB` (CPE-1384): a context-menu invocation
   *  targets whichever pane the menu was opened OVER (`runAction`'s `inPaneB` local); Ctrl+D passes the
   *  live active pane, same routing `doCopy`/`doCut`/`doPaste`'s keyboard path already uses (CPE-1380).
   *  Pane B is always a plain real folder in v1, so its "no real destination" case is `paneBPath ===
   *  HOME` (mirrors `isHome` for pane A); `blockedInArchive()` (archive/smartFolder/Replay) is a
   *  pane-A-only concept and only gates a pane-A duplicate. */
  async function doDuplicate(inPaneB = false) {
    const pane = paneStateFor(inPaneB);
    if ((inPaneB ? paneBPath === HOME : (isHome || blockedInArchive())) || pane.selectedEntries.length === 0) return;
    const sources = pane.selectedEntries.map((e) => e.path);
    const dir = inPaneB ? paneBPath : currentPath;
    try {
      const results = await commands.copyEntries(sources, dir);
      reportResults(results, "duplicate");
      if (inPaneB) { if (paneBPath) await explorerPaneB?.loadListing(paneBPath, false); }
      else await loadPath(currentPath);
    } catch (e) {
      showNotice(String(e), true);
    }
  }

  /** Compare exactly two selected files for byte-identical content (CPE-418) → a notice. */
  async function compareFiles() {
    if (selectedEntries.length !== 2 || selectedEntries.some((e) => e.is_dir)) return;
    const [a, b] = selectedEntries;
    try {
      const same = unwrap(await commands.filesIdentical(a.path, b.path));
      showNotice($t(same ? "notice.filesIdentical" : "notice.filesDiffer", { a: a.name, b: b.name }));
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

  /** The base name for a new archive: the single selected item's name (folder) or stem (file), or
   *  "Archive" for a multi-selection. Shared by every compress variant (CPE-251/1182/1183). `sel`
   *  (CPE-1386) is the pane-aware selection the caller already resolved — no longer reads the pane-A-only
   *  `selectedEntries` global directly, so a pane-B compress computes the right base name. */
  function compressBaseName(sel: DirEntry[]): string {
    return sel.length === 1
      ? sel[0].is_dir
        ? sel[0].name
        : stripExt(sel[0].name)
      : "Archive";
  }

  /** Compress the selection into a new .zip in the current folder (CPE-251), through the transfer
   *  queue (CPE-1184) so a large selection streams progress + stays cancellable instead of freezing the
   *  UI on one blocking call. The actual "Compressed…" notice happens once the queued run finishes, via
   *  the global `transfer://done` listener below, which also does the folder refresh (`pendingArchiveOps`'
   *  `dir`, CPE-1386). `inPaneB`: a context-menu invocation targets whichever pane the menu was opened
   *  OVER (`runAction`'s `inPaneB` local) — `isHome`/`blockedInArchive()` are pane-A-only concepts (pane B
   *  is always a plain real folder), so they only gate a pane-A compress; a pane-B compress is instead
   *  gated on `paneBPath === HOME` (the closest pane-B equivalent, same as `copyMoveEligible`). */
  async function doCompress(inPaneB = false) {
    const pane = paneStateFor(inPaneB);
    const dir = inPaneB ? paneBPath : currentPath;
    if ((!inPaneB && (isHome || blockedInArchive())) || (inPaneB && dir === HOME) || pane.selectedEntries.length === 0) return;
    const name = uniqueNameWithExt(compressBaseName(pane.selectedEntries), ".zip", (inPaneB ? entriesB : entries).map((e) => e.name));
    const dest = joinPath(dir, name);
    const n = pane.selectedEntries.length;
    try {
      const id = await startArchiveCompress(pane.selectedEntries.map((e) => e.path), dest, null);
      pendingArchiveOps.set(id, {
        dir,
        onSuccess: () => {
          if (!inPaneB) pendingSelectPath = dest;
          showNotice($t(n === 1 ? "notice.compressedToOne" : "notice.compressedToMany", { count: n, name }));
        },
        cancelledNotice: $t("notice.compressCancelled"),
        failedNotice: $t("notice.compressFailedTo", { name }),
      });
    } catch (e) {
      showNotice(String(e), true);
    }
  }

  /** Compress the selection via the queued archive path (CPE-1183/1184), letting `ext` pick the format:
   *  `.zip` (deflate, same bytes as `doCompress`) or `.tar.gz` (gzip tarball). Kept as a separate
   *  function from `doCompress` so its callers/behaviour stay distinguishable, even though both now
   *  queue through the same `start_archive_compress` command (dest's extension picks the format).
   *  `inPaneB` (CPE-1386): same routing as `doCompress` above. */
  async function doCompressAs(ext: ".zip" | ".tar.gz", inPaneB = false) {
    const pane = paneStateFor(inPaneB);
    const dir = inPaneB ? paneBPath : currentPath;
    if ((!inPaneB && (isHome || blockedInArchive())) || (inPaneB && dir === HOME) || pane.selectedEntries.length === 0) return;
    const name = uniqueNameWithExt(compressBaseName(pane.selectedEntries), ext, (inPaneB ? entriesB : entries).map((e) => e.name));
    const dest = joinPath(dir, name);
    const n = pane.selectedEntries.length;
    try {
      const id = await startArchiveCompress(pane.selectedEntries.map((e) => e.path), dest, null);
      pendingArchiveOps.set(id, {
        dir,
        onSuccess: () => {
          if (!inPaneB) pendingSelectPath = dest;
          showNotice($t(n === 1 ? "notice.compressedToOne" : "notice.compressedToMany", { count: n, name }));
        },
        cancelledNotice: $t("notice.compressCancelled"),
        failedNotice: $t("notice.compressFailedTo", { name }),
      });
    } catch (e) {
      showNotice(String(e), true);
    }
  }

  /** Compress the selection into a password-protected .zip (CPE-1182), queued (CPE-1184): collect the
   *  password via `PasswordPromptDialog`, then `startArchiveCompress` with it. An empty password
   *  re-prompts (the backend itself rejects one, but asking again beats a raw error notice). `inPaneB`
   *  (CPE-1386): the selection + target dir are resolved HERE, before the password dialog opens, and
   *  threaded through `promptForCompressPassword` — mirroring `promptForCompressPassword`'s doc comment. */
  async function doCompressWithPassword(inPaneB = false) {
    const pane = paneStateFor(inPaneB);
    const dir = inPaneB ? paneBPath : currentPath;
    if ((!inPaneB && (isHome || blockedInArchive())) || (inPaneB && dir === HOME) || pane.selectedEntries.length === 0) return;
    const name = uniqueNameWithExt(compressBaseName(pane.selectedEntries), ".zip", (inPaneB ? entriesB : entries).map((e) => e.name));
    const dest = joinPath(dir, name);
    const n = pane.selectedEntries.length;
    promptForCompressPassword(pane.selectedEntries, inPaneB, dir, dest, n, name, "");
  }

  /** `sel`/`inPaneB`/`dir` (CPE-1386): SNAPSHOT by `doCompressWithPassword` before this dialog opens —
   *  the password prompt can stay open for as long as the user takes to type one, so re-deriving the
   *  selection/pane from live state on submit (instead of the snapshot passed in) could otherwise let a
   *  pane switch retarget the compress onto the OTHER pane, mirroring `askDelete`'s snapshot reasoning. */
  function promptForCompressPassword(sel: DirEntry[], inPaneB: boolean, dir: string, dest: string, n: number, name: string, error: string) {
    passwordPrompt = {
      title: "Set a password",
      message: "Choose a password to protect this archive — you'll need it again to open the archive.",
      confirmLabel: "Compress",
      error,
      onSubmit: async (password) => {
        if (!password) {
          promptForCompressPassword(sel, inPaneB, dir, dest, n, name, "A password is required.");
          return;
        }
        try {
          const id = await startArchiveCompress(sel.map((e) => e.path), dest, password);
          passwordPrompt = null;
          pendingArchiveOps.set(id, {
            dir,
            onSuccess: () => {
              if (!inPaneB) pendingSelectPath = dest;
              showNotice($t(n === 1 ? "notice.compressedToPasswordOne" : "notice.compressedToPasswordMany", { count: n, name }));
            },
            cancelledNotice: $t("notice.compressCancelled"),
            failedNotice: $t("notice.compressFailedTo", { name }),
          });
        } catch (e) {
          showNotice(String(e), true);
          passwordPrompt = null;
        }
      },
    };
  }

  /** True when an archive error looks like it needs a password rather than being some other failure —
   *  the `zip` crate's own wording for both cases ("Password required to decrypt file" when none was
   *  given, "The password provided is incorrect" when the wrong one was) always contains "password"
   *  (CPE-1182). */
  function isPasswordError(e: unknown): boolean {
    return String(e).toLowerCase().includes("password");
  }

  /** Where "extract here" (and a locked archive's forced-extract fallback, see `enterArchive`) puts an
   *  archive's contents: a new subfolder of the TARGET folder, named after the archive and
   *  auto-numbered on collision (CPE-252/1182). `inPaneB` (CPE-1386): dedupes against pane B's own
   *  `entriesB` + lands inside `paneBPath` when the entry being extracted is pane B's own row. */
  function extractHereDest(entry: DirEntry, inPaneB = false): { dest: string; name: string } {
    const dir = inPaneB ? paneBPath : currentPath;
    const name = uniqueName(archiveBaseName(entry.name), (inPaneB ? entriesB : entries).map((e) => e.name));
    return { dest: joinPath(dir, name), name };
  }

  /** Try a plain extract queued through the transfer engine (CPE-1184: streamed progress + cancel
   *  instead of one blocking call); if the archive is AES-encrypted, `startArchiveExtract` rejects
   *  synchronously (it checks the password up front before queuing anything — see its doc comment), so
   *  this still prompts for the password and retries exactly like the old one-shot version did
   *  (CPE-1182). `onSuccess` runs once the queued run actually finishes, via the global
   *  `transfer://done` listener below — not inline here. `refreshDir` (CPE-1386) is the folder the
   *  listener refreshes on success (`pendingArchiveOps`' `dir`) — the pane's own folder for "extract
   *  here", or the picked destination for "extract to…" (see each caller). */
  async function extractWithPasswordFallback(entry: DirEntry, dest: string, refreshDir: string, onSuccess: () => void | Promise<void>) {
    try {
      const id = await startArchiveExtract(entry.path, dest, null);
      pendingArchiveOps.set(id, {
        onSuccess,
        dir: refreshDir,
        cancelledNotice: $t("notice.extractCancelled"),
        failedNotice: $t("notice.extractFailedName", { name: entry.name }),
      });
    } catch (e) {
      if (!isPasswordError(e)) {
        showNotice(String(e), true);
        return;
      }
      promptForExtractPassword(entry, dest, refreshDir, onSuccess);
    }
  }

  function promptForExtractPassword(
    entry: DirEntry,
    dest: string,
    refreshDir: string,
    onSuccess: () => void | Promise<void>,
    error = "",
  ) {
    passwordPrompt = {
      title: "Password required",
      message: `"${entry.name}" is password-protected — enter its password to extract it.`,
      confirmLabel: "Extract",
      error,
      onSubmit: async (password) => {
        try {
          const id = await startArchiveExtract(entry.path, dest, password);
          passwordPrompt = null;
          pendingArchiveOps.set(id, {
            onSuccess,
            dir: refreshDir,
            cancelledNotice: $t("notice.extractCancelled"),
            failedNotice: $t("notice.extractFailedName", { name: entry.name }),
          });
        } catch (e) {
          if (!isPasswordError(e)) {
            // A non-password failure (disk full, corrupt archive, permission denied, …) — surface the
            // real error and stop instead of misreporting it as a bad password (CPE-1186).
            passwordPrompt = null;
            showNotice(String(e), true);
            return;
          }
          // Wrong (or empty) password — re-prompt with the error line instead of dismissing.
          promptForExtractPassword(entry, dest, refreshDir, onSuccess, "Wrong password — try again.");
        }
      },
    };
  }

  /** Extract the selected archive into a new subfolder of the current folder
      (CPE-252). Named after the archive, auto-numbered to avoid collisions. Transparently prompts for a
      password when the archive is AES-encrypted (CPE-1182). Queued through the transfer engine so a
      large archive streams progress + stays cancellable (CPE-1184). `inPaneB` (CPE-1386): same pane
      routing as `doCompress` — `isHome`/`blockedInArchive()` only gate a pane-A extract; pane B is
      instead gated on `paneBPath === HOME`. */
  async function doExtract(inPaneB = false) {
    const pane = paneStateFor(inPaneB);
    const dir = inPaneB ? paneBPath : currentPath;
    if ((!inPaneB && (isHome || blockedInArchive())) || (inPaneB && dir === HOME)) return;
    const entry = pane.selectedEntries[0];
    if (pane.selectedEntries.length !== 1 || !entry || !isExtractable(entry)) return;
    const { dest, name } = extractHereDest(entry, inPaneB);
    await extractWithPasswordFallback(entry, dest, dir, () => {
      if (!inPaneB) pendingSelectPath = dest;
      showNotice($t("notice.extractedTo", { entry: entry.name, dest: name }));
    });
  }

  /** Extract the selected archive into a folder chosen from the native picker (CPE-1183), alongside the
   *  existing "extract here". Same password fallback as `doExtract`, same queue-routing (CPE-1184).
   *  `inPaneB` (CPE-1386): same pane routing as `doExtract`. The refresh target is the PICKED
   *  destination itself (not `dir`) — reused via `refreshBatchApplyTarget` in the `transfer://done`
   *  listener, so either pane showing that folder (not necessarily the one the menu was opened over)
   *  refreshes, matching the pre-CPE-1386 "only refresh if it lands in view" intent but extended to
   *  cover pane B too. */
  async function doExtractTo(inPaneB = false) {
    const pane = paneStateFor(inPaneB);
    const dir = inPaneB ? paneBPath : currentPath;
    if ((!inPaneB && (isHome || blockedInArchive())) || (inPaneB && dir === HOME)) return;
    const entry = pane.selectedEntries[0];
    if (pane.selectedEntries.length !== 1 || !entry || !isExtractable(entry)) return;
    let dest: string | string[] | null;
    try {
      dest = await openFolderDialog({
        directory: true,
        multiple: false,
        defaultPath: dir,
        title: `Extract "${entry.name}" to…`,
      });
    } catch {
      return; // dialog unavailable / errored — no-op
    }
    if (!dest || typeof dest !== "string") return; // cancelled
    const target = dest;
    await extractWithPasswordFallback(entry, target, target, () => {
      showNotice($t("notice.extractedTo", { entry: entry.name, dest: target }));
    });
  }

  // ---- Archive actions on the preview pane's action bar (CPE-1578, epic CPE-1568 slice 4) ----------
  // Pure UI wiring onto the EXISTING context-menu backend paths above: the single `<PreviewPane>` in
  // this file only ever shows pane A's own selection (see its `entry={...}` binding below), so these
  // three simply call the SAME `extractHereDest`/`extractWithPasswordFallback`/`archiveSafetyFor` core
  // `doExtract`/`doExtractTo`/`askArchiveSafety` use — entered with the previewed entry directly rather
  // than derived from `pane.selectedEntries[0]` — instead of a pane-selection-based wrapper. No
  // `inPaneB` branch: unlike the context menu (which can act on either pane), the preview pane has no
  // pane-B counterpart to route to.

  /** Extract the previewed archive here — the preview-pane counterpart to `doExtract` above. */
  async function extractPreviewHere(entry: DirEntry): Promise<void> {
    if (isHome || blockedInArchive() || !isExtractable(entry)) return;
    const { dest, name } = extractHereDest(entry);
    await extractWithPasswordFallback(entry, dest, currentPath, () => {
      pendingSelectPath = dest;
      showNotice($t("notice.extractedTo", { entry: entry.name, dest: name }));
    });
  }

  /** Extract the previewed archive to a picked folder — the preview-pane counterpart to `doExtractTo`
   *  above. */
  async function extractPreviewTo(entry: DirEntry): Promise<void> {
    if (isHome || blockedInArchive() || !isExtractable(entry)) return;
    let dest: string | string[] | null;
    try {
      dest = await openFolderDialog({
        directory: true,
        multiple: false,
        defaultPath: currentPath,
        title: `Extract "${entry.name}" to…`,
      });
    } catch {
      return; // dialog unavailable / errored — no-op
    }
    if (!dest || typeof dest !== "string") return; // cancelled
    const target = dest;
    await extractWithPasswordFallback(entry, target, target, () => {
      showNotice($t("notice.extractedTo", { entry: entry.name, dest: target }));
    });
  }

  /** Check the previewed archive's safety — the preview-pane counterpart to `askArchiveSafety` above:
   *  opens the SAME `ArchiveSafetyDialog` (it owns the `analyze_archive_safety` call + rendering). */
  function checkPreviewArchiveSafety(entry: DirEntry): void {
    if (isHome || archive || !isArchiveSafetyEligible(entry)) return;
    archiveSafetyFor = entry.path;
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
    // CPE-1112 rework: an overlay row still carries its REAL reconstructed path in `[data-drop-path]`
    // (`folderUnderCursor` below reads it straight off the DOM), so without this an OS file-drop during
    // Replay mode could silently import into a folder the user only meant to look at in the past — the
    // exact same "read-only means read-only" contract the other guards in this file enforce.
    if (replayOverlayEntries !== null) {
      showNotice($t("replay.blockedNotice"), true);
      return;
    }
    // CPE-1368: an archive browse-view is read-only, but its folder rows still render `[data-drop-path]`
    // with a SYNTHETIC in-zip path (e.g. "docs") — so `folderUnderCursor` returns that non-empty string and
    // defeats the `archive ? ""` fallback below, letting an OS drop copy into a virtual path the backend
    // resolves to some unexpected on-disk location. Guard it up front, exactly like Replay mode above (the
    // internal-drag path is already blocked via `canDrag={!archive}`; this closes the OS drop-in hole).
    if (archive) {
      showNotice($t("archive.blockedImportNotice"), true);
      return;
    }
    const dest = folderUnderCursor(pos) || (isHome || smartFolder || structuredSearch ? "" : currentPath);
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
      sameVolume = await commands.sameVolume(paths[0], dest).catch(() => false);
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
      const results = await commands.moveEntries(paths, dest);
      reportResults(results, "move");
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
      await refreshDropSourcePane(paths);
    } catch (e) {
      showNotice(String(e), true);
    }
  }

  /** After a move via `dropInto`, refresh whichever pane's folder actually LOST the dragged items
   *  (CPE-1371: pane B can now both originate and receive drags, so the refresh can no longer just
   *  hard-code pane A). A `FileList` drop always lands ON a folder ROW inside the receiving pane's own
   *  listing — never on that pane's own root — so the receiving pane's top-level listing is unaffected
   *  by the move; only the pane the items came FROM needs to reload. The source is identified by the
   *  dragged paths' parent directory: if it matches pane B's folder, refresh pane B; otherwise (pane A,
   *  single-pane mode, or a Sidebar-driven drop whose source isn't a currently-open pane at all) fall
   *  back to refreshing pane A, matching pre-CPE-1371 behavior. */
  async function refreshDropSourcePane(paths: string[]) {
    const parent = normalizePath(parentOfPath(paths[0] ?? ""));
    const matchesB = !!(dualPane && paneBPath && parent === normalizePath(paneBPath));
    const matchesA = parent === normalizePath(currentPath);
    // Both can match at once — a common commander pattern is mirroring the SAME folder into both panes
    // (compare/sort one dir two ways). A mutually-exclusive if/else here left the non-matched pane
    // rendering a GHOST row for a file the move had already removed (CPE-1371 review/UAT: reproduced
    // with `paneBPath === currentPath`). Refresh whichever pane(s) actually show the source folder,
    // falling back to pane A when neither matches (Sidebar-driven drop whose source isn't a currently
    // open pane at all), matching pre-fix behavior.
    if (matchesB) await explorerPaneB?.loadListing(paneBPath, false);
    if (matchesA || !matchesB) await loadPath(currentPath);
  }

  /** After a clipboard cut+paste MOVE (CPE-1380), refresh whichever pane(s) show either side of the
   *  move: the pane(s) whose current folder is the moved items' SOURCE parent (their rows disappear) —
   *  same source-matching logic as `refreshDropSourcePane` — PLUS the paste's own DESTINATION pane
   *  (`destInPaneB`), whose rows newly appear. This differs from `refreshDropSourcePane`: a drag-drop
   *  always lands ON a child row inside the receiving pane, so only the source needs reloading; a paste's
   *  destination IS the target pane's own current-folder listing, so it must reload too. Both source and
   *  destination pane(s) can match at once (e.g. both panes mirror the same folder) — refresh every pane
   *  that matches either side so no ghost/missing row is left behind, mirroring CPE-1371's reasoning. */
  async function refreshPasteAffectedPanes(sources: string[], destInPaneB: boolean) {
    const srcParent = normalizePath(parentOfPath(sources[0] ?? ""));
    const matchesB = !!(dualPane && paneBPath && srcParent === normalizePath(paneBPath));
    const matchesA = srcParent === normalizePath(currentPath);
    const refreshB = matchesB || destInPaneB;
    const refreshA = matchesA || !destInPaneB;
    if (refreshB) await explorerPaneB?.loadListing(paneBPath, false);
    if (refreshA) await loadPath(currentPath);
  }

  /** `inPaneBOverride` (CPE-1377): a context-menu-invoked delete must target the pane the menu was
   *  opened OVER (`ctx?.inPaneB`), not the live `activePane` — right-clicking pane B doesn't focus it
   *  (only a plain click does), so the default `dualPane && activePane === 1` would silently target
   *  whichever pane last had focus instead of the one actually right-clicked. The keyboard Delete path
   *  (Delete/Shift+Delete in `handleKeydown`) has no menu-open-time pane to snapshot, so it keeps using
   *  the default (live active pane), unchanged from CPE-1370. */
  function askDelete(permanent: boolean, inPaneBOverride?: boolean) {
    // CPE-1370 review (data-loss fix): SNAPSHOT the target pane + paths right now via
    // `snapshotConfirmTarget` — the confirm dialog stays open for an arbitrary amount of time, during
    // which `activePane` can still change underneath it (Tab still reaches `handleKeydown` while
    // `confirm` is showing). Without the snapshot, a user could select pane A's files, Shift+Delete, see
    // pane A's files in the confirm message, Tab to pane B, click "Delete permanently" — and PERMANENTLY
    // delete pane B's files, which were never shown or confirmed. `target` is threaded through the
    // `onYes` closure into `doDelete` below, which must NOT re-derive it from live state.
    // `archive`/`smartFolder`/`structuredSearch`/Replay mode are all pane-A-only virtual views (pane B —
    // when it's the active pane — always shows a plain real folder), so `blockedInArchive()` only
    // applies when we're actually targeting pane A.
    const inPaneB = inPaneBOverride ?? (dualPane && activePane === 1);
    const pane = paneStateFor(inPaneB);
    if ((!inPaneB && blockedInArchive()) || pane.selectedEntries.length === 0) return;
    const target = snapshotConfirmTarget(inPaneB, pane.selectedEntries);
    const n = pane.selectedEntries.length;
    const what = n === 1 ? `"${pane.selectedEntries[0].name}"` : `${n} items`;

    if (!permanent) {
      // Recycle bin is recoverable, so no modal — just do it and say so. (Nothing can change `pane`
      // between here and `doDelete` since this path is fully synchronous — no `await` in between — but
      // it still goes through the same snapshot for a single, consistent code path.)
      doDelete(false, target);
      return;
    }
    confirm = {
      title: "Delete permanently?",
      message: `${what} will be permanently deleted. This cannot be undone and does not go to the Recycle Bin.`,
      label: "Delete permanently",
      // The `true` for `confirmed` (CPE-1651) is set HERE and nowhere else: this closure only runs when
      // the user actually pressed "Delete permanently" on the dialog above. It is deliberately a
      // separate argument from `permanent` — reusing the intent flag as the consent flag is the exact
      // bug CPE-1646 was filed against.
      onYes: () => doDelete(true, target, true),
    };
  }

  /** `target` is the snapshot `askDelete` captured at confirm-open time (or immediately, for the
   *  non-permanent no-modal path) via `snapshotConfirmTarget` — deliberately a parameter, NOT
   *  re-derived from live `activePane`/selection state here, so a pane switch that happens while a
   *  confirm dialog was open can never retarget an already-confirmed delete onto a different pane's
   *  files (CPE-1370 review).
   *
   *  `confirmed` (CPE-1651) is the user's CONSENT, tracked separately from `permanent` (the caller's
   *  INTENT) and defaulting to `false`: the backend now refuses an unconsented `delete_permanent`
   *  outright, so a future call site that forgets the confirm dialog fails loudly instead of quietly
   *  destroying files. Only `askDelete`'s confirm-dialog `onYes` may pass `true`. */
  async function doDelete(permanent: boolean, target: ConfirmTarget, confirmed = false) {
    confirm = null;
    const { inPaneB, paths } = target;
    if (paths.length === 0) return;
    try {
      const results = permanent
        ? unwrap(await commands.deletePermanent(paths, confirmed))
        : await commands.deleteToTrash(paths);
      reportResults(results, permanent ? "deletePermanent" : "moveToBin");

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
      if (inPaneB) {
        if (paneBPath) await explorerPaneB?.loadListing(paneBPath, false);
      } else {
        await loadPath(currentPath);
      }
    } catch (e) {
      showNotice(String(e), true);
    }
  }

  /** Open the "Securely delete…" confirm dialog (CPE-1240, epic CPE-738) for the current selection.
   *  Guarded the same way as `askDelete`, plus a folder check: the shred engine overwrites a single
   *  file's bytes then unlinks it — it isn't recursive — so a folder in the selection is refused with a
   *  clear notice rather than silently skipped mid-operation. `ContextMenu`'s `shreddable` prop already
   *  hides the row for a folder-containing selection; this is the belt-and-braces check for any other
   *  caller (defense in depth, same reasoning as `blockedInArchive`'s re-checks elsewhere). `inPaneB`
   *  (CPE-1386): a context-menu invocation targets whichever pane the menu was opened OVER — the
   *  DESTRUCTIVE, non-recoverable target paths + pane are SNAPSHOT into `shredConfirmFor` right now,
   *  before the confirm dialog opens, mirroring `askDelete`'s `snapshotConfirmTarget` (CPE-1370) — this
   *  is the one archive/vault op that deletes real files, so it gets the same treatment as a permanent
   *  delete: never re-derive the target from live `activePane` after the dialog is showing. */
  function askShred(inPaneB = false) {
    const pane = paneStateFor(inPaneB);
    const dir = inPaneB ? paneBPath : currentPath;
    if ((!inPaneB && blockedInArchive()) || (inPaneB && dir === HOME) || pane.selectedEntries.length === 0) return;
    if (pane.selectedEntries.some((e) => e.is_dir)) {
      showNotice($t("ctx.shredFoldersNotAllowed"), true);
      return;
    }
    const n = pane.selectedEntries.length;
    shredConfirmFor = {
      paths: pane.selectedEntries.map((e) => e.path),
      what: n === 1 ? `"${pane.selectedEntries[0].name}"` : `${n} items`,
      inPaneB,
      dir,
    };
  }

  /** `ShredConfirmDialog`'s `done` handler: per-path shred results are `OpResult`-shaped (plus extra
   *  pass/byte fields this summary doesn't need) — `reportResults` already handles a partial-failure
   *  batch honestly. Refresh whichever pane(s) show the SNAPSHOT `dir` `askShred` captured (CPE-1386,
   *  same `refreshBatchApplyTarget` reuse as `onVaultCreated`) so shredded files disappear from view —
   *  not live `currentPath`/`paneBPath`, so a pane switch while the (irreversible) confirm was open can't
   *  retarget the refresh either. */
  async function onShredDone(results: OpResult[]) {
    const target = shredConfirmFor;
    shredConfirmFor = null;
    reportResults(results, "deleteSecure");
    if (target) await refreshBatchApplyTarget(target.dir);
  }

  /** `ShredConfirmDialog`'s `error` handler — same reasoning as `onNewLinkError`/`onLinkRepairError`:
   *  surface via the toast; the dialog stays open (it shows the same message inline) so the user can
   *  retry or cancel instead of losing the confirm state. */
  function onShredError(message: string) {
    showNotice(message, true);
  }

  /** `entries` defaults to pane A's selection (CPE-1377) — `runAction`'s "properties" case passes the
   *  context-menu's pane-aware selection so Properties opened over a pane-B row describes pane B's
   *  item, not whatever happens to be selected in pane A. */
  function openProperties(entries: DirEntry[] = selectedEntries) {
    if (entries.length === 0) return;
    propsFor = entries;
  }

  /** Properties for the CURRENT folder (CPE-1153) — the empty-area menu's Properties row, when nothing
   *  is selected. Home is an abstract view with no single path, so it's skipped there. The dialog
   *  re-fetches real info from the backend via the path, so a synthesized folder entry is enough.
   *  `path` defaults to pane A's `currentPath` (CPE-1377) — `runAction`'s "properties-folder" case
   *  passes `paneBPath` when the empty-area menu was opened over pane B. */
  function openFolderProperties(path: string = currentPath) {
    if (path === HOME) return;
    const name = splitPath(path).at(-1)?.name ?? path;
    propsFor = [{ name, path, is_dir: true, size: 0, modified: null, extension: "", hidden: false, is_symlink: false }];
  }

  function openMetadataStudio() {
    if (selectedEntries.length === 0) return;
    studioFor = selectedEntries;
  }

  /** Select every visible entry (CPE-605). A named function so the palette command can reference it
      without textually assigning `selection` inside the reactive `paletteCommands` block — that would
      make Svelte see a write and form a selection ⇄ selectedEntries cycle. */
  function selectAllVisible() {
    // CPE-1373: keep the current lead (scroll position) instead of jumping to the last row.
    selection = selectAll(visible.length, selection.lead);
  }

  /** Run the selected executable normally (CPE-241) — same shell open as double-click. */
  async function executeSelected() {
    const entry = selectedEntries[0];
    if (!entry || !isExecutable(entry)) return;
    try {
      unwrap(await commands.openExternal(entry.path));
    } catch {
      showNotice($t("notice.runFailed", { name: entry.name }), true);
    }
  }

  /** Run the selected executable elevated (UAC prompt on Windows) (CPE-241). */
  async function executeAsAdmin() {
    const entry = selectedEntries[0];
    if (!entry || !isExecutable(entry)) return;
    try {
      unwrap(await commands.runAsAdmin(entry.path));
    } catch {
      showNotice($t("notice.runAsAdminFailed", { name: entry.name }), true);
    }
  }

  // ---- context menu / command dispatch ----
  function runAction(action: string) {
    // Run a saved macro from the "Run macro ▸" context-menu submenu (CPE-1191). `slice(6)` (not
    // `split(":")`) so a macro name that itself contains a colon still round-trips intact.
    if (action.startsWith("macro:")) {
      void startMacro(action.slice(6));
      return;
    }
    // Run a user-defined command from the Context menu's "Run command ▸" submenu or a Toolbar button
    // (CPE-1577, epic CPE-711) — same confirm-before-launch gate the Palette surface already uses.
    // `slice(3)` (not `split(":")`) so an id embedding a colon still round-trips intact.
    if (action.startsWith("uc:")) {
      runUserCommandById(action.slice(3));
      return;
    }
    // CPE-1377: which pane the OPEN menu was opened over (`ctx?.inPaneB`) — NOT the live `activePane`,
    // since a right-click doesn't focus a pane (see the `ctx` declaration's comment). Read once, up
    // front, synchronously — before any `await` — exactly like `askDelete` already snapshots its own
    // target, so a case below can't read a stale `ctx` after `ContextMenu`'s `close` event nulls it.
    // `CommandBar`'s toolbar actions and the empty-space palette also route through here with `ctx`
    // null, which correctly resolves to pane A (the toolbar always acts on pane A today).
    const inPaneB = ctx?.inPaneB ?? false;
    const pane = paneStateFor(inPaneB);
    // Typed New ▸ file types (CPE-1161) carry the extension as `new-file:<ext>` / `new-file-in:<ext>`
    // / `drive-new-file:<ext>`, resolved back to a spec here so one list drives all three menus. The
    // target folder mirrors the plain new-file rules: current folder / the clicked folder / drive root.
    if (action.includes(":")) {
      const [verb, ext] = action.split(":");
      const spec = ext ? NEW_FILE_TYPE_BY_EXT[ext] : undefined;
      if (spec && (verb === "new-file" || verb === "new-file-in" || verb === "drive-new-file" || verb === "home-new-file")) {
        if (verb === "new-file-in") {
          if (pane.selectedEntries[0]?.is_dir) newFile(pane.selectedEntries[0].path, spec, inPaneB);
        } else if (verb === "drive-new-file") {
          if (driveCtxPath) newFile(driveCtxPath, spec);
        } else if (verb === "home-new-file") {
          if (homeCtxIsDir && homeCtxPath) newFile(homeCtxPath, spec);
        } else {
          newFile(inPaneB ? (paneBPath === HOME ? currentPath : paneBPath) : currentPath, spec, inPaneB);
        }
        return;
      }
    }
    switch (action) {
      // Command Palette discoverable entry points (CPE-1164) — the toolbar button and the empty-area
      // context-menu row both dispatch this; reuse the same open path as Ctrl+Shift+P.
      case "command-palette": paletteOpen = true; break;
      case "open": if (pane.selectedEntries[0]) pane.openEntry(pane.selectedEntries[0]); break;
      case "execute": executeSelected(); break;
      case "execute-admin": executeAsAdmin(); break;
      case "open-new-tab": if (pane.selectedEntries[0]) openInNewTab(pane.selectedEntries[0]); break;
      // Cut/copy/paste/duplicate/batch-rename/batch-media/copy-to/move-to all route via `inPaneB`
      // (CPE-1380 for cut/copy/paste; CPE-1384 for the rest) — a context-menu invocation targets
      // whichever pane the menu was opened OVER, same as every other pane-routed case in this switch.
      case "cut": doCut(inPaneB); break;
      case "copy": doCopy(inPaneB); break;
      case "add-drop-stack": doAddToDropStack(inPaneB); break;
      case "paste": doPaste(inPaneB); break;
      case "duplicate": doDuplicate(inPaneB); break;
      // Compress/extract/archive-safety/shred/vault-create are pane-routed (CPE-1386, extending
      // CPE-1384's reasoning to the archive/vault family): a context-menu invocation targets whichever
      // pane the menu was opened OVER, same as cut/copy/paste/duplicate above — `<ContextMenu>`'s
      // `compressible`/`extractable`/`archiveSafetyEligible`/`shreddable`/`vaultable` props are un-gated
      // for pane B below (mirroring `copyMoveEligible`'s `paneBPath !== HOME` guard). `compare` stays
      // pane-A-only (out of this ticket's scope) — `comparable` is still forced off for a pane-B menu.
      case "compare": compareFiles(); break;
      case "compress": doCompress(inPaneB); break;
      case "compress-targz": doCompressAs(".tar.gz", inPaneB); break;
      case "compress-password": doCompressWithPassword(inPaneB); break;
      case "extract": doExtract(inPaneB); break;
      case "extract-to": doExtractTo(inPaneB); break;
      case "archive-safety": askArchiveSafety(inPaneB); break;
      case "copy-path": doCopyPath(pane.selectedEntries); break;
      case "copy-name": doCopyName(pane.selectedEntries); break;
      case "reveal": revealInExplorer(pane.selectedEntries, inPaneB ? (paneBPath === HOME ? "" : paneBPath) : (isHome ? "" : currentPath)); break;
      case "terminal": openTerminal(currentPath); break;
      case "terminal-folder": if (pane.selectedEntries[0]?.is_dir) openTerminal(pane.selectedEntries[0].path); break;
      case "pin": togglePinSelected(pane.selectedEntries); break;
      case "favorite": toggleFavoriteSelected(pane.selectedEntries); break;
      case "open-in-console": openSelectionInConsole(); break;
      case "copy-to": copyMoveToFolder(false, inPaneB); break;
      case "move-to": copyMoveToFolder(true, inPaneB); break;
      case "open-folder-in-console": if (!isHome && !archive) openAiConsole({ cwd: currentPath }); break;
      case "rename": if (pane.selectedEntries.length === 1) beginRename(pane.selectedEntries[0], inPaneB); break;
      case "batch-rename": beginBatchRename(inPaneB); break;
      case "batch-media": beginBatchMedia(inPaneB); break;
      case "delete": askDelete(false, inPaneB); break;
      case "shred": askShred(inPaneB); break;
      case "vault-create": askVaultCreate(inPaneB); break;
      // Certificate management (CPE-1424, epic CPE-1417) — same pane-routed reasoning as vault-create
      // above: a context-menu invocation targets whichever pane the menu was opened OVER.
      case "cert-create-here": askCertCreate(inPaneB); break;
      case "cert-issue-from-csr": if (pane.selectedEntries[0]) askCertSign(inPaneB, { csrPath: pane.selectedEntries[0].path }); break;
      case "cert-sign-as-ca": if (pane.selectedEntries[0]) askCertSign(inPaneB, { caCertPath: pane.selectedEntries[0].path }); break;
      case "cert-inspect": inspectCryptoFile(inPaneB); break;
      case "jwt-inspect": inspectCryptoFile(inPaneB); break;
      // File split/join (CPE-1509, parent CPE-1491) — same pane-routed reasoning as cert-* above.
      case "split-file": askSplitFile(inPaneB); break;
      case "join-parts": askJoinParts(inPaneB); break;
      case "properties": openProperties(pane.selectedEntries); break;
      case "metadataStudio": openMetadataStudio(); break;
      case "tags": if (pane.selectedEntries.length >= 1) tagEditorFor = [...pane.selectedEntries]; break;
      // CPE-1377 review: route the empty-area create to whichever pane the menu was opened over — the
      // paneBPath===HOME guard mirrors "properties-folder"/"reveal" above (Home has no real path to
      // create in; falls back to pane A's currentPath in that edge case, matching those siblings).
      case "new-folder": newFolder(inPaneB ? (paneBPath === HOME ? currentPath : paneBPath) : currentPath, inPaneB); break;
      case "new-file": newFile(inPaneB ? (paneBPath === HOME ? currentPath : paneBPath) : currentPath, undefined, inPaneB); break;
      // New Link… (CPE-1207) — empty-area menu + command palette only, always targets currentPath.
      case "new-link": newLinkDialogFor = currentPath; break;
      // Repair link… (CPE-1209) — offered only when ContextMenu's `linkBroken` gated it on.
      case "repair-link": if (selectedEntries.length === 1) repairLinkFor = selectedEntries[0]; break;
      // New ▸ from a folder right-click (CPE-1156) — create INSIDE the clicked folder (its own path).
      // Pane-routed (CPE-1377 review) — the clicked folder can be pane B's (its own on:rowContext), so
      // the create + post-create inline rename must land there, not pane A.
      case "new-folder-in": if (pane.selectedEntries[0]?.is_dir) newFolder(pane.selectedEntries[0].path, inPaneB); break;
      case "new-file-in": if (pane.selectedEntries[0]?.is_dir) newFile(pane.selectedEntries[0].path, undefined, inPaneB); break;
      // Drive/disk menu (CPE-1158) — every action targets the drive ROOT (`driveCtxPath`), so it works
      // the same from a Home tile and a sidebar row. New reuses CPE-1156's create-in-target path.
      // "drive-open" navigates the pane the drive tile lives in (inPaneB from a pane-B Home tile; the
      // shared Sidebar's drive rows always resolve to pane A, unchanged).
      case "drive-open": if (driveCtxPath) { if (inPaneB) void navigateB(driveCtxPath); else navigate(driveCtxPath); } break;
      case "drive-new-folder": if (driveCtxPath) newFolder(driveCtxPath); break;
      case "drive-new-file": if (driveCtxPath) newFile(driveCtxPath); break;
      case "drive-copy-path": copyDrivePath(); break;
      case "drive-terminal": openDriveTerminal(); break;
      case "drive-properties": openDriveProperties(); break;
      case "drive-eject": if (driveCtxPath) ejectDrive(driveCtxPath, driveCtxName || driveCtxPath); break;
      // Home row menu (CPE-1162) — every action targets `homeCtxPath` (the clicked row), independent of
      // any FileList selection. `home-delete` trashes the real file; `home-remove` prunes only the list
      // pointer — deliberately two different verbs so the menu can keep them unmistakably distinct.
      // Home's underlying stores (pins/favorites/recents/…) are shared across both panes (CPE-1378), so
      // these need no pane-aware routing beyond `homeCtxPath` itself, already set by `onHomeItemContext`.
      case "home-open": openHomeItem(); break;
      case "home-open-new-tab": if (homeCtxIsDir && homeCtxPath) openInNewTab(homeCtxEntry()); break;
      case "home-reveal": revealHomeItem(); break;
      case "home-copy": copyHomeItem(); break;
      case "home-copy-path": copyHomeItemPath(); break;
      case "home-rename": renameHomeItem(); break;
      case "home-new-folder": if (homeCtxIsDir && homeCtxPath) newFolder(homeCtxPath); break;
      case "home-new-file": if (homeCtxIsDir && homeCtxPath) newFile(homeCtxPath); break;
      case "home-properties": openHomeItemProperties(); break;
      case "home-delete": deleteHomeItem(); break;
      case "home-favorite": favoriteHomeItem(); break;
      case "home-pin": pinHomeItem(); break;
      case "home-remove": removeHomePointer(homeCtxPath, homeCtxView); break;
      case "home-clear": recents = []; settings.saveRecents(recents); break;
      // Shared row menu (CPE-1163): Disconnect a mapped drive, or Remove a user-added location.
      case "share-disconnect": disconnectShare(); break;
      case "share-remove": removeNetworkLocation(homeCtxPath); break;
      // CPE-1373: pass the current lead through so bulk selections keep the scroll position instead of
      // yanking the viewport to the last/max-index row. CPE-1377: routed through `pane` so a select-all/
      // invert/same-type triggered from a pane-B context menu acts on pane B's own selection.
      case "select-all": pane.setSelection(selectAll(pane.visible.length, pane.selection.lead)); break;
      case "invert-selection": pane.setSelection(invertSelection(pane.selection, pane.visible.length, pane.selection.lead)); break;
      case "select-pattern": patternSelectOpen = true; break;
      case "color-rules": colorRulesOpen = true; break;
      case "select-type": {
        const e = pane.selectedEntries[0];
        if (e && !e.is_dir) pane.setSelection(selectIndices(sameTypeIndices(pane.visible, e.extension), pane.selection.lead));
        break;
      }
      case "refresh":
        if (inPaneB) { if (paneBPath) void explorerPaneB?.loadListing(paneBPath, false); }
        else refresh();
        break;
      case "undo": undo(); break;
      case "properties-folder": openFolderProperties(inPaneB ? paneBPath : currentPath); break;
      // View / Sort submenus (CPE-1153) — drive the SAME view/sortKey/sortDir state the toolbar and
      // column headers use (single source of truth), and persist exactly as those paths do.
      case "view:details": view = "details"; settings.saveView(view); break;
      case "view:list": view = "list"; settings.saveView(view); break;
      case "view:icons": view = "icons"; settings.saveView(view); break;
      case "view:gallery": view = "gallery"; settings.saveView(view); break;
      case "sort:name": sortKey = "name"; settings.saveSortKey(sortKey); break;
      case "sort:modified": sortKey = "modified"; settings.saveSortKey(sortKey); break;
      case "sort:type": sortKey = "type"; settings.saveSortKey(sortKey); break;
      case "sort:size": sortKey = "size"; settings.saveSortKey(sortKey); break;
      case "sortdir:asc": sortDir = "asc"; settings.saveSortDir(sortDir); break;
      case "sortdir:desc": sortDir = "desc"; settings.saveSortDir(sortDir); break;
      case "help-docs": openDocs(currentSection()); break;
    }
  }

  /** `inPaneB` (CPE-1377): pane B's `<ExplorerPane>` passes `true` so a pane-B right-click selects
   *  within `selectionB`/`visibleB` (never pane A's) and records which pane the menu is FOR on `ctx`,
   *  independent of live `activePane` — see the `ctx` declaration's comment. */
  async function onRowContext(e: { x: number; y: number; index: number }, inPaneB = false) {
    // Right-clicking an unselected row selects it first, as Explorer does.
    const sel = inPaneB ? selectionB : selection;
    const vis = inPaneB ? visibleB : visible;
    if (!sel.indices.has(e.index)) {
      if (inPaneB) selectionB = selectOnly(e.index);
      else selection = selectOnly(e.index);
    }
    ctx = { x: e.x, y: e.y, target: "item", inPaneB };
    ctxLinkBroken = false; // reset before the async check below settles (CPE-1209)
    // `vis[e.index]` (not `selectedEntries`/`selectedEntriesB`) — the selection was just reassigned
    // above and the `$:` derived selected-entries list hasn't recomputed yet in this same tick.
    const entry = vis[e.index];
    if (entry?.is_symlink) {
      try {
        const status = await commands.linkStatus(entry.path);
        // Guard against a stale write: only apply if this item's menu is still the one open.
        const curVis = inPaneB ? visibleB : visible;
        if (ctx?.target === "item" && curVis[e.index]?.path === entry.path) ctxLinkBroken = status.broken;
      } catch {
        // Never block the menu on a failed check — leave it false (Repair row simply doesn't show).
      }
    }
  }

  /** Open the drive/disk context menu (CPE-1158) for a Home drive tile or a sidebar drive row. The
   *  menu's actions all target `driveCtxPath` (the drive ROOT), so New creates at the root and
   *  Properties describes the root — independent of the FileList selection (Home has none).
   *  `inPaneB` (CPE-1377): true only when pane B's own `<ExplorerPane>` (its Home drive tiles) fired
   *  this — the shared Sidebar's drive rows always pass the default `false` (pane A), unchanged. */
  function onDriveContext(e: { x: number; y: number; path: string; name: string }, inPaneB = false) {
    driveCtxPath = e.path;
    driveCtxName = e.name;
    ctx = { x: e.x, y: e.y, target: "drive", inPaneB };
  }

  /** Copy the drive root path to the OS clipboard (CPE-1158 drive menu). */
  async function copyDrivePath() {
    if (!driveCtxPath) return;
    try {
      await navigator.clipboard.writeText(driveCtxPath);
      showNotice($t("notice.copiedPath"));
    } catch {
      showNotice($t("notice.copyPathFailed"), true);
    }
  }

  /** Open a terminal at the drive root (CPE-1158). Unlike `openTerminal`, this has no `isHome` guard —
   *  the drive menu is reachable FROM Home, and a drive root is a real, terminal-worthy path. */
  async function openDriveTerminal() {
    if (!driveCtxPath) return;
    try {
      unwrap(await commands.openTerminal(driveCtxPath));
    } catch {
      showNotice($t("notice.openTerminalFailed"), true);
    }
  }

  /** Properties for the drive root (CPE-1158) — a synthesized folder entry; the dialog re-fetches the
   *  real volume info from the path, mirroring `openFolderProperties`. */
  function openDriveProperties() {
    if (!driveCtxPath) return;
    propsFor = [{ name: driveCtxName || driveCtxPath, path: driveCtxPath, is_dir: true, size: 0, modified: null, extension: "", hidden: false, is_symlink: false }];
  }

  // ---- Home row context menu (CPE-1162) ---------------------------------------------------------
  // The Home Recent/Favorites/Folders lists have no `<FileList>`/selection, so — exactly like the drive
  // tile (CPE-1158) — a right-clicked row carries its own {path,is_dir,view} up here; every `home-*`
  // action targets `homeCtxPath`. This is what keeps the two ideas of "remove" distinct: `home-delete`
  // trashes the real file on disk, while `home-remove` only prunes the list ENTRY (favorite/recent
  // pointer). Shared is deliberately not a `view` value yet — the menu machinery is view-agnostic so it
  // plugs in once Shared's data source is decided (CPE-1163).

  /** Derive a leaf name from a path (last non-empty \ or / segment). */
  function leafName(path: string): string {
    return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
  }

  /** The clicked home item as a synthesized `DirEntry` for the dialogs/ops that want one (Properties,
   *  Open-in-new-tab, delete). The size/modified/extension are placeholders the backend re-derives. */
  function homeCtxEntry(): DirEntry {
    const name = leafName(homeCtxPath);
    const dot = name.lastIndexOf(".");
    return {
      name,
      path: homeCtxPath,
      is_dir: homeCtxIsDir,
      size: 0,
      modified: null,
      extension: !homeCtxIsDir && dot > 0 ? name.slice(dot + 1).toLowerCase() : "",
      hidden: false,
      is_symlink: false,
    };
  }

  /** Open the "home-item" menu for a Home row (CPE-1162). Stores the target, opens the menu, then fires
   *  a best-effort existence check (reusing `entries_for_paths`, the same stat-a-path command Home's
   *  preview uses) to mark it stale — a missing target disables the on-disk rows but keeps Remove live. */
  function onHomeItemContext(e: { x: number; y: number; path: string; is_dir: boolean; view: "recent" | "favorites" | "folders" | "shared"; kind?: string }, inPaneB = false) {
    homeCtxPath = e.path;
    homeCtxName = leafName(e.path);
    homeCtxIsDir = e.is_dir;
    homeCtxView = e.view;
    homeCtxKind = e.kind ?? "";
    homeCtxStale = false; // optimistic; the async check below flips it if the target is gone
    ctx = { x: e.x, y: e.y, target: "home-item", inPaneB };
    // Shared rows deliberately skip the stat-based stale check (CPE-1163): statting a dead/offline
    // network path could stall, and an unreachable share must degrade gracefully — Open surfaces its
    // own error, while Remove/Disconnect stay live regardless. Local rows keep the freshness check.
    if (e.view !== "shared") void checkHomeCtxStale(e.path);
  }

  /** (Re)load the Home "Shared" tab (CPE-1163): the OS-enumerated network drives merged with the
   *  user's added locations. Pull-only — called when the Shared tab is opened or after add/remove,
   *  never on a timer. Time-bounded in the backend, so an offline server can't hang this. */
  async function loadShared(): Promise<void> {
    sharedLoading = true;
    try {
      // `?? []` guards a backend/test-double that hands back `null` (CPE-1513: this now also runs at
      // startup for the sidebar's Network section, not just when the Home Shared tab pull-loads it).
      shared = (await commands.listNetworkShares(networkLocations)) ?? [];
    } catch {
      // A failed enumeration degrades to just the user-added locations, never a crash.
      shared = networkLocations
        .map((p) => ({ name: leafName(p) || p, path: p, kind: "user" }))
        .filter((s) => s.path.trim().length > 0);
    } finally {
      sharedLoading = false;
    }
  }

  /** Load the sidebar's "Discovered on your network" tier: the Windows-native WNet enumeration of the
   *  same network neighborhood Explorer shows (CPE-1519) MERGED with the cross-platform mDNS/DNS-SD
   *  browse (CPE-1523) — the two run in parallel (neither backend call waits on the other) and are
   *  combined + deduplicated by `mergeDiscovered` (pure, unit-tested in `network.test.ts`).
   *  `discover_network_windows` is deliberately excluded from the typed specta bindings (its behavior is
   *  Windows-only, like `set_file_attribute` — see CLAUDE.md), so it's called via the raw `invoke`, not
   *  `commands.*`; `discover_network_mdns` behaves identically on every OS, so it IS a typed
   *  `commands.discoverNetworkMdns()` call. Both are bounded (~6s) backend-side, and each degrades to `[]`
   *  independently on failure (a dead/absent mDNS daemon never blanks out the WNet rows, and vice versa)
   *  — so a total failure of both simply leaves the tier empty, exactly like it doesn't grow tier 2 rows
   *  when there are no OS shares. */
  async function loadDiscovered(): Promise<void> {
    const [windows, mdns] = await Promise.all([
      invoke<NetShare[]>("discover_network_windows").catch((e) => {
        console.debug("discover_network_windows failed:", e);
        return [] as NetShare[];
      }),
      commands.discoverNetworkMdns().catch((e) => {
        console.debug("discover_network_mdns failed:", e);
        return [] as NetShare[];
      }),
    ]);
    discoveredShares = mergeDiscovered(windows ?? [], mdns ?? []);
  }

  /** Add a user-typed network location (CPE-1163): persist it, then reload so it appears (the backend
   *  validates + dedupes against enumerated drives). An unparseable address simply won't list. */
  function addNetworkLocation(path: string): void {
    const next = settings.addNetworkLocation(networkLocations, path);
    if (next === networkLocations) return;
    networkLocations = next;
    settings.saveNetworkLocations(networkLocations);
    showNotice($t("notice.networkLocationAdded", { path: path.trim() }));
    void loadShared();
  }

  /** Remove a user-added network location (CPE-1163) — prunes the persisted list + reloads. Never
   *  touches an OS-enumerated mapped drive (that's Disconnect). */
  function removeNetworkLocation(path: string): void {
    networkLocations = settings.removeNetworkLocation(networkLocations, path);
    settings.saveNetworkLocations(networkLocations);
    void loadShared();
  }

  /** Disconnect a mapped network drive from the Shared row menu (CPE-1163) — Windows `net use /delete`
   *  via the backend, then reload. Best-effort: a failure surfaces a notice, never throws. */
  async function disconnectShare(): Promise<void> {
    if (!homeCtxPath) return;
    try {
      await commands.disconnectNetworkShare(homeCtxPath);
      showNotice($t("notice.shareDisconnected", { name: homeCtxName }));
    } catch (e) {
      showNotice(String(e), true);
    }
    void loadShared();
  }

  // ---- Network sidebar section (CPE-1513, epic CPE-1498) ------------------------------------------
  // The visible entry point for the SFTP/WebDAV backend CPE-1510 (keychain secrets) + CPE-1511 (remote
  // `list_dir` routing) already ship. `connections` is loaded once at startup (see the onMount Promise.all
  // below); every mutation here re-fetches the authoritative list from `connections_upsert`/`_remove`'s own
  // return value rather than hand-patching the array, so the sidebar can never drift from the on-disk store.

  /** Connect to a saved connection: resolve whether it needs a keychain secret first (password auth always
   *  does; key auth is tried directly and only reactively re-prompted on failure — see `secretAlwaysRequired`'s
   *  docs), then navigate into its location via the existing `navigate`/`navigateB`, reusing CPE-1511's
   *  remote routing exactly like any other sidebar row. Tracks connected/error state from the SAME
   *  `error`/`errorB` the pane already surfaces, so this never double-fetches the listing. */
  async function onNetworkConnect(conn: Connection, inPaneB = false): Promise<void> {
    if (secretAlwaysRequired(conn.auth)) {
      try {
        const stored = unwrap(await commands.connectionSecretGet(conn.name));
        if (!stored) {
          networkSecretPrompt = { x: 40, y: 80, conn };
          return;
        }
      } catch {
        networkSecretPrompt = { x: 40, y: 80, conn };
        return;
      }
    }
    await connectNetworkConnection(conn, inPaneB);
  }

  /** Actually perform the connect-by-navigating, after any needed secret is already in the keychain. */
  async function connectNetworkConnection(conn: Connection, inPaneB = false): Promise<void> {
    const uri = connectionLocation(conn);
    if (inPaneB) await navigateB(uri);
    else { if (archive) exitArchive(); await navigate(uri); }
    const err = inPaneB ? errorB : error;
    if (err) {
      connectionStates = { ...connectionStates, [conn.name]: "error" };
      connectionErrors = { ...connectionErrors, [conn.name]: err };
    } else {
      connectionStates = { ...connectionStates, [conn.name]: "connected" };
      connectionErrors = { ...connectionErrors, [conn.name]: "" };
    }
  }

  /** The secret prompt's submit (CPE-1510): stash the secret in the keychain long enough for THIS connect
   *  to succeed (the remote route reads secrets from the keychain, not from the navigate call — see
   *  NetworkSecretPrompt's doc comment), then scrub it back out immediately when "Remember" was unchecked,
   *  so "Remember" really means "persist past this app session" rather than silently always persisting. */
  async function submitNetworkSecret(conn: Connection, secret: string, remember: boolean): Promise<void> {
    networkSecretPrompt = null;
    try {
      await commands.connectionSecretSet(conn.name, secret);
    } catch (e) {
      showNotice(String(e), true);
      return;
    }
    await connectNetworkConnection(conn);
    if (!remember) {
      try {
        await commands.connectionSecretDelete(conn.name);
      } catch {
        // Best-effort scrub; a failure here just means the secret outlives this session, which is the
        // safer failure direction (a re-prompt is more annoying than useful, but never a security hole).
      }
    }
  }

  /** The add/edit form's Save (CPE-1513): upsert (editing is just an upsert with the same name), then
   *  replace `connections` with the command's returned whole list — the on-disk store is authoritative. */
  async function saveNetworkConnection(conn: Connection): Promise<void> {
    try {
      connections = unwrap(await commands.connectionsUpsert(conn));
      networkForm = null;
      showNotice($t("notice.connectionSaved", { name: conn.name }));
    } catch (e) {
      showNotice(String(e), true);
    }
  }

  /** "Forget" (CPE-1513): remove the saved profile AND its keychain secret, so nothing orphaned survives. */
  async function forgetNetworkConnection(conn: Connection): Promise<void> {
    try {
      connections = unwrap(await commands.connectionsRemove(conn.name));
    } catch (e) {
      showNotice(String(e), true);
      return;
    }
    try {
      await commands.connectionSecretDelete(conn.name);
    } catch {
      // Best-effort; the profile is already gone either way.
    }
    connectionStates = { ...connectionStates, [conn.name]: "disconnected" };
    showNotice($t("notice.connectionForgotten", { name: conn.name }));
  }

  /** "Disconnect" (CPE-1513): client-side state reset only — there is no backend command yet to tear down
   *  the pooled remote session (CPE-1499's provider pool has no exposed "close" — a natural follow-up once
   *  a real session-status query lands). The connection stays saved; only its status dot resets. */
  function disconnectNetworkConnection(name: string): void {
    connectionStates = { ...connectionStates, [name]: "disconnected" };
  }

  async function checkHomeCtxStale(path: string): Promise<void> {
    try {
      const [found] = await commands.entriesForPaths([path]);
      // Guard against a newer menu having opened for a different row while this awaited.
      if (homeCtxPath === path) homeCtxStale = !found;
    } catch {
      // A failed check is treated as "not stale" — never wrongly disable a live entry over a hiccup;
      // an on-disk action that then fails surfaces its own graceful error.
      if (homeCtxPath === path) homeCtxStale = false;
    }
  }

  /** Open the clicked home item: a folder navigates, a file opens via the OS (reusing `openRecent`,
   *  which self-heals a vanished Recent entry). */
  function openHomeItem() {
    if (!homeCtxPath) return;
    if (homeCtxIsDir) navigate(homeCtxPath);
    else openRecent(homeCtxPath);
  }

  /** Copy the home item to the clipboard (stage a copy, like `doCopy` for a FileList selection). */
  function copyHomeItem() {
    if (!homeCtxPath) return;
    clipboard = stage([homeCtxPath], "copy");
    showNotice($t("home.copiedOneItem"));
  }

  /** Copy the home item's path to the OS clipboard. */
  async function copyHomeItemPath() {
    if (!homeCtxPath) return;
    try {
      await navigator.clipboard.writeText(formatPathsForClipboard([homeCtxPath]));
      showNotice($t("notice.copiedPath"));
    } catch {
      showNotice($t("notice.copyPathFailed"), true);
    }
  }

  /** Reveal the home item in the OS file manager. */
  async function revealHomeItem() {
    if (!homeCtxPath) return;
    try {
      await revealItemInDir(homeCtxPath);
    } catch {
      showNotice($t("notice.revealFailed"), true);
    }
  }

  /** Rename the real file/folder behind a home row (CPE-1162). Home has no `<FileList>` to inline-edit,
   *  so we navigate to the item's PARENT folder and hand off to the existing post-load inline-rename
   *  hook (`pendingRenamePath`) — the same path a freshly-created item uses. */
  async function renameHomeItem() {
    if (!homeCtxPath) return;
    const parent = splitPath(homeCtxPath).at(-2)?.path;
    if (!parent) { showNotice($t("home.renameFromHereFailed"), true); return; }
    pendingRenamePath = homeCtxPath;
    await navigate(parent);
  }

  /** Delete the real file/folder behind a home row to the Recycle Bin (CPE-1162) — the DESTRUCTIVE
   *  action, distinct from `home-remove`'s list-pointer pruning. Reuses the trash command + undo, then
   *  prunes the now-dead pointer from whichever list it came from so the entry doesn't linger. */
  async function deleteHomeItem() {
    if (!homeCtxPath) return;
    const path = homeCtxPath;
    const view = homeCtxView;
    try {
      const results = await commands.deleteToTrash([path]);
      reportResults(results, "moveToBin");
      if (canRestoreTrash) {
        const restored = results.filter((r) => r.ok).map((r) => ({ from: r.path, to: "" }));
        if (restored.length > 0) {
          undoStack = pushUndo(undoStack, { kind: "delete", moves: restored, label: "Delete 1 item" });
        }
      }
      // Trashing the file makes any list pointer to it dead — prune it from its source list.
      if (results.some((r) => r.ok)) removeHomePointer(path, view);
    } catch (e) {
      showNotice(String(e), true);
    }
  }

  /** Prune a home list ENTRY (pointer) — never touches the file on disk. Shared by `home-remove` and
   *  the post-delete cleanup. */
  function removeHomePointer(path: string, view: "recent" | "favorites" | "folders" | "shared") {
    if (view === "favorites") {
      favorites = favorites.filter((f) => f.path !== path);
      settings.saveFavorites(favorites);
    } else if (view === "folders") {
      recentFolders = settings.removeRecent(recentFolders, path);
      settings.saveRecentFolders(recentFolders);
    } else if (view === "shared") {
      // A Shared row's pointer is a user-added network location (CPE-1163).
      removeNetworkLocation(path);
    } else {
      recents = settings.removeRecent(recents, path);
      settings.saveRecents(recents);
    }
  }

  /** Add the clicked home item to Favorites (cross-view action for Recent/Folders rows). */
  function favoriteHomeItem() {
    if (!homeCtxPath) return;
    if (favorites.some((f) => f.path === homeCtxPath)) {
      showNotice($t("notice.alreadyFavorite", { name: homeCtxName }));
      return;
    }
    favorites = settings.toggleFavorite(favorites, { path: homeCtxPath, name: homeCtxName, is_dir: homeCtxIsDir });
    settings.saveFavorites(favorites);
    showNotice($t("notice.addedToFavorites", { name: homeCtxName }));
  }

  /** Pin the clicked home folder to Quick access (cross-view action; folders only). */
  function pinHomeItem() {
    if (!homeCtxPath || !homeCtxIsDir) return;
    const wasPinned = pins.includes(homeCtxPath);
    pins = settings.togglePin(pins, homeCtxPath);
    settings.savePins(pins);
    showNotice(wasPinned ? $t("notice.unpinnedHome", { name: homeCtxName }) : $t("notice.pinnedToQuickAccess", { name: homeCtxName }));
  }

  function openHomeItemProperties() {
    if (!homeCtxPath) return;
    propsFor = [homeCtxEntry()];
  }

  // CPE-1154: kill the native WebView2/Edge browser context menu ("Back / Refresh / Save as /
  // Print / …") EVERYWHERE. The app's own menus open via per-element `on:contextmenu` handlers that
  // already `preventDefault` + dispatch the custom `ContextMenu`; this window-level catch-all ONLY
  // `preventDefault`s — it never `stopPropagation`s and never touches `ctx` — so on a handled element
  // the custom menu still opens, while every otherwise-unhandled pixel (pane padding, the blank area
  // around an empty-folder box, the toolbar, the sidebar, Home) no longer leaks the browser menu.
  function suppressNativeMenu(e: MouseEvent) {
    e.preventDefault();
  }

  /** The keyboard-active pane's live selection state (CPE-1370): in dual-pane mode, when pane B has
   *  focus, navigation/destructive keys must read/write pane B's own `selectionB`/`visibleB`/
   *  `selectedEntriesB` instead of always operating on pane A — `pickActivePane` (lib/selection.ts) is
   *  the pure, unit-tested routing decision; this just wraps it around the live bindings. Single-pane
   *  (dualPane off) — and pane A focused in dual-pane — always resolves to pane A, so existing
   *  single-pane behaviour is untouched. `openEntry` mirrors the double-click wiring on each pane's
   *  `on:open` (line ~5000/5045 below): pane A's richer `open()` (archives/vaults/…) vs pane B's
   *  simpler `openB()` (plain folders only, v1). */
  /** Same paneA/paneB shape as `activePaneState()`, but for an explicit `inPaneB` flag rather than the
   *  live `activePane` (CPE-1377) — a right-click doesn't focus a pane, so a context-menu action must
   *  target whichever pane the menu was opened OVER (`ctx?.inPaneB`), not whichever pane last had a
   *  plain click. Reuses `pickActivePane` (same routing decision `activePaneState()` uses) by always
   *  passing `dualPane: true` and feeding it the explicit flag instead of the live `activePane` — when
   *  `inPaneB` is false this resolves to pane A exactly like single-pane mode always has. */
  function paneStateFor(inPaneB: boolean) {
    const paneA = {
      selection,
      visible,
      selectedEntries,
      setSelection: (s: Selection) => { selection = s; },
      openEntry: (e: DirEntry) => open(e),
    };
    const paneB = {
      selection: selectionB,
      visible: visibleB,
      selectedEntries: selectedEntriesB,
      setSelection: (s: Selection) => { selectionB = s; },
      openEntry: (e: DirEntry) => openB(e),
    };
    return pickActivePane(true, inPaneB ? 1 : 0, paneA, paneB);
  }

  function activePaneState() {
    return paneStateFor(dualPane && activePane === 1);
  }

  // ---- keyboard ----

  /**
   * Run the handler for a remappable built-in action resolved from the effective keymap (CPE-1557, epic
   * CPE-1484). Returns true when it consumed the event (the caller then returns); false for the actions
   * this migration intentionally leaves to the contextual handlers below — `openItem` (Enter) and
   * `clearSelection` (Escape) are context-sensitive, and `shortcutsCheatSheet` ("?") is shadowed by
   * type-ahead today, so routing any of them here would change default behavior. Every `true` branch runs
   * the exact code the old literal chord branch ran, so the default keymap is a no-op change.
   */
  function dispatchMappedAction(
    id: ActionId,
    event: KeyboardEvent,
    pane: ReturnType<typeof activePaneState>,
    inPaneB: boolean,
  ): boolean {
    switch (id) {
      case "back": event.preventDefault(); goBack(); return true;
      case "forward": event.preventDefault(); goForward(); return true;
      case "up": event.preventDefault(); goUp(); return true;
      case "refresh": event.preventDefault(); refresh(); return true;
      case "editAddress": event.preventDefault(); editingPath = true; return true;
      case "searchFolder": event.preventDefault(); navToolbar?.focusSearch(); return true;
      case "findFiles": event.preventDefault(); if (!isHome && !archive) fileSearchOpen = true; return true;
      case "contentSearch": event.preventDefault(); if (!isHome && !archive) contentSearchOpen = true; return true;
      case "instantSearch": event.preventDefault(); instantSearchOpen = true; return true;
      case "newTab": event.preventDefault(); newTab(); return true;
      case "closeTab": event.preventDefault(); closeTab(activeId); return true;
      case "reopenTab": event.preventDefault(); reopenClosedTab(); return true;
      case "nextTab": event.preventDefault(); cycleTab(1); return true;
      case "prevTab": event.preventDefault(); cycleTab(-1); return true;
      case "selectAll":
        event.preventDefault();
        // CPE-1373: keep the current lead (scroll position) instead of jumping to the last row.
        pane.setSelection(selectAll(pane.visible.length, pane.selection.lead));
        return true;
      case "copy": event.preventDefault(); doCopy(inPaneB); return true;
      case "cut": event.preventDefault(); doCut(inPaneB); return true;
      case "paste": event.preventDefault(); doPaste(inPaneB); return true;
      case "duplicate": event.preventDefault(); doDuplicate(inPaneB); return true;
      case "addToDropStack": event.preventDefault(); doAddToDropStack(inPaneB); return true;
      case "undo": event.preventDefault(); undo(); return true;
      case "rename":
        event.preventDefault();
        if (pane.selectedEntries.length === 1) beginRename(pane.selectedEntries[0], inPaneB);
        return true;
      case "deleteToTrash": event.preventDefault(); askDelete(false); return true;
      case "deletePermanent": event.preventDefault(); askDelete(true); return true;
      case "newFolder": event.preventDefault(); newFolder(); return true;
      case "copyAsPath": event.preventDefault(); doCopyPath(); return true;
      case "properties": event.preventDefault(); openProperties(); return true;
      case "toggleDetails":
        event.preventDefault();
        showDetails = !showDetails;
        settings.saveShowDetails(showDetails);
        return true;
      case "popOutPreview": event.preventDefault(); popOutPreview(); return true;
      case "commandPalette": event.preventDefault(); paletteOpen = true; return true;
      case "docsHelp": event.preventDefault(); openDocs(currentSection()); return true;
      // Contextual / shadowed — handled by the switch + type-ahead below, not here.
      case "openItem":
      case "clearSelection":
      case "shortcutsCheatSheet":
        return false;
    }
    return false;
  }

  function handleKeydown(event: KeyboardEvent) {
    const target = event.target as HTMLElement | null;
    // Never hijack keys while typing in an editor, the path bar, or search.
    if (target && ["INPUT", "TEXTAREA"].includes(target.tagName)) return;
    // CPE-1377 review: guard BOTH panes' inline-rename editors, not just pane A's — the INPUT/TEXTAREA
    // check above already blocks typed keystrokes reaching a focused editor, but this is the symmetric
    // guard against global shortcuts (F5, Delete, …) firing while an editor is open-but-unfocused, which
    // previously only worked for pane A.
    if (renamingPath || renamingPathB) return;
    // CPE-1370 review (defense-in-depth): a confirm dialog (delete, etc.) owns the keyboard while open —
    // in particular Tab must NOT be able to flip `activePane` behind it, which would otherwise let a
    // confirm captured against one pane get confirmed against another (see `askDelete`'s snapshot for
    // the primary fix). `ConfirmDialog` only wires its OWN `<svelte:window>` listener for Escape; every
    // other key (including Tab) would otherwise still reach this handler underneath the modal.
    if (confirm) return;

    // Quick-look owns the keyboard while open (CPE-645).
    if (quickLook) {
      if (event.key === "Escape" || event.key === " ") { event.preventDefault(); quickLook = null; }
      else if (event.key === "ArrowRight") { event.preventDefault(); quickLookMove(1); }
      else if (event.key === "ArrowLeft") { event.preventDefault(); quickLookMove(-1); }
      return;
    }
    // The media quick-look owns the keyboard while open (CPE-1430): Space/Esc close, ←/→ step the folder.
    if (mediaQuickLook) {
      const action = mediaQuickLookAction(event);
      if (action === "close") { event.preventDefault(); mediaQuickLook = null; }
      else if (action === "next") { event.preventDefault(); mediaQuickLookStep(1); }
      else if (action === "prev") { event.preventDefault(); mediaQuickLookStep(-1); }
      return;
    }

    // --- Navigation Mode (CPE-1556, epic CPE-1487): opt-in vim-modal layer over the file list. ---
    // The VERY FIRST condition is the Settings gate: when `navigationModeEnabled` is FALSE (the default),
    // this whole branch short-circuits and every line below it runs EXACTLY as it does today — zero
    // behavior change with the mode off. Reaching here already means the file list has focus: the
    // INPUT/TEXTAREA, rename, confirm, and quick-look guards above have all returned, so typing in a field
    // is never intercepted. V1's grammar is unmodified single keys only, so any Ctrl/Alt/Meta chord
    // (Ctrl+C, Ctrl+F, Alt+Arrow, …) is deliberately left to fall through to its existing handler below;
    // the modal layer only ever consumes bare keys.
    if (navigationModeEnabled && !navCommandLineOpen && !event.ctrlKey && !event.altKey && !event.metaKey) {
      // `?` opens the Navigation Mode cheatsheet (discoverability affordance — a one-line addition, no
      // new toolbar button, per the fast/small/predictable tiebreaker).
      if (event.key === "?") { event.preventDefault(); navCheatsheetOpen = true; return; }
      const { state, intent } = reduceNavKey(navState, event.key);
      navState = state;
      if (intent.kind !== "none") {
        event.preventDefault();
        dispatchNavIntent(intent, dualPane && activePane === 1);
        return;
      }
      // A pending chord/count (a bare `g`, or a digit count mid-entry) is buffered but hasn't resolved to
      // an intent yet: swallow the key so it doesn't also type-ahead, and keep waiting for the rest.
      if (state.pendingChord !== "" || state.pendingCount !== "") { event.preventDefault(); return; }
      // Otherwise the key means nothing to the mode (e.g. an unmapped letter) — fall through so today's
      // handlers (type-ahead find, etc.) still run, matching the surveyed TUIs' "unmapped keys pass".
    }

    const ctrl = event.ctrlKey || event.metaKey;
    // CPE-1370: the pane keyboard nav/destructive keys below act on — pane B only in dual-pane mode
    // with pane B focused, pane A otherwise. Computed once up front so every case below reads/writes
    // the same pane consistently.
    const pane = activePaneState();
    const inPaneB = dualPane && activePane === 1;
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
    // Space quick-looks the selected image (CPE-645) or media file (CPE-1430) — image lightbox first,
    // then the full-screen media player; both are guarded to their own file kinds so they never collide.
    if (!ctrl && !event.altKey && !event.shiftKey && event.key === " " && (openQuickLook(inPaneB) || openMediaQuickLook(inPaneB))) { event.preventDefault(); return; }

    // --- Remappable built-in actions (CPE-1557, epic CPE-1484 "hotkey customization"). ---
    // Every hard-coded chord branch that used to live here is now resolved through the EFFECTIVE keymap:
    // `chordFromEvent` canonicalizes the keypress (permissively — bare F5/F2/Delete included),
    // `actionForChord` maps it (exact match, first-wins) to a registry ActionId, and `dispatchMappedAction`
    // runs that action's existing handler. With the DEFAULT keymap every default chord resolves to exactly
    // the action it always fired, so this is a byte-for-byte no-op change; a remap saved via the bindings UI
    // (which publishes to `keymapStore`) changes what a chord does LIVE, no restart. A chord the user
    // cleared to "" never matches, so that action simply stops firing. Contextual/shadowed keys — Enter
    // (open), Escape (clear), the "?" cheat sheet (shadowed by type-ahead below today) — are deliberately
    // NOT routed here; `dispatchMappedAction` returns false for them so they fall through to the switch
    // and type-ahead exactly as before.
    //
    // Two escape hatches stay AHEAD of the keymap so their exact prior behavior is preserved:
    //   • Ctrl+C while text is selected must fall through to the browser's own copy (Preview Pane text),
    //     returning before any later handler — unchanged from before.
    //   • Alt+D is a secondary "edit address" accelerator the single-binding registry doesn't model
    //     (Ctrl+L is the registry's editAddress), so it stays as a literal and keeps working.
    if (ctrl && event.key.toLowerCase() === "c" && !(window.getSelection()?.isCollapsed ?? true)) return;
    if (event.altKey && event.key.toLowerCase() === "d") { event.preventDefault(); editingPath = true; return; }
    const mappedAction = actionForChord($keymapStore, chordFromEvent(event));
    if (mappedAction && dispatchMappedAction(mappedAction, event, pane, inPaneB)) return;

    // "?" opens the keyboard-shortcuts cheat sheet (CPE-1584 fix) — special-cased HERE, ahead of the
    // type-ahead block below, because type-ahead greedily claims every bare single-character printable
    // key (event.key.length === 1) INCLUDING "?", so a literal `case "?"` further down the switch could
    // never be reached. No INPUT/TEXTAREA/rename/confirm/quick-look context is focused at this point —
    // every one of those already returned earlier in this handler — so a bare "?" unambiguously means
    // "open the cheat sheet," never "type a question mark."
    if (!ctrl && !event.altKey && event.key === "?") { event.preventDefault(); shortcutsOpen = true; return; }

    // Type-ahead find: a printable key with no modifier jumps to the next match.
    if (!ctrl && !event.altKey && event.key.length === 1 && /\S/.test(event.key)) {
      event.preventDefault();
      const now = Date.now();
      const continuing = now - typeAheadAt <= 700;
      typeAheadBuffer = continuing ? typeAheadBuffer + event.key : event.key;
      typeAheadAt = now;
      const single = typeAheadBuffer.length === 1;
      const idx = firstMatchIndex(
        pane.visible.map((e) => e.name),
        typeAheadBuffer,
        pane.selection.lead,
        single,
      );
      if (idx >= 0) pane.setSelection(selectOnly(idx));
      return;
    }

    // NOTE: F1 (docsHelp), F5 (refresh), F2 (rename), Delete/Shift+Delete (deleteToTrash/deletePermanent),
    // and "?" (shortcutsCheatSheet) used to live in this switch. The first four are remappable and
    // resolved above via the keymap dispatch (removed here to avoid double-firing). "?" is ALSO handled
    // above now (CPE-1584 fix) — special-cased ahead of the type-ahead block that used to shadow it, not
    // routed through the keymap since it stays a fixed, non-remappable key (like Enter/Escape below).
    switch (event.key) {
      case "Escape":
        // CPE-1370 review: route through the active pane like every other case here, so Escape clears
        // pane B's selection when it's the one focused instead of always hard-clearing pane A's.
        pane.setSelection(emptySelection());
        ctx = null;
        break;
      case "ArrowDown":
        event.preventDefault();
        pane.setSelection(moveLead(pane.selection, arrowDelta("ArrowDown", currentGridCols()), pane.visible.length, event.shiftKey));
        break;
      case "ArrowUp":
        event.preventDefault();
        pane.setSelection(moveLead(pane.selection, arrowDelta("ArrowUp", currentGridCols()), pane.visible.length, event.shiftKey));
        break;
      case "ArrowRight":
      case "ArrowLeft": {
        // 2-D grid nav (CPE-769): in icons/gallery, Left/Right move the lead by one tile (moveLead wraps
        // across rows + clamps). In list/details (single column) they're left unhandled — no horizontal move.
        const gcols = currentGridCols();
        if (gcols > 1) {
          event.preventDefault();
          pane.setSelection(moveLead(pane.selection, arrowDelta(event.key, gcols), pane.visible.length, event.shiftKey));
        }
        break;
      }
      case "PageDown":
      case "PageUp": {
        // CPE-1374: move the lead by ~one viewport of rows, grid-aware (currentGridCols) — same
        // Shift-extend semantics as Arrow keys, just scaled up to a page.
        event.preventDefault();
        const delta = pageDelta(event.key, currentGridCols(), visibleRowCount());
        pane.setSelection(moveLead(pane.selection, delta, pane.visible.length, event.shiftKey));
        break;
      }
      case "Home":
        event.preventDefault();
        pane.setSelection(moveLead(pane.selection, -pane.visible.length, pane.visible.length, event.shiftKey));
        break;
      case "End":
        event.preventDefault();
        pane.setSelection(moveLead(pane.selection, pane.visible.length, pane.visible.length, event.shiftKey));
        break;
      case "Enter":
        if (target?.closest(".row")) return;
        event.preventDefault();
        if (pane.selectedEntries[0]) void pane.openEntry(pane.selectedEntries[0]);
        break;
      case "Backspace":
        event.preventDefault();
        goUp();
        break;
    }

    // User-bound macro hotkeys (CPE-1191) — checked LAST, after every built-in binding above, so a
    // macro can never shadow one: this only runs when the combo didn't match any `if`/`case` earlier
    // in this handler (every one of them either `return`s or `break`s without touching this code).
    // `hotkeyFromEvent` returns "" for a combo with no Ctrl/Alt, so it never fires for the type-ahead
    // or plain-typing cases already handled above.
    const macroCombo = hotkeyFromEvent(event);
    const macroHit = macroCombo ? matchHotkey(macroBindings, macroCombo) : undefined;
    if (macroHit && macroSummaries.some((m) => m.name === macroHit.name)) {
      event.preventDefault();
      void startMacro(macroHit.name);
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
    // CPE-1140: re-clamp persisted widths to the current dynamic rules on load, so a value saved
    // under the old fixed SIDEBAR_MAX/RIGHT_MAX (or simply too wide for today's window) can't paint
    // a broken first layout. Load both raw values first, then clamp each against the other's
    // now-current width — sidebarMaxWidth()/rightMaxWidth() read live state, the same functions the
    // drag-resize handlers use, so load and drag can never disagree.
    sidebarWidth = settings.loadSidebarWidth();
    rightWidth = settings.loadRightWidth();
    if (showDetails && !dualPane) {
      // Both side panes are live: fit them TOGETHER (order-independent) so two large persisted widths on a
      // now-narrower window shrink proportionally instead of the first-clamped pane absorbing the whole
      // squeeze (CPE-1140 review). Budget = window minus the two dividers and the middle's minimum.
      const budget = window.innerWidth - 2 * PANE_DIVIDER_W - MID_MIN;
      [sidebarWidth, rightWidth] = fitSidePanes(sidebarWidth, rightWidth, SIDEBAR_MIN, RIGHT_MIN, budget);
    } else {
      // Only the sidebar competes with the middle here (right pane absent / is a dual-pane file column), so
      // its single dynamic clamp is already order-independent.
      sidebarWidth = clampWidth(sidebarWidth, SIDEBAR_MIN, sidebarMaxWidth());
      rightWidth = clampWidth(rightWidth, RIGHT_MIN, rightMaxWidth());
    }
    pins = settings.loadPins();
    recents = settings.loadRecents();
    favorites = settings.loadFavorites();
    recentFolders = settings.loadRecentFolders();
    networkLocations = settings.loadNetworkLocations();
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

  /** (Re)start or stop the live watched-folder watcher to match the current config (CPE-794), ALSO
      folding in whichever paths an open smart folder needs watched (CPE-1230) — one shared `notify`
      watcher/event-bus rather than a second one. Rule execution (`startFolderWatch`'s own listener) is
      unaffected by the extra paths: it only *acts* on a landed file that matches a configured watch
      rule, so a smart-folder-only path with no matching rule is a no-op for it (typically ALL of them,
      since most users have configured no watch rules at all). Only the sidecar build has the backend;
      a no-op fails soft elsewhere. */
  async function reconcileWatch() {
    const rulePaths = watchLive ? watchedFolders : [];
    const scopePaths = watchPathsForScope(smartFolderScope);
    const paths = Array.from(new Set([...rulePaths, ...scopePaths]));
    if (paths.length && aiConsoleAvailable) {
      // Gate the rules themselves on `watchLive`, not just the path list: a smart folder's scope can
      // overlap a configured (but paused) rule's folder, which would otherwise keep `paths` nonempty
      // and reactivate every enabled rule — "off means off" (CPE-1230 review) means a paused rule set
      // must never execute just because a smart folder is open watching the same paths.
      await startFolderWatch(paths, () => (watchLive ? watchRules : []), (fire) => {
        watchLog = [fire, ...watchLog].slice(0, 50);
        showNotice($t("notice.watchFired", { summary: fire.summary }));
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
      showNotice($t("notice.watchUndone", { rule: fire.rule }));
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
      showNotice($t("notice.attributesNeedSelection"));
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
      unwrap(await commands.writeFileText(path, payload.content));
      showNotice($t("notice.exported", { name: path.split(/[\\/]/).pop() ?? path }));
    } catch (e) {
      showNotice(String(e), true);
    }
  }

  /** Record a successfully-opened folder in the recently-visited MRU (CPE-342). */
  function recordRecentFolder(path: string) {
    const name = path.split(/[\\/]/).filter(Boolean).pop() ?? path;
    recentFolders = settings.addRecent(recentFolders, { path, name });
    settings.saveRecentFolders(recentFolders);
    // Mirror into the system-tray quick-access list (CPE-1272, epic CPE-713) so the tray menu offers a
    // one-click jump back here. Fire-and-forget: the tray is a nicety, never blocking navigation, and it's
    // simply absent outside a desktop Tauri build (the call no-ops / rejects, which we swallow).
    void invoke("tray_note_folder", { path, label: name }).catch(() => {});
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
      showNotice($t("notice.previewPopoutNeedsOne"), true);
      return;
    }
    // Inside an archive the selected entry's path is virtual (CPE-1360). The float window has no archive
    // context of its own, so resolve the inner entry to a real temp-file DirEntry here before sending it,
    // giving the FloatPreview host the same previewable entry the in-app pane gets.
    let floatEntry = entry;
    if (archive && !entry.is_dir) {
      try {
        floatEntry = await resolveArchivePreviewEntry(archive.zipPath, entry);
      } catch {
        showNotice($t("notice.previewFromArchiveFailed", { name: entry.name }), true);
        return;
      }
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
      await emit("float:add", floatEntry);
      await win.setFocus();
    } catch (e) {
      console.debug("pop out failed:", e);
      showNotice($t("notice.previewWindowFailed"), true);
    }
  }

  /** Route a menu selection to its action. See MenuBar's `menus` table. */
  function onMenuSelect(action: string) {
    switch (action) {
      case "command-palette": paletteOpen = true; break; // Tools ▸ Command Palette (CPE-1164)
      case "exit": exitApp(); break;
      case "check-updates": checkForUpdates(true); break;
      case "settings": showSettings = true; break;
      case "shortcuts": shortcutsOpen = true; break;
      case "documents": openDocs(currentSection()); break;
      case "diagnostics": diagnostics = !diagnostics; settings.saveDiagnostics(diagnostics); break;
      case "about": showAbout = true; break;
      case "content-search": if (!isHome && !archive) contentSearchOpen = true; break;
      case "find-duplicates": if (!isHome && !archive) duplicatesOpen = true; break;
      case "find-similar-images": if (!isHome && !archive) similarImagesOpen = true; break;
      case "find-similar-documents": if (!isHome && !archive) similarDocsOpen = true; break;
      case "find-similar-folders": if (!isHome && !archive) similarFoldersOpen = true; break;
      case "find-dangling-links": if (!isHome && !archive) openFileHealth("dangling"); break;
      case "find-type-mismatches": if (!isHome && !archive) openFileHealth("mismatch"); break;
      case "find-orphan-sidecars": if (!isHome && !archive) openFileHealth("orphan"); break;
      case "find-empty-dirs": if (!isHome && !archive) openFileHealth("empty"); break;
      case "find-clutter": if (!isHome && !archive) declutterOpen = true; break;
      case "organize-folder": if (!isHome && !archive) organizeOpen = true; break;
      case "copy-file-names": copyListing(namesList(visible), "names"); break;
      case "copy-file-list": copyListing(detailList(visible), "rows"); break;
      case "save-file-list": saveFileList(); break;
    }
  }

  /** Save the current (visible) folder listing to a CSV/TXT file via a native Save dialog (CPE-425). */
  async function saveFileList() {
    if (isHome || visible.length === 0) {
      showNotice($t("notice.nothingToSave"));
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
      unwrap(await commands.writeFileText(path, text));
      showNotice($t("notice.savedRows", { count: visible.length, name: path.split(/[\\/]/).pop() ?? path }));
    } catch (e) {
      showNotice(String(e), true);
    }
  }

  /** Export the whole tag store to a JSON file (CPE-654). */
  async function exportTagsToFile() {
    try {
      const path = await saveFileDialog({ defaultPath: "tags.json", filters: [{ name: "JSON", extensions: ["json"] }] });
      if (!path) return;
      unwrap(await commands.writeFileText(path, exportTags()));
      showNotice($t("notice.tagsExportedTo", { name: path.split(/[\\/]/).pop() ?? path }));
    } catch (e) {
      showNotice(String(e), true);
    }
  }

  /** Import a tag store JSON file, merged into the current tags (CPE-654). */
  async function importTagsFromFile() {
    try {
      const picked = await openFolderDialog({ directory: false, multiple: false, filters: [{ name: "JSON", extensions: ["json"] }] });
      if (!picked || typeof picked !== "string") return;
      const json = unwrap(await commands.readFileText(picked, 16 * 1024 * 1024));
      await importTags(json);
      showNotice($t("tags.imported"));
    } catch (e) {
      showNotice(String(e), true);
    }
  }

  /** Copy the current (visible) folder listing to the clipboard as text (CPE-422). `kind` (CPE-1634)
   *  picks the translated wording ("names" vs "rows") — an identifier, not display text, so this stays
   *  English-free; see `reportResults`' `OpKind` above for the same pattern. */
  async function copyListing(text: string, kind: "names" | "rows") {
    if (isHome || visible.length === 0) {
      showNotice($t("notice.nothingToCopy"));
      return;
    }
    try {
      await navigator.clipboard.writeText(text);
      const key =
        kind === "names"
          ? visible.length === 1
            ? "notice.copiedNamesOne"
            : "notice.copiedNamesMany"
          : visible.length === 1
            ? "notice.copiedRowsOne"
            : "notice.copiedRowsMany";
      showNotice($t(key, { count: visible.length }));
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
        unwrap(await commands.openExternal(a.target));
        showNotice($t("notice.actionLabel", { label: a.label }));
      } else if (a.kind === "open-github") {
        const url = await commands.gitRemoteUrl(a.target);
        if (url) await openUrl(url);
        else showNotice($t("notice.noRemoteUrl"), true);
      }
    } catch (e) {
      console.debug("context action failed:", e);
      showNotice($t("notice.actionFailed"), true);
    }
  }

  /** Open a URL in the default browser, surfacing failures rather than swallowing. */
  async function openExternal(url: string) {
    try {
      await openUrl(url);
    } catch {
      showNotice($t("link.openFailed"), true);
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
    userCommands = settings.loadUserCommands(); // CPE-783: user-defined commands
    macroBindings = settings.loadMacroBindings(); // CPE-1191: saved-macro surface/hotkey bindings
    void refreshMacroSummaries(); // CPE-1189: catalog for menu/palette surfacing + hotkey dispatch
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
    // Drop Stack (CPE-1530/1531/1532): hydrate the reactive store from settings.json once, BEFORE any
    // "Add to Drop Stack" action can fire and before the panel first renders — otherwise a first add
    // would overwrite a persisted stack with just the new entries instead of appending to it (the store
    // starts empty until loaded), and the panel would show an empty shelf until something new landed.
    // Idle (empty) until something is shelved. Idempotent/sync — safe to call once here.
    initDropStack();
    listen<TransferReport>("transfer://done", (e) => {
      const r = e.payload;
      // Archive compress/extract (CPE-1184) queue through the same engine but resolve via the
      // `onSuccess` closure the call site registered in `pendingArchiveOps` (it already knows the
      // exact wording — a "Compressed"/"Extracted" notice, not "Copied"). The actual folder refresh
      // (CPE-1386) is centralized HERE via `refreshBatchApplyTarget(pending.dir)` — reused as-is, so a
      // pane-B-opened compress/extract refreshes pane B (and pane A too if it happens to mirror the same
      // folder), instead of the pre-CPE-1386 hard-coded pane-A `loadPath(currentPath)`.
      if (r.op === "compress" || r.op === "extract") {
        const pending = pendingArchiveOps.get(r.id);
        pendingArchiveOps.delete(r.id);
        if (!pending) {
          // A very fast/small archive op — e.g. "Compress with password…", whose dialog also closes
          // right after queuing — can finish and emit this event before the call site's `await
          // startArchiveCompress(...)` continuation has run `pendingArchiveOps.set(id, ...)` (CPE-1254:
          // the operations panel still shows "N item(s) compressed" via its own independent listener
          // in `lib/transfers.ts`, but this listener — the one that owns the folder refresh — had
          // nothing registered yet, so the new/extracted entry never appeared). This race predates pane
          // routing and can't be fixed here — the transfer id isn't known until the same continuation
          // that's racing against this event, so nothing can be keyed by it any earlier (CPE-1386: now
          // that a pane-B compress/extract can also hit this window, the fallback below stays pane-A-only
          // — a rare, pre-existing limitation, not a wrong-pane action against a specific op). On a clean
          // finish, still refresh pane A's folder so the entry shows up there; fall back to a generic
          // notice since the call site's specific wording isn't available here.
          if (!r.cancelled && r.failed === 0) {
            const ONE = r.op === "compress" ? "notice.archiveCompressedOne" : "notice.archiveExtractedOne";
            const MANY = r.op === "compress" ? "notice.archiveCompressedMany" : "notice.archiveExtractedMany";
            showNotice($t(r.transferred === 1 ? ONE : MANY, { count: r.transferred }));
            loadPath(currentPath).catch(() => {});
          }
          return;
        }
        if (r.cancelled) { showNotice(pending.cancelledNotice); return; }
        if (r.failed > 0) { showNotice(r.errors[0] || pending.failedNotice, true); return; }
        Promise.resolve(pending.onSuccess()).then(() => refreshBatchApplyTarget(pending.dir)).catch(() => {});
        return;
      }
      // CPE-1533: a Drop-Stack "Copy all here" is tagged in `dropStackTransferOps` with the exact paths
      // it captured — on a clean, uncancelled, all-transferred finish those paths come off the Drop
      // Stack. `TransferReport` has no per-path result (aggregate counts only), so a partial
      // failure/skip/cancel leaves the whole captured batch shelved rather than guessing which landed.
      const dropStackPaths = dropStackTransferOps.get(r.id);
      if (dropStackPaths) {
        dropStackTransferOps.delete(r.id);
        if (!r.cancelled && r.failed === 0 && r.skipped === 0) {
          dropStackPaths.forEach((p) => removeFromDropStack(p));
        }
      }
      // CPE-1380/CPE-1384: a clipboard-paste copy OR a "Copy to…" that targeted pane B is tagged in
      // `pasteCopyPaneB` (by `startCopyWithPolicy` / `copyMoveToFolder`) — refresh pane B for it, PLUS
      // pane A too when pane A happens to mirror the same folder (both-can-match, same reasoning as
      // `refreshDropSourcePane`/`refreshPasteAffectedPanes` — otherwise pane A would miss the newly-copied
      // item if both panes show the same folder). Every other copy source (a pane-A "Copy to…",
      // drag-drop, Home copy, quick actions) never adds an id here, so this falls through to the
      // pre-existing pane-A-only refresh, unchanged.
      if (pasteCopyPaneB.delete(r.id)) {
        if (paneBPath && normalizePath(paneBPath) === normalizePath(currentPath)) loadPath(currentPath).catch(() => {});
        explorerPaneB?.loadListing(paneBPath, false).catch(() => {});
      } else {
        loadPath(currentPath).catch(() => {});
      }
      if (r.cancelled) showNotice($t("xfer.cancelled"));
      else if (r.failed > 0) showNotice($t("notice.copyFailedSome", { transferred: r.transferred, failed: r.failed }), true);
      else showNotice($t(r.transferred === 1 ? "notice.copiedItemsOne" : "notice.copiedItemsMany", { count: r.transferred }));
    }).then((un) => (unlistenTransferDone = un)).catch(() => {});

    // Open the regular Documents dialog when another window (e.g. the Agent Deck's area "?" help) asks
    // for a specific doc section, instead of that window showing its own inline help panel (CPE-929).
    // Bring the main window forward so the dialog is actually visible.
    listen<{ slug?: string }>("open-docs", (e) => {
      const slug = e.payload?.slug;
      if (slug) openDocsSlug(slug);
      getCurrentWindow().setFocus().catch(() => {});
    }).then((un) => (unlistenOpenDocs = un)).catch(() => {});

    // Spotlight overlay (CPE-1216, epic CPE-704): CPE-1215 owns the OS-level global hotkey + emits this
    // event from the backend when it fires. The overlay is also reachable in-app via the Command
    // Palette's "Spotlight (search everywhere)…" entry above, so it's testable without the OS hotkey.
    listen("spotlight:open", () => {
      spotlightOpen = true;
    }).then((un) => (unlistenSpotlightOpen = un)).catch(() => {});

    // System-tray quick-access jump (CPE-1272, epic CPE-713): the tray backend shows/focuses the window
    // and emits this with the chosen folder path; navigate there through the normal navigation path.
    listen<string>("tray://open-folder", (e) => {
      const path = e.payload;
      if (path) navigate(path);
    }).then((un) => (unlistenTrayOpen = un)).catch(() => {});

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
        commands.specialFolders(),
        commands.listDrives(),
        commands.homeDir().then(unwrap),
        commands.canRestoreFromTrash(),
      ]);
      places = p;
      drives = d;
      homePath = h;
      canRestoreTrash = canRestore;
      loadDriveUsage(d); // fire-and-forget: sidebar usage bars (CPE-406)
      loadDriveRemovable(d); // fire-and-forget: which drives get an eject button (CPE-1278)
    } catch (e) {
      console.debug("could not load places:", e);
    }
    // Network sidebar section (CPE-1513; permanent since CPE-1516): saved connections + OS-discovered
    // shares, both needed up front (not pull-on-open like the Home Shared tab used to be alone) so the
    // section's empty-vs-populated body is correct from first paint. Both are time-bounded in the backend,
    // so an offline server/share can't hang startup (see `loadShared`'s own doc comment) — and deliberately
    // NOT awaited inline here: this whole onMount is one sequential chain ending in `restoreLastSession()`
    // below, so an awaited call here would delay session restore by exactly this round trip for zero
    // benefit (the sidebar reacting a beat later is invisible; a delayed session restore is not).
    // Fire-and-forget, same as `loadShared` itself.
    (async () => {
      try {
        // `?? []` guards a backend/test-double that hands back `null` for an empty store — the sidebar
        // section's empty-body check (`hasAnyNetworkRows`) assumes a real array.
        connections = unwrap(await commands.connectionsList()) ?? [];
      } catch (e) {
        console.debug("could not load saved connections:", e);
      }
    })();
    void loadShared();
    // CPE-1519's "Discovered on your network" tier: same fire-and-forget treatment as `loadShared` just
    // above (time-bounded backend-side, an offline/absent neighborhood can't hang session restore below).
    void loadDiscovered();
    try {
      appVersion = await getVersion();
    } catch {
      // Version is cosmetic (About dialog) — a failure must not break startup.
    }

    // A `--open <dir>` launch argument (CPE-1043) opens the explorer at that folder, taking precedence
    // over last-session restore for this launch. The backend injects the resolved folder as a synchronous
    // `window.__CPE_OPEN_DIR__` global (set before this script runs), so no command/gate is involved — in
    // a plain browser / test env the global is simply absent and we fall through to the normal startup.
    const openArg =
      typeof window !== "undefined"
        ? (window as unknown as { __CPE_OPEN_DIR__?: string }).__CPE_OPEN_DIR__ ?? null
        : null;
    if (openArg) {
      // Navigate (not just loadPath): the active tab's history drives `currentPath`/the breadcrumb, so we
      // must push the folder onto it — a bare loadPath would fetch the listing but leave the view on Home.
      await navigate(openArg);
    } else {
      const restored = await restoreLastSession();
      if (!restored) await loadPath(HOME);
    }
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

    // CPE-1154: app-wide native-context-menu suppressor (see suppressNativeMenu above). Registered
    // here alongside the other window listeners and torn down in onDestroy below.
    window.addEventListener("contextmenu", suppressNativeMenu);

    // Drive-connect scheduler (CPE-797): starts polling only if a backup job opted into auto-run.
    reconcileDriveScheduler();

    // Live removable-drive detection (CPE-1280): keep the sidebar Drives section in step with reality —
    // a plugged-in USB appears, an unplugged one drops out — without relaunching. Always-on but cheap
    // (fires only on a real drive-set change); poke it on focus for instant feedback after alt-tabbing.
    startDriveWatch(applyDriveList);
    window.addEventListener("focus", pokeDriveWatch);
  });

  onDestroy(() => {
    if (verifyTimer) clearInterval(verifyTimer); // CPE-875: stop the periodic integrity re-verify
    smartRefreshDebounce.cancel(); // CPE-1633: close/close-only teardown missed the destroy-while-open case
    smartRefreshUnlisten?.();
    unlistenSessions?.();
    unlistenTransferDone?.();
    unlistenOpenDocs?.();
    unlistenSpotlightOpen?.();
    unlistenTrayOpen?.();
    unlistenOsDrop?.();
    unlistenActivity?.();
    // CPE-1643: these two are armed alongside `unlistenActivity` (same reconcile block, ~L1476-1492)
    // but were missed here — a watch still armed at destroy time left both listeners registered past
    // the component's life.
    unlistenDiffs?.();
    unlistenCost?.();
    if (watchRefreshTimer) clearTimeout(watchRefreshTimer);
    if (autoMirrorTimer) clearInterval(autoMirrorTimer);
    window.removeEventListener("focus", maybeAutoSync);
    window.removeEventListener("contextmenu", suppressNativeMenu); // CPE-1154
    stopDriveScheduler();
    stopDriveWatch(); // CPE-1280: stop live drive polling
    window.removeEventListener("focus", pokeDriveWatch); // CPE-1280
    // CPE-1643: `showNotice`'s timer was only ever cleared by the NEXT notice replacing it, never on
    // destroy — a pending one otherwise outlives the component.
    if (noticeTimer) clearTimeout(noticeTimer);
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
      style="order: {appOrder.board}"
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
        style="order: {appOrder.repos}"
        title={$t("sidebar.repositories")}
        on:click={() => (showRepos = true)}
      >
        <Icon name="code" size={15} /> {$t("sidebar.repositories")}
      </button>
      <button
        class="tb-console"
        type="button"
        style="order: {appOrder.console}"
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
  {density}
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
  {density}
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
  on:density={(e) => setDensity(e.detail)}
  on:navigate={(e) => onCrumbNavigate(e.detail)}
  on:search={(e) => { search = e.detail; selection = emptySelection(); }}
  on:searchDocs={() => openDocsSlug("12-search")}
  on:searchDeep={(e) => {
    if (isHome) { showNotice($t("search.deepNeedsFolder"), false); return; }
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
  {showTerminal}
  userCommands={userCommandsToolbar}
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
  on:toggleTerminal={() => (showTerminal = !showTerminal)}
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
          bind:value={sidebarWidth}
          on:change={() => {
            sidebarWidth = clampWidth(sidebarWidth, SIDEBAR_MIN, sidebarMaxWidth());
            settings.saveSidebarWidth(sidebarWidth);
          }}
        />
      </div>
    </Toolbar>
    <Sidebar
      {places}
      {drives}
      {favorites}
      {density}
      {driveUsage}
      {driveRemovable}
      sessions={$agentSessions}
      {currentPath}
      {isHome}
      selectedPath={selectedEntries.length === 1 && selectedEntries[0]?.is_dir ? selectedEntries[0].path : ""}
      {draggedPaths}
      {tagList}
      selectedTag={dualPane && activePane === 1 ? selectedTagB : selectedTag}
      smartFolders={$smartFolders}
      activeSmartFolder={smartFolder?.id ?? ""}
      savedSearches={$savedSearches}
      activeSavedSearch={structuredSearch?.id ?? ""}
      {connections}
      networkShares={shared}
      {discoveredShares}
      {connectionStates}
      {connectionErrors}
      canBrowseTrash={canRestoreTrash}
      on:openTrash={() => (showTrash = true)}
      on:networkAdd={(e) =>
        (networkForm = { x: e.detail.x, y: e.detail.y, editing: null, prefill: e.detail.prefill ?? null })}
      on:networkConnect={(e) => void onNetworkConnect(e.detail)}
      on:networkContext={(e) => (networkContextMenu = { x: e.detail.x, y: e.detail.y, conn: e.detail.conn })}
      on:filterTag={(e) => {
        // The Sidebar is shared by both panes (CPE-1376), so route the click to whichever pane is
        // active — same `activePane` split as `commanderContext`/`activePaneState` (CPE-1370).
        if (dualPane && activePane === 1) selectedTagB = selectedTagB === e.detail ? "" : e.detail;
        else selectedTag = selectedTag === e.detail ? "" : e.detail;
      }}
      on:tagMenu={(e) => (tagMenu = e.detail)}
      on:openSmartFolder={(e) => openSmartFolder(e.detail)}
      on:smartFolderMenu={(e) => (smartFolderMenu = e.detail)}
      on:openSavedSearch={(e) => openStructuredSearch(e.detail)}
      on:savedSearchMenu={(e) => (structuredSearchMenu = e.detail)}
      on:driveContext={(e) => onDriveContext(e.detail)}
      on:eject={(e) => ejectDrive(e.detail.path, e.detail.name)}
      on:navigate={(e) => { if (archive) exitArchive(); navigate(e.detail); }}
      on:openFile={(e) => openRecent(e.detail)}
      on:home={() => {
        // CPE-1383: route Home to whichever pane is active — same `dualPane && activePane === 1` split
        // `activePaneState`/the `filterTag` handler above use. Pane B has no archive-browse concept (see
        // `archive`'s pane-A-only wiring on the left `<ExplorerPane>`), so `exitArchive` only applies to
        // pane A's path; `navigateB(HOME)` already short-circuits correctly (CPE-1377).
        if (dualPane && activePane === 1) { void navigateB(HOME); return; }
        if (archive) exitArchive();
        navigate(HOME);
      }}
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
    {#if activeVaultBlob}
      <!-- Unlocked-vault browsing banner (CPE-1249): shown while navigated inside a decrypted vault's
           session dir; its Lock button re-seals the vault (navigate out + wipe). -->
      <VaultBanner
        name={vaultDisplayName(activeVaultBlob)}
        on:lock={() => { if (activeVaultBlob) lockActiveVault(activeVaultBlob); }}
      />
    {/if}
    <ExplorerPane
      bind:this={explorerPane}
      inHome={isHome && !smartFolder && !structuredSearch}
      {density}
      {places}
      {drives}
      {pins}
      {recents}
      {favorites}
      {recentFolders}
      {shared}
      {sharedLoading}
      {activeWatchCwd}
      {watchedAgentName}
      {recentChanges}
      sessions={$agentSessions}
      bind:showTimeline
      replayOverlay={replayOverlayEntries}
      bind:entries
      smartOverride={smartFolder ? smartEntries : structuredSearch ? structuredSearchEntries : null}
      archiveOverride={archive ? archiveChildren(archive) : null}
      archivePath={archive ? archive.zipPath : null}
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
      {activeMetaColumns}
      on:resizeMetaColumns={(e) => { activeMetaColumns = applyMetaColumnWidths(activeMetaColumns, e.detail); settings.saveMetaColumnsForFolder(currentPath, activeMetaColumns); }}
      on:openColumnPicker={() => { columnPickerInPaneB = false; columnPickerOpen = true; }}
      bind:selection
      bind:selectedEntries
      bind:draggedPaths
      bind:rowEls
      on:contextAction={(e) => handleContextAction(e.detail)}
      on:navigate={(e) => navigate(e.detail)}
      on:openRecent={(e) => openRecent(e.detail)}
      on:homeSelect={(e) => selectHomeEntry(e.detail)}
      on:unpin={(e) => { pins = settings.togglePin(pins, e.detail); settings.savePins(pins); }}
      on:unfavorite={(e) => { favorites = favorites.filter((f) => f.path !== e.detail); settings.saveFavorites(favorites); }}
      on:removeRecent={(e) => { recents = settings.removeRecent(recents, e.detail); settings.saveRecents(recents); }}
      on:removeRecentFolder={(e) => { recentFolders = settings.removeRecent(recentFolders, e.detail); settings.saveRecentFolders(recentFolders); }}
      on:clearRecents={() => { recents = []; settings.saveRecents(recents); }}
      on:open={(e) => open(e.detail)}
      on:rowContext={(e) => onRowContext(e.detail)}
      on:driveContext={(e) => onDriveContext(e.detail)}
      on:homeItemContext={(e) => onHomeItemContext(e.detail)}
      on:loadShared={() => loadShared()}
      on:addNetworkLocation={(e) => addNetworkLocation(e.detail)}
      on:removeNetworkLocation={(e) => removeNetworkLocation(e.detail)}
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
        inHome={paneBPath === HOME}
        {density}
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
        {shared}
        {sharedLoading}
        {colorRules}
        {folderContexts}
        {view}
        {sortKey}
        {sortDir}
        {foldersFirst}
        {showHidden}
        {search}
        {fileFilter}
        {cutPaths}
        {showFolderSizes}
        {folderSizes}
        on:needSizes={(e) => fillFolderSizes(e.detail)}
        bind:renamingPath={renamingPathB}
        renameValue={renameValueB}
        bind:columnWidths
        activeMetaColumns={activeMetaColumnsB}
        on:resizeMetaColumns={(e) => { activeMetaColumnsB = applyMetaColumnWidths(activeMetaColumnsB, e.detail); settings.saveMetaColumnsForFolder(paneBPath, activeMetaColumnsB); }}
        on:openColumnPicker={() => { columnPickerInPaneB = true; columnPickerOpen = true; }}
        bind:selectedTag={selectedTagB}
        bind:draggedPaths
        on:contextAction={(e) => handleContextAction(e.detail)}
        on:open={(e) => openB(e.detail)}
        on:navigate={(e) => navigateB(e.detail)}
        on:openRecent={(e) => openRecent(e.detail)}
        on:homeSelect={(e) => selectHomeEntry(e.detail)}
        on:unpin={(e) => { pins = settings.togglePin(pins, e.detail); settings.savePins(pins); }}
        on:unfavorite={(e) => { favorites = favorites.filter((f) => f.path !== e.detail); settings.saveFavorites(favorites); }}
        on:removeRecent={(e) => { recents = settings.removeRecent(recents, e.detail); settings.saveRecents(recents); }}
        on:removeRecentFolder={(e) => { recentFolders = settings.removeRecent(recentFolders, e.detail); settings.saveRecentFolders(recentFolders); }}
        on:clearRecents={() => { recents = []; settings.saveRecents(recents); }}
        on:rowContext={(e) => onRowContext(e.detail, true)}
        on:driveContext={(e) => onDriveContext(e.detail, true)}
        on:homeItemContext={(e) => onHomeItemContext(e.detail, true)}
        on:loadShared={() => loadShared()}
        on:addNetworkLocation={(e) => addNetworkLocation(e.detail)}
        on:removeNetworkLocation={(e) => removeNetworkLocation(e.detail)}
        on:contextEmpty={(e) => (ctx = { x: e.detail.x, y: e.detail.y, target: "empty", inPaneB: true })}
        on:commitRename={(e) => commitRename(e.detail, true)}
        on:drop={(e) => dropInto(e.detail.paths, e.detail.dest, e.detail)}
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
            bind:value={rightWidth}
            on:change={() => {
              rightWidth = clampWidth(rightWidth, RIGHT_MIN, rightMaxWidth());
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
          entry={archive ? archivePreviewEntry : (selectedEntries.length === 1 ? selectedEntries[0] : homePreview)}
          assetUrl={convertFileSrc}
          loadText={loadPreviewText}
          loadEntries={loadArchiveEntries}
          loadInfo={loadPreviewInfo}
          loadImageData={loadImageData}
          loadRawImageData={loadRawImageData}
          loadDicomImageData={loadDicomImageData}
          loadDicomTags={loadDicomTags}
          loadHeicImageData={loadHeicImageData}
          loadPdfValidity={loadPdfValidity}
          saveText={savePreviewText}
          openExternal={async (p) => { unwrap(await commands.openExternal(p)); }}
          extractArchiveHere={extractPreviewHere}
          extractArchiveTo={extractPreviewTo}
          checkArchiveSafety={checkPreviewArchiveSafety}
          on:pick={onFolderPeekPick}
          on:open={onFolderPeekOpen}
        >
          <DetailsPane selected={selectedEntries.length ? selectedEntries : (homePreview ? [homePreview] : [])} {folderName} {itemCount} {folderIcon} />
        </PreviewPane>
      {:else}
        <DetailsPane selected={selectedEntries.length ? selectedEntries : (homePreview ? [homePreview] : [])} {folderName} {itemCount} {folderIcon} />
      {/if}
    </div>
  {/if}
</div>

{#if showTerminal}
  <!-- Docked in normal layout flow (not an overlay), so it takes real space above the status bar and
       pushes the explorer content up — like the other rows in #app's grid (CLAUDE.md STREAMING/dock
       conventions). Mounted only while toggled on, so an unused terminal is zero cost. -->
  <TerminalPanel cwd={isHome || archive ? "" : currentPath} on:close={() => (showTerminal = false)} />
{/if}

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
    canPaste={ctxPasteCheck.allowed}
    selectionCount={selectedCount(ctxPane.selection)}
    folderSelected={ctxPane.selectedEntries.length === 1 && ctxPane.selectedEntries[0]?.is_dir}
    executableSelected={ctxPane.selectedEntries.length === 1 && isExecutable(ctxPane.selectedEntries[0])}
    openIcon={ctxPane.selectedEntries.length === 1 ? iconFor(ctxPane.selectedEntries[0]) : "folder"}
    pinned={ctxPane.selectedEntries.length === 1 && pins.includes(ctxPane.selectedEntries[0].path)}
    favorited={ctxPane.selectedEntries.length === 1 && favorites.some((f) => f.path === ctxPane.selectedEntries[0].path)}
    extractable={(ctxInPaneB ? paneBPath !== HOME : (!isHome && !archive)) && ctxPane.selectedEntries.length === 1 && isExtractable(ctxPane.selectedEntries[0])}
    archiveSafetyEligible={(ctxInPaneB ? paneBPath !== HOME : (!isHome && !archive)) && ctxPane.selectedEntries.length === 1 && isArchiveSafetyEligible(ctxPane.selectedEntries[0])}
    compressible={(ctxInPaneB ? paneBPath !== HOME : (!isHome && !archive)) && ctxPane.selectedEntries.length >= 1}
    comparable={!ctxInPaneB && !isHome && !archive && ctxPane.selectedEntries.length === 2 && ctxPane.selectedEntries.every((e) => !e.is_dir)}
    mediaEligible={ctxPane.selectedEntries.length > 1 && ctxPane.selectedEntries.some((e) => !e.is_dir && canBatchTransform(e.name))}
    canTerminal={!ctxInPaneB && !isHome && !archive}
    copyMoveEligible={ctxInPaneB ? paneBPath !== HOME : (!isHome && !archive)}
    sameTypeExt={ctxPane.selectedEntries.length === 1 && !ctxPane.selectedEntries[0].is_dir ? ctxPane.selectedEntries[0].extension : ""}
    shreddable={(ctxInPaneB ? paneBPath !== HOME : (!isHome && !archive)) && ctxPane.selectedEntries.length >= 1 && ctxPane.selectedEntries.every((e) => !e.is_dir)}
    vaultable={(ctxInPaneB ? paneBPath !== HOME : (!isHome && !archive)) && ctxPane.selectedEntries.length === 1 && ctxPane.selectedEntries[0]?.is_dir}
    certFileKind={ctxPane.selectedEntries.length === 1 ? certKindOf(ctxPane.selectedEntries[0]) : ""}
    jwtSelected={ctxPane.selectedEntries.length === 1 && isJwtFile(ctxPane.selectedEntries[0])}
    certCreateEligible={ctxInPaneB ? paneBPath !== HOME : (!isHome && !archive)}
    splitEligible={ctxPane.selectedEntries.length === 1 && canSplitFile(ctxPane.selectedEntries[0])}
    joinEligible={ctxPane.selectedEntries.length === 1 && canJoinFile(ctxPane.selectedEntries[0])}
    {view}
    {sortKey}
    {sortDir}
    canUndo={canUndo(undoStack)}
    undoLabel={peekLabel(undoStack)}
    homeView={ctx.target === "home-item" ? homeCtxView : ""}
    homeKind={ctx.target === "home-item" ? homeCtxKind : ""}
    homeIsDir={homeCtxIsDir}
    homeStale={homeCtxStale}
    macros={ctx.target === "item" ? macroContextNames : []}
    userCommands={ctx.target === "item" ? userCommandsContext : []}
    linkBroken={ctx.target === "item" ? ctxLinkBroken : false}
    driveEjectable={ctx.target === "drive" && !!driveRemovable[driveCtxPath]}
    on:action={(e) => runAction(e.detail)}
    on:close={() => (ctx = null)}
  />
{/if}

{#if newLinkDialogFor}
  <NewLinkDialog
    targetDir={newLinkDialogFor}
    on:created={(e) => onNewLinkCreated(e.detail)}
    on:error={(e) => onNewLinkError(e.detail)}
    on:close={() => (newLinkDialogFor = null)}
  />
{/if}

{#if repairLinkFor}
  <RepairLinkDialog
    linkPath={repairLinkFor.path}
    linkName={repairLinkFor.name}
    searchRoots={[currentPath]}
    on:repaired={(e) => onLinkRepaired(e.detail)}
    on:error={(e) => onLinkRepairError(e.detail)}
    on:close={() => (repairLinkFor = null)}
  />
{/if}

{#if shredConfirmFor}
  <ShredConfirmDialog
    paths={shredConfirmFor.paths}
    what={shredConfirmFor.what}
    on:done={(e) => onShredDone(e.detail)}
    on:error={(e) => onShredError(e.detail)}
    on:close={() => (shredConfirmFor = null)}
  />
{/if}

{#if vaultCreateFor}
  <VaultCreateDialog
    folderPath={vaultCreateFor.folderPath}
    folderName={vaultCreateFor.folderName}
    rememberDefault={settings.loadVaultRememberPassphrases()}
    on:created={(e) => onVaultCreated(e.detail)}
    on:error={(e) => showNotice(e.detail, true)}
    on:close={() => (vaultCreateFor = null)}
  />
{/if}

{#if archiveSafetyFor}
  <ArchiveSafetyDialog
    path={archiveSafetyFor}
    on:close={() => (archiveSafetyFor = null)}
  />
{/if}

{#if certCreateFor}
  <CreateCertDialog
    outDir={certCreateFor.outDir}
    on:created={(e) => onCertCreated(e.detail)}
    on:error={(e) => showNotice(e.detail, true)}
    on:close={() => (certCreateFor = null)}
  />
{/if}

{#if certSignFor}
  <SignCertDialog
    csrPath={certSignFor.csrPath}
    caCertPath={certSignFor.caCertPath}
    outDir={certSignFor.dir}
    on:created={(e) => onCertSigned(e.detail)}
    on:error={(e) => showNotice(e.detail, true)}
    on:close={() => (certSignFor = null)}
  />
{/if}

{#if cryptoInspectFor}
  <InspectCryptoDialog
    path={cryptoInspectFor.path}
    kind={cryptoInspectFor.kind}
    on:close={() => (cryptoInspectFor = null)}
  />
{/if}

{#if splitFileFor}
  <SplitFileDialog
    path={splitFileFor.path}
    on:split={(e) => onSplitFileDone(e.detail)}
    on:error={(e) => showNotice(e.detail, true)}
    on:close={() => (splitFileFor = null)}
  />
{/if}

{#if joinPartsFor}
  <JoinPartsDialog
    path={joinPartsFor.path}
    on:joined={(e) => onJoinPartsDone(e.detail)}
    on:error={(e) => showNotice(e.detail, true)}
    on:close={() => (joinPartsFor = null)}
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

{#if passwordPrompt}
  <!-- `{#key passwordPrompt}` remounts the dialog on every (re)prompt — each prompt assigns a fresh object
       reference (vault unlock, archive extract, compress), so a wrong-password re-prompt starts clean:
       empty field, re-fired auto-focus, and a re-armed submit guard (CPE-1249 review #3 + #B). -->
  {#key passwordPrompt}
    <PasswordPromptDialog
      title={passwordPrompt.title}
      message={passwordPrompt.message}
      confirmLabel={passwordPrompt.confirmLabel}
      error={passwordPrompt.error}
      on:submit={(e) => passwordPrompt?.onSubmit(e.detail)}
      on:cancel={() => (passwordPrompt = null)}
    />
  {/key}
{/if}

{#if activeWatchCwd && showTimeline}
  <AgentTimeline
    entries={$agentTimeline}
    agentName={watchedAgentName}
    sessionId={watchedSessionId}
    sessions={$agentSessions}
    {currentPath}
    on:navigate={(e) => navigate(e.detail)}
    on:close={() => (showTimeline = false)}
    on:replayOverlay={(e) => (replayOverlayEntries = e.detail)}
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

<!-- Automation test-mode badge (CPE-1046): only rendered when launched with `--test-mode`. -->
{#if testMode}
  <TestModeOverlay />
{/if}

{#if batchRenameFor}
  <BatchRenameDialog
    names={batchRenameFor.entries.map((e) => e.name)}
    on:apply={(e) => applyBatchRename(e.detail)}
    on:cancel={() => (batchRenameFor = null)}
  />
{/if}

{#if batchMediaFor}
  <BatchMediaDialog
    paths={batchMediaFor.entries.map((e) => e.path)}
    on:apply={(e) =>
      applyBatchMedia(e.detail.report, e.detail.checkpointFailures ?? [], e.detail.checkpointPartial ?? [])}
    on:cancel={(e) => {
      batchMediaFor = null;
      noticeCheckpointFailures(e.detail?.checkpointFailures ?? [], e.detail?.checkpointPartial ?? []);
    }}
  />
{/if}

{#if propsFor}
  <PropertiesDialog entries={propsFor} on:close={() => (propsFor = null)} />
{/if}

{#if studioFor}
  <MetadataStudioDialog entries={studioFor} on:close={() => (studioFor = null)} />
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
    on:close={() => { showSettings = false; navigationModeEnabled = settings.loadNavigationModeEnabled(); }}
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

{#if instantSearchOpen}
  <InstantSearch
    root={isHome ? "" : currentPath}
    on:help={() => openDocsSlug("12-search")}
    on:navigate={(e) => { instantSearchOpen = false; revealFileInApp(e.detail); }}
    on:close={() => (instantSearchOpen = false)}
  />
{/if}

{#if contentIndexSearchOpen}
  <ContentIndexSearchDialog
    root={currentPath}
    on:help={() => openDocsSlug("12-search")}
    on:navigate={(e) => { contentIndexSearchOpen = false; revealFileInApp(e.detail); }}
    on:close={() => (contentIndexSearchOpen = false)}
  />
{/if}

{#if copilotOpen}
  <CopilotDialog
    root={isHome || archive ? "" : currentPath}
    on:help={() => openDocsSlug("21-ai-copilot")}
    on:applied={() => refresh()}
    on:reverted={() => refresh()}
    on:openSettings={() => { copilotOpen = false; showSettings = true; }}
    on:close={() => (copilotOpen = false)}
  />
{/if}

{#if duplicatesOpen}
  <DuplicatesDialog
    root={currentPath}
    on:navigate={(e) => { duplicatesOpen = false; revealFileInApp(e.detail); }}
    on:close={() => (duplicatesOpen = false)}
  />
{/if}

{#if similarImagesOpen}
  <SimilarImagesDialog
    root={currentPath}
    on:navigate={(e) => { similarImagesOpen = false; revealFileInApp(e.detail); }}
    on:close={() => (similarImagesOpen = false)}
  />
{/if}

{#if similarDocsOpen}
  <NearDuplicatesDialog
    root={currentPath}
    kind="documents"
    on:navigate={(e) => { similarDocsOpen = false; revealFileInApp(e.detail); }}
    on:close={() => (similarDocsOpen = false)}
  />
{/if}

{#if similarFoldersOpen}
  <NearDuplicatesDialog
    root={currentPath}
    kind="folders"
    on:navigate={(e) => { similarFoldersOpen = false; revealFileInApp(e.detail); }}
    on:close={() => (similarFoldersOpen = false)}
  />
{/if}

{#if fileHealthOpen}
  <FileHealthDialog
    root={currentPath}
    initialTab={fileHealthTab}
    openNonce={fileHealthNonce}
    on:navigate={(e) => { fileHealthOpen = false; revealFileInApp(e.detail); }}
    on:close={() => (fileHealthOpen = false)}
  />
{/if}

{#if declutterOpen}
  <DeclutterDialog
    root={currentPath}
    on:navigate={(e) => { declutterOpen = false; revealFileInApp(e.detail); }}
    on:close={() => (declutterOpen = false)}
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

{#if showTrash}
  <TrashView on:help={(e) => openDocs(e.detail)} on:close={() => (showTrash = false)} />
{/if}

{#if showDocs}
  <DocsView initialSlug={docsSlug} on:close={() => (showDocs = false)} />
{/if}

<TransferPanel />
<DropStackPanel
  canTransfer={!dropStackDestBlocked}
  on:moveAll={doDropStackMoveAll}
  on:copyAll={doDropStackCopyAll}
/>

{#if quickLook}
  <QuickLook
    images={quickLook.images}
    index={quickLook.index}
    on:prev={() => quickLookMove(-1)}
    on:next={() => quickLookMove(1)}
    on:close={() => (quickLook = null)}
  />
{/if}

{#if mediaQuickLook && mediaQuickLook.playlist}
  {@const pl = mediaQuickLook.playlist}
  {@const cur = pl.current()}
  {#if cur}
    <MediaQuickLook
      track={cur}
      position={pl.position}
      count={pl.length}
      repeat={pl.repeat}
      shuffled={pl.isShuffled}
      assetUrl={convertFileSrc}
      openExternal={async (p) => { unwrap(await commands.openExternal(p)); }}
      on:prev={() => mediaQuickLookStep(-1)}
      on:next={() => mediaQuickLookStep(1)}
      on:cycleRepeat={mediaQuickLookRepeat}
      on:toggleShuffle={mediaQuickLookShuffle}
      on:close={() => (mediaQuickLook = null)}
    />
  {/if}
{/if}

{#if pendingCopy}
  <TransferConflictDialog
    count={pendingCopy.count}
    on:choose={(e) => resolveCopyConflict(e.detail)}
    on:cancel={() => (pendingCopy = null)}
  />
{/if}

{#if pendingDropStackCopy}
  <TransferConflictDialog
    count={pendingDropStackCopy.count}
    on:choose={(e) => resolveDropStackCopyConflict(e.detail)}
    on:cancel={() => (pendingDropStackCopy = null)}
  />
{/if}

{#if paletteOpen}
  <CommandPalette commands={paletteCommands} on:close={() => (paletteOpen = false)} />
{/if}

<!-- Navigation Mode (CPE-1556, epic CPE-1487): all three surfaces are mounted ONLY when the opt-in
     Settings toggle is on, so a fresh install renders none of them (zero behavior change with the mode
     off). The mode badge always shows the current NORMAL/VISUAL state + pending chord/count; the `:`
     command line mounts on the `startCommand` intent and dispatches the chosen palette Command; the
     cheatsheet opens on `?` in the modal layer. -->
{#if navigationModeEnabled}
  <NavModeIndicator mode={navState.mode} pendingCount={navState.pendingCount} pendingChord={navState.pendingChord} />
  {#if navCommandLineOpen}
    <div class="nav-command-line-anchor">
      <NavCommandLine
        commands={paletteCommands}
        on:run={(e) => { navCommandLineOpen = false; e.detail.run(); }}
        on:cancel={() => (navCommandLineOpen = false)}
      />
    </div>
  {/if}
  <NavCheatsheet open={navCheatsheetOpen} on:close={() => (navCheatsheetOpen = false)} />
{/if}

{#if spotlightOpen}
  <Spotlight
    root={isHome || archive ? "" : currentPath}
    paletteCommands={paletteCommands}
    places={[...places, ...drives]}
    favorites={favorites}
    history={activeTab.history}
    on:close={() => (spotlightOpen = false)}
    on:activate={(e) => onSpotlightActivate(e.detail)}
  />
{/if}

{#if showUserCommands}
  <UserCommandsDialog
    commands={userCommands}
    on:change={(e) => persistUserCommands(e.detail)}
    on:close={() => (showUserCommands = false)}
  />
{/if}

{#if runConfirm}
  <RunCommandConfirm
    title={runConfirm.title}
    commands={runConfirm.commands}
    cwd={runConfirm.cwd}
    on:close={() => (runConfirm = null)}
  />
{/if}

{#if macrosOpen}
  <MacrosDialog
    bindings={macroBindings}
    on:bindingschange={(e) => persistMacroBindings(e.detail)}
    on:changed={() => refreshMacroSummaries()}
    on:close={() => (macrosOpen = false)}
  />
{/if}

{#if macroParamPromptFor}
  <MacroParamPrompt
    title="Macro parameters — {macroParamPromptFor.macro.name}"
    labels={macroParamPromptFor.labels}
    on:submit={(e) => submitMacroParams(e.detail)}
    on:cancel={() => (macroParamPromptFor = null)}
  />
{/if}

{#if macroRunConfirmFor}
  <MacroRunConfirm
    macro={macroRunConfirmFor.macro}
    inputs={macroRunConfirmFor.inputs}
    root={macroRunConfirmFor.root}
    on:ran={() => refresh()}
    on:close={() => (macroRunConfirmFor = null)}
  />
{/if}

{#if agentMenu}
  <AgentMenu
    x={agentMenu.x}
    y={agentMenu.y}
    label={agentMenu.label}
    sessionId={agentMenu.sessionId}
    sessionLabel={agentMenu.sessionLabel}
    on:confirm={confirmCloseAllConsoles}
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
    canMoveUp={$smartFolders.findIndex((sf) => sf.id === smartFolderMenu?.id) > 0}
    canMoveDown={(() => { const i = $smartFolders.findIndex((sf) => sf.id === smartFolderMenu?.id); return i !== -1 && i < $smartFolders.length - 1; })()}
    on:rename={(e) => { renameSmartSaved(smartFolderMenu?.id ?? "", e.detail); if (smartFolder && smartFolder.id === smartFolderMenu?.id) smartFolder = { ...smartFolder, name: e.detail }; smartFolderMenu = null; }}
    on:remove={() => { const id = smartFolderMenu?.id ?? ""; if (smartFolder?.id === id) exitSmartFolder(); removeSmartSaved(id); smartFolderMenu = null; }}
    on:moveUp={() => moveSmartSaved(smartFolderMenu?.id ?? "", -1)}
    on:moveDown={() => moveSmartSaved(smartFolderMenu?.id ?? "", 1)}
    on:close={() => (smartFolderMenu = null)}
  />
{/if}

{#if networkForm}
  <NetworkConnectionForm
    x={networkForm.x}
    y={networkForm.y}
    editing={networkForm.editing}
    initial={networkForm.editing
      ? formFromConnection(networkForm.editing)
      : (networkForm.prefill ?? blankConnectionForm())}
    on:save={(e) => void saveNetworkConnection(e.detail)}
    on:close={() => (networkForm = null)}
  />
{/if}

{#if networkContextMenu}
  {@const ctxConn = networkContextMenu.conn}
  {@const ctxX = networkContextMenu.x}
  {@const ctxY = networkContextMenu.y}
  <NetworkConnectionMenu
    x={ctxX}
    y={ctxY}
    name={ctxConn.name}
    state={connectionStates[ctxConn.name] ?? "disconnected"}
    on:connect={() => void onNetworkConnect(ctxConn)}
    on:disconnect={() => disconnectNetworkConnection(ctxConn.name)}
    on:edit={() => (networkForm = { x: ctxX, y: ctxY, editing: ctxConn, prefill: null })}
    on:forget={() => void forgetNetworkConnection(ctxConn)}
    on:close={() => (networkContextMenu = null)}
  />
{/if}

{#if networkSecretPrompt}
  {@const promptConn = networkSecretPrompt.conn}
  <NetworkSecretPrompt
    x={networkSecretPrompt.x}
    y={networkSecretPrompt.y}
    name={promptConn.name}
    label={promptConn.auth.kind === "key" ? "Passphrase" : "Password"}
    on:submit={(e) => void submitNetworkSecret(promptConn, e.detail.secret, e.detail.remember)}
    on:close={() => (networkSecretPrompt = null)}
  />
{/if}

{#if structuredSearchMenu}
  <!-- Reuses SmartFolderMenu (CPE-1229) — it's already a generic name-based rename/delete popover, not
       tag-specific, so a saved structured search's rename/delete needs no new menu component. -->
  <SmartFolderMenu
    x={structuredSearchMenu.x}
    y={structuredSearchMenu.y}
    name={structuredSearchMenu.name}
    canMoveUp={$savedSearches.findIndex((s) => s.id === structuredSearchMenu?.id) > 0}
    canMoveDown={(() => { const i = $savedSearches.findIndex((s) => s.id === structuredSearchMenu?.id); return i !== -1 && i < $savedSearches.length - 1; })()}
    on:rename={(e) => { renameSavedSearch(structuredSearchMenu?.id ?? "", e.detail); if (structuredSearch && structuredSearch.id === structuredSearchMenu?.id) structuredSearch = { ...structuredSearch, name: e.detail }; structuredSearchMenu = null; }}
    on:remove={() => { const id = structuredSearchMenu?.id ?? ""; if (structuredSearch?.id === id) exitStructuredSearch(); removeSavedSearch(id); structuredSearchMenu = null; }}
    on:moveUp={() => moveSavedSearch(structuredSearchMenu?.id ?? "", -1)}
    on:moveDown={() => moveSavedSearch(structuredSearchMenu?.id ?? "", 1)}
    on:close={() => (structuredSearchMenu = null)}
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
    assetUrl={convertFileSrc}
    on:cancel={() => (compareOpen = false)}
  />
{/if}

{#if selectByOpen}
  <SelectByDialog
    autoReveal={selectByAutoSave}
    on:submit={(e) => applySelectBy(e.detail)}
    on:save={(e) => saveCurrentSearch(e.detail)}
    on:cancel={() => { selectByOpen = false; selectByAutoSave = false; }}
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

{#if templatesOpen}
  <TemplatesDialog
    path={isHome || archive ? "" : currentPath}
    on:stamped={() => refresh()}
    on:close={() => (templatesOpen = false)}
  />
{/if}

{#if checkpointOpen}
  <CheckpointDialog
    initialPath={isHome || archive ? "" : currentPath}
    on:reverted={() => refresh()}
    on:help={() => openDocsSlug("16-checkpoints")}
    on:cancel={() => (checkpointOpen = false)}
  />
{/if}

{#if organizeOpen}
  <OrganizeDialog
    path={isHome || archive ? "" : currentPath}
    on:applied={() => refresh()}
    on:undo={() => { organizeOpen = false; checkpointOpen = true; }}
    on:help={() => openDocsSlug("03-explorer")}
    on:cancel={() => (organizeOpen = false)}
  />
{/if}

{#if columnPickerOpen}
  <!-- Column picker (CPE-1146, epic CPE-707): every change (add/remove/reorder) is persisted per-folder
       immediately, so there's nothing to "cancel" — the dialog just closes. `columnPickerInPaneB`
       (CPE-1388, captured at open time by each pane's `on:openColumnPicker` handler) routes both the
       initial `active` set and the save target to whichever pane actually opened the dialog — mirroring
       CPE-1382's per-pane READ fix on the WRITE side too, instead of always editing pane A. -->
  <ColumnPickerDialog
    available={$metaColumnCatalog}
    active={columnPickerInPaneB ? activeMetaColumnsB : activeMetaColumns}
    on:change={(e) => {
      if (columnPickerInPaneB) { activeMetaColumnsB = e.detail; settings.saveMetaColumnsForFolder(paneBPath, activeMetaColumnsB); }
      else { activeMetaColumns = e.detail; settings.saveMetaColumnsForFolder(currentPath, activeMetaColumns); }
    }}
    on:close={() => (columnPickerOpen = false)}
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
  /* Navigation Mode (CPE-1556): dock the `:` command line at the bottom-centre of the window, above
     everything, so it reads like a vim status-line prompt. Width-capped + centred; the component owns
     its own surface/border/shadow (all theme tokens). */
  .nav-command-line-anchor {
    position: fixed;
    left: 50%;
    bottom: 40px;
    transform: translateX(-50%);
    width: min(560px, 92vw);
    z-index: 210;
  }

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
