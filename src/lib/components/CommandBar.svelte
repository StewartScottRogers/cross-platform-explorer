<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import Icon from "./Icon.svelte";
  import type { SortKey, SortDir, ViewMode } from "../types";
  import { FILE_FILTERS } from "../filetypes";
  import { t } from "../i18n";

  export let selectionCount = 0;
  export let canPaste = false;
  export let showDetails = true;
  export let showHidden = false;
  export let sortKey: SortKey = "name";
  export let sortDir: SortDir = "asc";
  export let view: ViewMode = "details";
  export let fileFilter = "all";
  export let foldersFirst = true;

  const dispatch = createEventDispatcher<{
    action: string;
    sort: { key: SortKey; dir: SortDir };
    view: ViewMode;
    filter: string;
    toggleHidden: void;
    toggleFoldersFirst: void;
    toggleDetails: void;
  }>();

  let open: "" | "sort" | "view" | "filter" = "";

  const SORTS: { key: SortKey; labelKey: string }[] = [
    { key: "name", labelKey: "sort.name" },
    { key: "modified", labelKey: "sort.modified" },
    { key: "type", labelKey: "sort.type" },
    { key: "size", labelKey: "sort.size" },
  ];
  const VIEWS: { key: ViewMode; labelKey: string }[] = [
    { key: "details", labelKey: "view.details" },
    { key: "list", labelKey: "view.list" },
    { key: "icons", labelKey: "view.icons" },
  ];

  $: one = selectionCount === 1;
  $: any = selectionCount > 0;
</script>

<svelte:window on:click={() => (open = "")} />

<div class="commandbar">
  <button class="cmd new" title="New folder (Ctrl+Shift+N)" on:click={() => dispatch("action", "new-folder")}>
    <Icon name="plus" size={15} /> {$t('cmd.new')}
  </button>

  <span class="cmd-sep" />

  <button class="cmd" disabled={!any} title="Cut (Ctrl+X)" on:click={() => dispatch("action", "cut")}>
    <Icon name="cut" />
  </button>
  <button class="cmd" disabled={!any} title="Copy (Ctrl+C)" on:click={() => dispatch("action", "copy")}>
    <Icon name="copy" />
  </button>
  <button class="cmd" disabled={!canPaste} title="Paste (Ctrl+V)" on:click={() => dispatch("action", "paste")}>
    <Icon name="paste" />
  </button>
  <button
    class="cmd"
    disabled={!one}
    title={selectionCount > 1 ? "Rename one item at a time" : "Rename (F2)"}
    on:click={() => dispatch("action", "rename")}
  >
    <Icon name="rename" />
  </button>
  <button class="cmd" disabled title="Share — not implemented yet"><Icon name="share" /></button>
  <button class="cmd" disabled={!any} title="Delete (Del)" on:click={() => dispatch("action", "delete")}>
    <Icon name="delete" />
  </button>

  <span class="cmd-sep" />

  <button class="cmd" disabled={!any} title="Open the selection" on:click={() => dispatch("action", "open")}>
    {$t('cmd.open')}
  </button>

  <span class="cmd-sep" />

  <div class="menu-wrap">
    <button class="cmd" title="Sort" on:click|stopPropagation={() => (open = open === "sort" ? "" : "sort")}>
      <Icon name="sort" /> {$t('cmd.sort')} <span class="chev"><Icon name="chev-down" size={12} /></span>
    </button>
    {#if open === "sort"}
      <!-- svelte-ignore a11y-no-noninteractive-element-interactions a11y-click-events-have-key-events -->
      <div class="menu" role="menu" tabindex="-1" on:click|stopPropagation>
        {#each SORTS as s (s.key)}
          <button role="menuitem" on:click={() => { dispatch("sort", { key: s.key, dir: sortDir }); open = ""; }}>
            <span class="check">{#if sortKey === s.key}<Icon name="check" size={13} />{/if}</span>
            {$t(s.labelKey)}
          </button>
        {/each}
        <div class="menu-sep" />
        <button role="menuitem" on:click={() => { dispatch("sort", { key: sortKey, dir: "asc" }); open = ""; }}>
          <span class="check">{#if sortDir === "asc"}<Icon name="check" size={13} />{/if}</span>
          {$t('cmd.ascending')}
        </button>
        <button role="menuitem" on:click={() => { dispatch("sort", { key: sortKey, dir: "desc" }); open = ""; }}>
          <span class="check">{#if sortDir === "desc"}<Icon name="check" size={13} />{/if}</span>
          {$t('cmd.descending')}
        </button>
      </div>
    {/if}
  </div>

  <div class="menu-wrap">
    <button class="cmd" title="View" on:click|stopPropagation={() => (open = open === "view" ? "" : "view")}>
      <Icon name="view" /> {$t('cmd.view')} <span class="chev"><Icon name="chev-down" size={12} /></span>
    </button>
    {#if open === "view"}
      <!-- svelte-ignore a11y-no-noninteractive-element-interactions a11y-click-events-have-key-events -->
      <div class="menu" role="menu" tabindex="-1" on:click|stopPropagation>
        {#each VIEWS as v (v.key)}
          <button role="menuitem" on:click={() => { dispatch("view", v.key); open = ""; }}>
            <span class="check">{#if view === v.key}<Icon name="check" size={13} />{/if}</span>
            {$t(v.labelKey)}
          </button>
        {/each}
        <div class="menu-sep" />
        <button role="menuitem" on:click={() => { dispatch("toggleHidden"); open = ""; }}>
          <span class="check">{#if showHidden}<Icon name="check" size={13} />{/if}</span>
          {$t('cmd.showHidden')}
        </button>
        <button role="menuitem" on:click={() => { dispatch("toggleFoldersFirst"); open = ""; }}>
          <span class="check">{#if foldersFirst}<Icon name="check" size={13} />{/if}</span>
          {$t('cmd.groupFolders')}
        </button>
      </div>
    {/if}
  </div>

  <div class="menu-wrap">
    <button class="cmd" class:on={fileFilter !== "all"} title="Filter by type" on:click|stopPropagation={() => (open = open === "filter" ? "" : "filter")}>
      <Icon name="filter" /> {FILE_FILTERS.find((f) => f.key === fileFilter) ? $t('filter.' + fileFilter) : $t('cmd.filter')}
      <span class="chev"><Icon name="chev-down" size={12} /></span>
    </button>
    {#if open === "filter"}
      <!-- svelte-ignore a11y-no-noninteractive-element-interactions a11y-click-events-have-key-events -->
      <div class="menu" role="menu" tabindex="-1" on:click|stopPropagation>
        {#each FILE_FILTERS as f (f.key)}
          <button role="menuitem" on:click={() => { dispatch("filter", f.key); open = ""; }}>
            <span class="check">{#if fileFilter === f.key}<Icon name="check" size={13} />{/if}</span>
            {$t('filter.' + f.key)}
          </button>
        {/each}
      </div>
    {/if}
  </div>

  <span class="spacer" />

  <button
    class="cmd"
    title={showDetails ? "Hide details pane (Alt+P)" : "Show details pane (Alt+P)"}
    on:click={() => dispatch("toggleDetails")}
  >
    <Icon name="details" /> Details
  </button>
</div>
