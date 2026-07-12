<script lang="ts">
  import { onMount } from "svelte";
  import { invoke, convertFileSrc } from "@tauri-apps/api/core";
  import { check } from "@tauri-apps/plugin-updater";
  import { relaunch } from "@tauri-apps/plugin-process";
  import { openPath } from "@tauri-apps/plugin-opener";

  import TabBar from "./lib/components/TabBar.svelte";
  import NavToolbar from "./lib/components/NavToolbar.svelte";
  import CommandBar from "./lib/components/CommandBar.svelte";
  import Sidebar from "./lib/components/Sidebar.svelte";
  import FileList from "./lib/components/FileList.svelte";
  import HomeView from "./lib/components/HomeView.svelte";
  import DetailsPane from "./lib/components/DetailsPane.svelte";
  import PreviewPane from "./lib/components/PreviewPane.svelte";
  import type { ArchiveEntry } from "./lib/preview/provider";
  import StatusBar from "./lib/components/StatusBar.svelte";
  import ContextMenu from "./lib/components/ContextMenu.svelte";
  import ConfirmDialog from "./lib/components/ConfirmDialog.svelte";
  import PropertiesDialog from "./lib/components/PropertiesDialog.svelte";

  import { friendlyError, splitPath, formatPathsForClipboard } from "./lib/format";
  import { sortEntries } from "./lib/sort";
  import { uniqueName } from "./lib/naming";
  import { validateFileName } from "./lib/filename";
  import { matchesQuery } from "./lib/search";
  import { firstMatchIndex } from "./lib/typeahead";
  import { clampWidth } from "./lib/resize";
  import {
    createHistory, visit, back, forward, canGoBack, canGoForward, current,
    type History,
  } from "./lib/history";
  import {
    emptySelection, click as selClick, selectOnly, selectAll, moveLead,
    selectedIndices, selectedCount, remapByPath, type Selection,
  } from "./lib/selection";
  import {
    emptyClipboard, stage, isEmpty as clipEmpty, canPaste as clipCanPaste,
    type Clipboard,
  } from "./lib/clipboard";
  import * as settings from "./lib/settings";
  import {
    pushUndo, popUndo, canUndo, peekLabel, invert, deletedPaths, type UndoEntry,
  } from "./lib/undo";
  import type { DirEntry, Place, SortKey, SortDir, ViewMode, RecentFile } from "./lib/types";

  interface OpResult { path: string; ok: boolean; error: string }

  const HOME = " home"; // sentinel: the Home view, not a filesystem path

  interface Tab { id: number; history: History }

  let nextTabId = 2;
  let tabs: Tab[] = [{ id: 1, history: createHistory(HOME) }];
  let activeId = 1;

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
  let search = "";
  let editingPath = false;

  let renamingPath = "";
  let renameValue = "";
  /** Path of a freshly-created folder, so we can auto-rename it once listed. */
  let pendingRenamePath = "";

  let undoStack: UndoEntry[] = [];
  /** Whether THIS platform can restore from the trash (false on macOS). */
  let canRestoreTrash = false;
  /** Paths currently being dragged, shared with the sidebar as a drop target. */
  let draggedPaths: string[] = [];
  let ctx: { x: number; y: number; target: "item" | "empty" } | null = null;
  let confirm: { title: string; message: string; label: string; onYes: () => void } | null = null;
  let propsFor: DirEntry[] | null = null;

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
      entries = await invoke<DirEntry[]>("list_dir", { path });
    } catch (e) {
      entries = [];
      error = friendlyError(String(e));
    } finally {
      loading = false;
    }

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
  }

  async function navigate(path: string) {
    setHistory(visit(activeTab.history, path));
    await loadPath(path);
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
    if (isHome) return;
    try {
      const parent = await invoke<string | null>("parent_dir", { path: currentPath });
      await navigate(parent ?? HOME);
    } catch {
      await navigate(HOME);
    }
  }

  async function refresh() {
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
    if (entry.is_dir) {
      await navigate(entry.path);
      return;
    }
    try {
      await openPath(entry.path);
      recents = settings.addRecent(recents, { path: entry.path, name: entry.name });
      settings.saveRecents(recents);
    } catch (e) {
      console.debug("openPath failed:", e);
      showNotice(`Can't open "${entry.name}" — no app is associated with this file type.`, true);
    }
  }

  async function openRecent(path: string) {
    try {
      await openPath(path);
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
    tabs = tabs.filter((t) => t.id !== id);
    if (activeId === id) {
      const fallback = tabs[Math.max(0, idx - 1)];
      activeId = fallback.id;
      loadPath(current(fallback.history) ?? HOME);
    }
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
  $: folderName = isHome ? "Home" : (splitPath(currentPath).at(-1)?.name ?? currentPath);
  $: searching = search.trim().length > 0;

  $: shown = entries.filter((e) => showHidden || !e.hidden);

  $: filtered = searching
    ? shown.filter((e) => matchesQuery(e.name, search))
    : shown;

  $: visible = sortEntries(filtered, sortKey, sortDir);

  $: crumbs = isHome
    ? [{ name: "Home", path: HOME }]
    : [{ name: "Home", path: HOME }, ...splitPath(currentPath)];

  $: selectedEntries = selectedIndices(selection)
    .map((i) => visible[i])
    .filter(Boolean);

  $: selectedSize = selectedEntries.reduce((n, e) => n + (e.is_dir ? 0 : e.size), 0);
  $: itemCount = isHome ? places.length + drives.length + pins.length : visible.length;
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
    renamingPath = entry.path;
    renameValue = entry.name;
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
    if (isHome) return;
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

  function doCopy() {
    if (selectedEntries.length === 0) return;
    clipboard = stage(selectedEntries.map((e) => e.path), "copy");
    showNotice(`Copied ${clipboard.paths.length} item${clipboard.paths.length === 1 ? "" : "s"}.`);
  }

  function doCut() {
    if (selectedEntries.length === 0) return;
    clipboard = stage(selectedEntries.map((e) => e.path), "cut");
    showNotice(`Cut ${clipboard.paths.length} item${clipboard.paths.length === 1 ? "" : "s"}.`);
  }

  async function doPaste() {
    if (isHome || clipEmpty(clipboard)) return;
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

  /** Duplicate the selection in place — copy it into the folder it lives in.
      Not undoable, for the same reason a copy-paste isn't (see doPaste). */
  async function doDuplicate() {
    if (isHome || selectedEntries.length === 0) return;
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
    if (selectedEntries.length === 0) return;
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

  // ---- context menu / command dispatch ----
  function runAction(action: string) {
    switch (action) {
      case "open": if (selectedEntries[0]) open(selectedEntries[0]); break;
      case "open-new-tab": if (selectedEntries[0]) openInNewTab(selectedEntries[0]); break;
      case "cut": doCut(); break;
      case "copy": doCopy(); break;
      case "paste": doPaste(); break;
      case "duplicate": doDuplicate(); break;
      case "copy-path": doCopyPath(); break;
      case "rename": if (selectedEntries.length === 1) beginRename(selectedEntries[0]); break;
      case "delete": askDelete(false); break;
      case "properties": openProperties(); break;
      case "new-folder": newFolder(); break;
      case "select-all": selection = selectAll(visible.length); break;
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
    if (ctrl && event.key.toLowerCase() === "t") { event.preventDefault(); newTab(); return; }
    if (ctrl && event.key.toLowerCase() === "w") { event.preventDefault(); closeTab(activeId); return; }
    if (ctrl && event.key === "Tab") { event.preventDefault(); cycleTab(event.shiftKey ? -1 : 1); return; }
    if (ctrl && event.key.toLowerCase() === "a") { event.preventDefault(); selection = selectAll(visible.length); return; }
    if (ctrl && event.shiftKey && event.key.toLowerCase() === "c") { event.preventDefault(); doCopyPath(); return; }
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

  async function checkForUpdates() {
    try {
      const update = await check();
      if (update) {
        showNotice(`Update ${update.version} available — downloading…`);
        await update.downloadAndInstall();
        await relaunch();
      }
    } catch (e) {
      console.debug("update check skipped:", e);
    }
  }

  onMount(async () => {
    view = settings.loadView();
    showHidden = settings.loadShowHidden();
    sortKey = settings.loadSortKey();
    sortDir = settings.loadSortDir();
    showDetails = settings.loadShowDetails();
    showPreview = settings.loadShowPreview();
    sidebarWidth = clampWidth(settings.loadSidebarWidth(), SIDEBAR_MIN, SIDEBAR_MAX);
    rightWidth = clampWidth(settings.loadRightWidth(), RIGHT_MIN, RIGHT_MAX);
    pins = settings.loadPins();
    recents = settings.loadRecents();

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
    } catch (e) {
      console.debug("could not load places:", e);
    }
    await loadPath(HOME);
    checkForUpdates();
  });
</script>

<svelte:window on:keydown={handleKeydown} />

<TabBar
  tabs={tabList}
  {activeId}
  on:select={(e) => selectTab(e.detail)}
  on:close={(e) => closeTab(e.detail)}
  on:new={newTab}
/>

<NavToolbar
  bind:this={navToolbar}
  bind:editingPath
  {crumbs}
  {currentPath}
  canBack={canGoBack(activeTab.history)}
  canForward={canGoForward(activeTab.history)}
  {search}
  searchScope={folderName}
  on:back={goBack}
  on:forward={goForward}
  on:up={goUp}
  on:refresh={refresh}
  on:navigate={(e) => (e.detail === HOME || e.detail.startsWith(" ") ? navigate(e.detail) : navigateToTyped(e.detail))}
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
  on:action={(e) => runAction(e.detail)}
  on:sort={(e) => {
    sortKey = e.detail.key; sortDir = e.detail.dir;
    settings.saveSortKey(sortKey); settings.saveSortDir(sortDir);
  }}
  on:view={(e) => { view = e.detail; settings.saveView(view); }}
  on:toggleHidden={() => { showHidden = !showHidden; settings.saveShowHidden(showHidden); }}
  on:toggleDetails={() => { showDetails = !showDetails; settings.saveShowDetails(showDetails); }}
/>

<div
  class="main"
  class:with-details={showDetails}
  class:resizing
  style="grid-template-columns: {gridCols}"
>
  <Sidebar
    {places}
    {drives}
    {currentPath}
    {isHome}
    {draggedPaths}
    on:navigate={(e) => navigate(e.detail)}
    on:home={() => navigate(HOME)}
    on:drop={(e) => dropInto(e.detail.paths, e.detail.dest, e.detail.copy)}
  />

  <!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
  <div
    class="resizer"
    role="separator"
    aria-orientation="vertical"
    aria-label="Resize sidebar"
    title="Drag to resize"
    on:mousedown={(e) => startResize("left", e)}
  ></div>

  <!-- File List Pane (middle column) -->
  <div class="filelist-pane" role="region" aria-label="File list">
    {#if isHome}
      <HomeView
        {places}
        {drives}
        {pins}
        {recents}
        on:navigate={(e) => navigate(e.detail)}
        on:openFile={(e) => openRecent(e.detail)}
        on:unpin={(e) => { pins = settings.togglePin(pins, e.detail); settings.savePins(pins); }}
        on:clearRecents={() => { recents = []; settings.saveRecents(recents); }}
      />
    {:else}
      <FileList
        entries={visible}
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

  {#if showDetails}
    <!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
    <div
      class="resizer"
      role="separator"
      aria-orientation="vertical"
      aria-label="Resize details pane"
      title="Drag to resize"
      on:mousedown={(e) => startResize("right", e)}
    ></div>

    <div class="preview-pane">
      <div class="preview-pane-toggle" role="tablist" aria-label="Preview or details">
        <button
          role="tab"
          class:active={showPreview}
          aria-selected={showPreview}
          on:click={() => { showPreview = true; settings.saveShowPreview(true); }}
        >Preview</button>
        <button
          role="tab"
          class:active={!showPreview}
          aria-selected={!showPreview}
          on:click={() => { showPreview = false; settings.saveShowPreview(false); }}
        >Details</button>
      </div>

      {#if showPreview}
        <PreviewPane
          entry={selectedEntries.length === 1 ? selectedEntries[0] : null}
          assetUrl={convertFileSrc}
          loadText={loadPreviewText}
          loadEntries={loadArchiveEntries}
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
  selectedCount={selectedCount(selection)}
  {selectedSize}
  filtered={searching}
  hiddenShown={showHidden}
  {notice}
  {noticeIsError}
/>

{#if ctx}
  <ContextMenu
    x={ctx.x}
    y={ctx.y}
    target={ctx.target}
    canPaste={pasteCheck.allowed}
    selectionCount={selectedCount(selection)}
    folderSelected={selectedEntries.length === 1 && selectedEntries[0]?.is_dir}
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

{#if propsFor}
  <PropertiesDialog entries={propsFor} on:close={() => (propsFor = null)} />
{/if}
