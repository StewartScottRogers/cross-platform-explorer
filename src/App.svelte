<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
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
  import StatusBar from "./lib/components/StatusBar.svelte";

  import { friendlyError, splitPath } from "./lib/format";
  import { typeName } from "./lib/filetypes";
  import {
    createHistory,
    visit,
    back,
    forward,
    canGoBack,
    canGoForward,
    current,
    type History,
  } from "./lib/history";
  import type { DirEntry, Place, SortKey, SortDir } from "./lib/types";

  const HOME = " home"; // sentinel: the Home view, not a filesystem path

  interface Tab {
    id: number;
    history: History;
  }

  let nextTabId = 2;
  let tabs: Tab[] = [{ id: 1, history: createHistory(HOME) }];
  let activeId = 1;

  let entries: DirEntry[] = [];
  let places: Place[] = [];
  let drives: Place[] = [];

  let error = "";
  let loading = false;
  let notice = "";
  let noticeTimer: ReturnType<typeof setTimeout> | undefined;

  let selected = -1;
  let rowEls: HTMLElement[] = [];

  let sortKey: SortKey = "name";
  let sortDir: SortDir = "asc";
  let showDetails = true;
  let search = "";

  $: activeTab = tabs.find((t) => t.id === activeId) as Tab;
  $: currentPath = current(activeTab.history) ?? HOME;
  $: isHome = currentPath === HOME;

  function showNotice(message: string) {
    notice = message;
    clearTimeout(noticeTimer);
    noticeTimer = setTimeout(() => (notice = ""), 4000);
  }

  /** Replace the active tab's history (Svelte needs a new array to react). */
  function setHistory(h: History) {
    tabs = tabs.map((t) => (t.id === activeId ? { ...t, history: h } : t));
  }

  async function loadPath(path: string) {
    selected = -1;
    search = "";
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
  }

  /** Navigate the active tab to a path (or Home) and record it in history. */
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
      const parent = await invoke<string | null>("parent_dir", {
        path: currentPath,
      });
      // At a drive/filesystem root there is no parent — fall back to Home.
      await navigate(parent ?? HOME);
    } catch {
      await navigate(HOME);
    }
  }

  async function refresh() {
    await loadPath(currentPath);
  }

  async function open(entry: DirEntry) {
    if (entry.is_dir) {
      await navigate(entry.path);
      return;
    }
    try {
      await openPath(entry.path);
    } catch (e) {
      console.debug("openPath failed:", e);
      showNotice(
        `Can't open "${entry.name}" — no app is associated with this file type.`,
      );
    }
  }

  function openSelected() {
    if (selected >= 0 && selected < visible.length) open(visible[selected]);
  }

  // --- tabs ---
  function newTab() {
    const tab: Tab = { id: nextTabId++, history: createHistory(HOME) };
    tabs = [...tabs, tab];
    activeId = tab.id;
    loadPath(HOME);
  }

  function closeTab(id: number) {
    if (tabs.length === 1) return; // never close the last tab
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

  // --- derived listing ---
  $: folderName = isHome
    ? "Home"
    : (splitPath(currentPath).at(-1)?.name ?? currentPath);

  $: searching = search.trim().length > 0;

  $: filtered = searching
    ? entries.filter((e) =>
        e.name.toLowerCase().includes(search.trim().toLowerCase()),
      )
    : entries;

  $: visible = [...filtered].sort((a, b) => {
    // Folders always precede files, as in Explorer.
    if (a.is_dir !== b.is_dir) return a.is_dir ? -1 : 1;

    let cmp = 0;
    switch (sortKey) {
      case "name":
        cmp = a.name.localeCompare(b.name);
        break;
      case "modified":
        cmp = (a.modified ?? 0) - (b.modified ?? 0);
        break;
      case "type":
        cmp =
          typeName(a).localeCompare(typeName(b)) || a.name.localeCompare(b.name);
        break;
      case "size":
        cmp = a.size - b.size;
        break;
    }
    return sortDir === "asc" ? cmp : -cmp;
  });

  $: crumbs = isHome
    ? [{ name: "Home", path: HOME }]
    : [{ name: "Home", path: HOME }, ...splitPath(currentPath)];

  $: selectedEntry =
    selected >= 0 && selected < visible.length ? visible[selected] : null;

  $: tabList = tabs.map((t) => {
    const p = current(t.history) ?? HOME;
    return {
      id: t.id,
      title: p === HOME ? "Home" : (splitPath(p).at(-1)?.name ?? p),
    };
  });

  // Keep the keyboard selection scrolled into view.
  $: if (selected >= 0 && rowEls[selected]) {
    rowEls[selected].scrollIntoView({ block: "nearest" });
  }

  function handleKeydown(event: KeyboardEvent) {
    const target = event.target as HTMLElement | null;
    if (target && ["INPUT", "TEXTAREA"].includes(target.tagName)) return;

    if (event.altKey && event.key === "ArrowLeft") {
      event.preventDefault();
      goBack();
      return;
    }
    if (event.altKey && event.key === "ArrowRight") {
      event.preventDefault();
      goForward();
      return;
    }
    if (event.key === "F5") {
      event.preventDefault();
      refresh();
      return;
    }

    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        if (visible.length > 0)
          selected = Math.min(selected + 1, visible.length - 1);
        break;
      case "ArrowUp":
        event.preventDefault();
        if (visible.length > 0) selected = selected <= 0 ? 0 : selected - 1;
        break;
      case "Enter":
        if (target?.closest(".row")) return; // the row's own handler opens it
        event.preventDefault();
        openSelected();
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
    try {
      const [p, d] = await Promise.all([
        invoke<Place[]>("special_folders"),
        invoke<Place[]>("list_drives"),
      ]);
      places = p;
      drives = d;
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
  {crumbs}
  canBack={canGoBack(activeTab.history)}
  canForward={canGoForward(activeTab.history)}
  {search}
  searchScope={folderName}
  on:back={goBack}
  on:forward={goForward}
  on:up={goUp}
  on:refresh={refresh}
  on:navigate={(e) => navigate(e.detail)}
  on:search={(e) => {
    search = e.detail;
    selected = -1;
  }}
/>

<CommandBar
  hasSelection={selectedEntry !== null}
  {showDetails}
  {sortKey}
  {sortDir}
  on:open={openSelected}
  on:sort={(e) => {
    sortKey = e.detail.key;
    sortDir = e.detail.dir;
  }}
  on:toggleDetails={() => (showDetails = !showDetails)}
/>

<div class="main" class:with-details={showDetails}>
  <Sidebar
    {places}
    {drives}
    {currentPath}
    {isHome}
    on:navigate={(e) => navigate(e.detail)}
    on:home={() => navigate(HOME)}
  />

  <div class="content">
    {#if isHome}
      <HomeView {places} {drives} on:navigate={(e) => navigate(e.detail)} />
    {:else}
      <FileList
        entries={visible}
        {selected}
        {sortKey}
        {sortDir}
        {error}
        {loading}
        {searching}
        bind:rowEls
        on:select={(e) => (selected = e.detail)}
        on:open={(e) => open(e.detail)}
        on:sort={(e) => {
          sortKey = e.detail.key;
          sortDir = e.detail.dir;
        }}
      />
    {/if}
  </div>

  {#if showDetails}
    <DetailsPane entry={selectedEntry} {folderName} itemCount={visible.length} />
  {/if}
</div>

<StatusBar
  itemCount={visible.length}
  selectedCount={selectedEntry ? 1 : 0}
  filtered={searching}
  {notice}
/>
