<script lang="ts">
  // ExplorerPane (CPE-676, epic CPE-617): the middle file-listing region — Home screen, the Agent-Watch
  // activity strip, the tag-filter indicator, and the FileList itself. Extracted from App.svelte as the
  // first step toward a reusable pane that can be instantiated twice for dual-pane commander mode. For now
  // it is presentational: App still owns the explorer state and passes it in via props/binds and receives
  // actions via events. Subsequent slices push state ownership down into this component.
  import { createEventDispatcher, tick } from "svelte";
  import { rawInvoke, createChannel } from "../invoke";
  import { friendlyError } from "../format";
  import Icon from "./Icon.svelte";
  import HomeView from "./HomeView.svelte";
  import FileList from "./FileList.svelte";
  import Toolbar from "./Toolbar.svelte";
  import ContextBar from "./ContextBar.svelte";
  import { t } from "../i18n";
  import * as settings from "../settings";
  import { baseName } from "../contentSearch";
  import { fsActivity, agentTimeline } from "../agentActivity";
  import { click as selClick, selectedIndices, type Selection } from "../selection";
  import { sortEntries } from "../sort";
  import { makeMatcher } from "../search";
  import { matchesFileFilter } from "../filetypes";
  import { filterEntriesByTag } from "../tagFilter";
  import { tags } from "../tags";
  import type { FolderAction, FolderContext } from "../folderContext";
  import type { DirEntry, Place, SortKey, SortDir, ViewMode, RecentFile, Favorite } from "../types";
  import type { ColorRule } from "../colorRules";

  /** True when the Home screen should show (App: `isHome && !smartFolder`). */
  export let inHome = false;
  export let places: Place[] = [];
  export let drives: Place[] = [];
  export let pins: string[] = [];
  export let recents: RecentFile[] = [];
  export let favorites: Favorite[] = [];
  export let recentFolders: RecentFile[] = [];

  // Agent-Watch strip (CPE-399).
  export let activeWatchCwd = "";
  export let watchedAgentName = "";
  export let recentChanges: { path: string; kind: string }[] = [];
  export let showTimeline = false;

  // The listing + its display state.
  export let showHidden = false;
  export let folderContexts: FolderContext[] = [];
  // The raw directory listing — owned here now (CPE-676 domino 3), bound back to App (whose loadPath still
  // populates it). The pane derives the whole sort/hidden/search/type/tag pipeline down to `visible`.
  export let entries: DirEntry[] = [];
  // Overrides App supplies for the non-plain views: the smart-folder matches, or the archive children.
  // Archive mode disables the hidden/search/type/tag filters (raw list, only sorted).
  export let smartOverride: DirEntry[] | null = null;
  export let archiveOverride: DirEntry[] | null = null;
  // The base list the pipeline runs on, resolved from the plain listing + the active-view overrides.
  $: baseEntries = archiveOverride ?? smartOverride ?? entries;
  $: rawList = archiveOverride != null;
  export let search = "";
  export let fileFilter = "all";
  export let foldersFirst = true;
  /** The filtered + sorted listing shown in the FileList. Derived + owned here; bound back to App. */
  export let visible: DirEntry[] = [];
  /** The pre-filter (hidden-only) listing, bound back to App for the status-bar "X of Y" total. */
  export let shown: DirEntry[] = [];
  export let selectedTag = "";
  export let error = "";
  export let loading = false;
  export let cutPaths: string[] = [];
  export let renamingPath = "";
  export let renameValue = "";
  export let canDrag = true;
  /** Rule-based coloring rule set (CPE-776), threaded through to the FileList rows. */
  export let colorRules: ColorRule[] = [];
  /** Recursive folder-size column (CPE-750). */
  export let showFolderSizes = false;
  export let folderSizes: Map<string, number> = new Map();
  export let view: ViewMode = "details";
  export let sortKey: SortKey = "name";
  export let sortDir: SortDir = "asc";
  export let columnWidths: number[] = [];
  export let selection: Selection;
  export let draggedPaths: string[] = [];
  export let rowEls: HTMLElement[] = [];
  /** The entries under the current selection, derived from `selection` + `visible` and owned here (CPE-676).
   * Bound back out to App so its file/nav operations read the active pane's selection. */
  export let selectedEntries: DirEntry[] = [];

  // ---- derived listing (CPE-676 domino 2) — the sort/hidden/search/type/tag pipeline, owned here.
  // In `rawList` mode (archive browsing) none of the filters apply: the base list is only sorted.
  $: searching = search.trim().length > 0;
  $: shown = rawList ? baseEntries : baseEntries.filter((e) => showHidden || !e.hidden);
  $: searchMatcher = makeMatcher(search);
  $: filtered = !rawList && searching ? shown.filter((e) => searchMatcher(e.name)) : shown;
  $: typeFiltered =
    !rawList && fileFilter !== "all" ? filtered.filter((e) => matchesFileFilter(e, fileFilter)) : filtered;
  $: tagFiltered =
    !rawList && selectedTag ? filterEntriesByTag(typeFiltered, $tags, selectedTag) : typeFiltered;
  // Recursive-size sort key (CPE-750): a not-yet-computed folder resolves to -1 so pending folders cluster.
  $: sizeOf = showFolderSizes
    ? (e: DirEntry) => (e.is_dir ? (folderSizes.get(e.path) ?? -1) : e.size)
    : undefined;
  $: visible = sortEntries(tagFiltered, sortKey, sortDir, foldersFirst, sizeOf);
  // `selectedEntries` depends on `visible`; Svelte orders these reactive blocks by dependency.
  $: selectedEntries = selectedIndices(selection)
    .map((i) => visible[i])
    .filter(Boolean);

  // ---- listing fetch + directory cache (CPE-676 domino 3b) — the pane owns fetching its own listing.
  // A generation token supersedes an in-flight stream when the caller navigates away; the LRU cache
  // (CPE-756) lets a navigation paint instantly and revalidates in the background. Reloads after a
  // mutation pass useCache=false so our own changes never show stale. Populates the bound `entries`
  // (+ `loading`/`error`); returns whether this load is still the current one (false = superseded).
  let loadGen = 0;
  const dirCache = new Map<string, DirEntry[]>(); // insertion order == LRU recency
  const DIR_CACHE_MAX = 48;
  function cacheGet(path: string): DirEntry[] | undefined {
    const v = dirCache.get(path);
    if (v) { dirCache.delete(path); dirCache.set(path, v); }
    return v;
  }
  function cachePut(path: string, list: DirEntry[]): void {
    dirCache.delete(path);
    dirCache.set(path, list);
    while (dirCache.size > DIR_CACHE_MAX) dirCache.delete(dirCache.keys().next().value as string);
  }
  const sameListing = (a: DirEntry[], b: DirEntry[]): boolean =>
    a.length === b.length && a.every((e, i) => e.path === b[i].path && e.size === b[i].size && e.modified === b[i].modified);
  async function revalidateDir(path: string, gen: number): Promise<void> {
    try {
      const fresh = await rawInvoke<DirEntry[]>("list_dir", { path });
      cachePut(path, fresh);
      if (gen === loadGen && !sameListing(entries, fresh)) entries = fresh;
    } catch { /* keep the cached view */ }
  }

  /** Fetch + stream `path` into `entries`. Owns supersede + cache. Returns false if superseded (the caller
   *  must then skip its post-load work). App keeps the navigation orchestration + HOME handling. */
  export async function loadListing(path: string, useCache = false): Promise<boolean> {
    const gen = ++loadGen;
    // Stop the backend walking the folder we just left (CPE-665); no-op if it already finished.
    if (gen > 1) rawInvoke("cancel_dir_stream", { streamId: gen - 1 }).catch(() => {});

    const servedFromCache = useCache ? cacheGet(path) : undefined;
    if (servedFromCache) {
      entries = servedFromCache;
      loading = false;
      await tick(); // let the reactive `visible` derive before the caller's post-load hooks read it
    } else {
      entries = [];
      loading = true;
      try {
        // Coalesce stream batches (CPE-689): buffer and flush once per animation frame so `visible`
        // re-sorts a handful of times, not once per 256-row batch; the first frame still paints live.
        const channel = createChannel<DirEntry[]>();
        let buffer: DirEntry[] = [];
        let flushQueued = false;
        const flush = () => {
          flushQueued = false;
          if (gen !== loadGen || buffer.length === 0) { buffer = []; return; }
          entries = entries.concat(buffer);
          buffer = [];
          loading = false;
        };
        channel.onmessage = (batch) => {
          if (gen !== loadGen) return; // superseded — drop stale rows
          buffer.push(...batch);
          if (!flushQueued) { flushQueued = true; requestAnimationFrame(flush); }
        };
        await rawInvoke("list_dir_stream", { path, streamId: gen, onEntry: channel });
        if (gen === loadGen && buffer.length > 0) flush();
      } catch (e) {
        if (gen === loadGen) { entries = []; error = friendlyError(String(e)); }
      } finally {
        if (gen === loadGen) loading = false;
      }
      if (gen === loadGen) cachePut(path, entries);
    }

    if (gen !== loadGen) return false; // a newer navigation superseded this one

    // Stale-while-revalidate (CPE-756): a cache-served folder re-lists in the background.
    if (servedFromCache && !error) setTimeout(() => revalidateDir(path, gen), 300);
    return true;
  }

  const dispatch = createEventDispatcher<{
    navigate: string;
    openRecent: string;
    unpin: string;
    unfavorite: string;
    removeRecent: string;
    removeRecentFolder: string;
    clearRecents: void;
    open: DirEntry;
    rowContext: { x: number; y: number; index: number };
    contextEmpty: { x: number; y: number };
    commitRename: string;
    drop: { paths: string[]; dest: string; ctrlKey: boolean; shiftKey: boolean };
    contextAction: FolderAction;
  }>();
