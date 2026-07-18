<script lang="ts">
  // ExplorerPane (CPE-676, epic CPE-617): the middle file-listing region — Home screen, the Agent-Watch
  // activity strip, the tag-filter indicator, and the FileList itself. Extracted from App.svelte as the
  // first step toward a reusable pane that can be instantiated twice for dual-pane commander mode. For now
  // it is presentational: App still owns the explorer state and passes it in via props/binds and receives
  // actions via events. Subsequent slices push state ownership down into this component.
  import { createEventDispatcher } from "svelte";
  import Icon from "./Icon.svelte";
  import HomeView from "./HomeView.svelte";
  import FileList from "./FileList.svelte";
  import { t } from "../i18n";
  import * as settings from "../settings";
  import { baseName } from "../contentSearch";
  import { fsActivity, agentTimeline } from "../agentActivity";
  import { click as selClick, type Selection } from "../selection";
  import type { DirEntry, Place, SortKey, SortDir, ViewMode, RecentFile, Favorite } from "../types";

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
  export let visible: DirEntry[] = [];
  export let selectedTag = "";
  export let error = "";
  export let loading = false;
  export let searching = false;
  export let cutPaths: string[] = [];
  export let renamingPath = "";
  export let renameValue = "";
  export let canDrag = true;
  export let view: ViewMode = "details";
  export let sortKey: SortKey = "name";
  export let sortDir: SortDir = "asc";
  export let columnWidths: number[] = [];
  export let selection: Selection;
  export let draggedPaths: string[] = [];
  export let rowEls: HTMLElement[] = [];

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
  }>();
</script>

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