</script>

<Toolbar label={$t("tb.fileList")}>
  <div class="settings-row">
    <span>{$t("menu.view")}</span>
    <select bind:value={view} on:change={() => settings.saveView(view)}>
      <option value="details">{$t("view.details")}</option>
      <option value="list">{$t("view.list")}</option>
      <option value="icons">{$t("tb.icons")}</option>
      <option value="gallery">{$t("view.gallery")}</option>
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
<ContextBar contexts={folderContexts} on:action={(e) => dispatch("contextAction", e.detail)} />
<div class="filelist-pane" role="region" aria-label={$t("tb.fileList")}>
{#if inHome}
  <HomeView
    {places}
    {drives}
    {pins}
    {recents}
    {favorites}
    {recentFolders}
    on:navigate={(e) => dispatch("navigate", e.detail)}
    on:openFile={(e) => dispatch("openRecent", e.detail)}
    on:unpin={(e) => dispatch("unpin", e.detail)}
    on:unfavorite={(e) => dispatch("unfavorite", e.detail)}
    on:removeRecent={(e) => dispatch("removeRecent", e.detail)}
    on:removeRecentFolder={(e) => dispatch("removeRecentFolder", e.detail)}
    on:clearRecents={() => dispatch("clearRecents")}
  />
{:else}
  {#if activeWatchCwd}
    <div class="agent-strip" role="status">
      <span class="agent-dot" />
      <span class="agent-strip-label">{$t("agent.watch", { name: watchedAgentName })}</span>
      {#each recentChanges as c (c.path)}
        <span class="agent-chip {c.kind}" title={c.path}>{c.kind === "removed" ? "−" : c.kind === "created" ? "+" : "~"} {baseName(c.path)}</span>
      {/each}
      {#if recentChanges.length === 0}
        <span class="agent-strip-idle">{$t("agent.watching")}</span>
      {/if}
      <button class="agent-log-btn" on:click={() => (showTimeline = !showTimeline)} title={$t("agent.showLog")}>
        {$t("agent.log")} {$agentTimeline.length ? `(${$agentTimeline.length})` : ""}
      </button>
    </div>
  {/if}
  {#if selectedTag}
    <div class="tag-filter-bar">
      <Icon name="tag" size={13} />
      <span class="tf-label">{selectedTag}</span>
      <span class="tf-count">{visible.length}</span>
      <button class="tf-clear" title="Clear tag filter" aria-label="Clear tag filter" on:click={() => (selectedTag = "")}>
        <Icon name="close" size={12} />
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
    {canDrag}
    {renameValue}
    {columnWidths}
    {colorRules}
    {showFolderSizes}
    {folderSizes}
    on:needSizes
    on:resizeColumns={(e) => { columnWidths = e.detail; settings.saveColumnWidths(columnWidths); }}
    bind:rowEls
    bind:draggedPaths
    on:click={(e) => (selection = selClick(selection, e.detail.index, e.detail))}
    on:open={(e) => dispatch("open", e.detail)}
    on:sort={(e) => {
      sortKey = e.detail.key; sortDir = e.detail.dir;
      settings.saveSortKey(sortKey); settings.saveSortDir(sortDir);
    }}
    on:context={(e) => dispatch("rowContext", e.detail)}
    on:contextEmpty={(e) => dispatch("contextEmpty", e.detail)}
    on:commitRename={(e) => dispatch("commitRename", e.detail)}
    on:cancelRename={() => (renamingPath = "")}
    on:drop={(e) => dispatch("drop", e.detail)}
  />
{/if}
</div>

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

  /* Active tag-filter indicator above the list (CPE-655). */
  .tag-filter-bar {
    display: flex; align-items: center; gap: 6px;
    padding: 4px 10px; margin: 4px 6px 0; border-radius: 6px;
    background: var(--surface-alt); border: 1px solid var(--border);
    font-size: 12px; color: var(--text);
  }
  .tf-label { font-weight: 600; }
  .tf-count { color: var(--text-faint); font-variant-numeric: tabular-nums; }
  .tf-clear { margin-left: auto; width: 20px; height: 20px; display: grid; place-items: center; border-radius: 4px; color: var(--text-dim); }
  .tf-clear:hover { background: var(--surface); color: var(--text); }
</style>
