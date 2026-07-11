<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import Icon from "./Icon.svelte";
  import type { SortKey, SortDir } from "../types";

  export let hasSelection = false;
  export let showDetails = true;
  export let sortKey: SortKey = "name";
  export let sortDir: SortDir = "asc";

  const dispatch = createEventDispatcher<{
    open: void;
    sort: { key: SortKey; dir: SortDir };
    toggleDetails: void;
  }>();

  let sortOpen = false;

  const SORTS: { key: SortKey; label: string }[] = [
    { key: "name", label: "Name" },
    { key: "modified", label: "Date modified" },
    { key: "type", label: "Type" },
    { key: "size", label: "Size" },
  ];

  function pickSort(key: SortKey) {
    dispatch("sort", { key, dir: sortDir });
    sortOpen = false;
  }
  function pickDir(dir: SortDir) {
    dispatch("sort", { key: sortKey, dir });
    sortOpen = false;
  }
</script>

<svelte:window on:click={() => (sortOpen = false)} />

<div class="commandbar">
  <!--
    Only actions that are genuinely wired up are enabled. Cut/Copy/Paste/Rename/
    Share/Delete are shown (Explorer has them) but disabled — better a visibly
    inert control than one that silently does nothing or, worse, pretends to.
  -->
  <button class="cmd new" disabled title="New — not implemented yet">
    <Icon name="plus" size={15} /> New
  </button>

  <span class="cmd-sep" />

  <button class="cmd" disabled title="Cut — not implemented yet"><Icon name="cut" /></button>
  <button class="cmd" disabled title="Copy — not implemented yet"><Icon name="copy" /></button>
  <button class="cmd" disabled title="Paste — not implemented yet"><Icon name="paste" /></button>
  <button class="cmd" disabled title="Rename — not implemented yet"><Icon name="rename" /></button>
  <button class="cmd" disabled title="Share — not implemented yet"><Icon name="share" /></button>
  <button class="cmd" disabled title="Delete — not implemented yet"><Icon name="delete" /></button>

  <span class="cmd-sep" />

  <button
    class="cmd"
    disabled={!hasSelection}
    title="Open the selected item"
    on:click={() => dispatch("open")}
  >
    Open
  </button>

  <span class="cmd-sep" />

  <div class="menu-wrap">
    <button class="cmd" title="Sort" on:click|stopPropagation={() => (sortOpen = !sortOpen)}>
      <Icon name="sort" /> Sort <span class="chev"><Icon name="chev-down" size={12} /></span>
    </button>
    {#if sortOpen}
      <!-- svelte-ignore a11y-no-static-element-interactions a11y-click-events-have-key-events -->
      <div class="menu" role="menu" tabindex="-1" on:click|stopPropagation>
        {#each SORTS as s (s.key)}
          <button role="menuitem" on:click={() => pickSort(s.key)}>
            <span class="check">
              {#if sortKey === s.key}<Icon name="check" size={13} />{/if}
            </span>
            {s.label}
          </button>
        {/each}
        <div class="menu-sep" />
        <button role="menuitem" on:click={() => pickDir("asc")}>
          <span class="check">
            {#if sortDir === "asc"}<Icon name="check" size={13} />{/if}
          </span>
          Ascending
        </button>
        <button role="menuitem" on:click={() => pickDir("desc")}>
          <span class="check">
            {#if sortDir === "desc"}<Icon name="check" size={13} />{/if}
          </span>
          Descending
        </button>
      </div>
    {/if}
  </div>

  <button class="cmd" disabled title="View — details view only for now">
    <Icon name="view" /> View <span class="chev"><Icon name="chev-down" size={12} /></span>
  </button>
  <button class="cmd" disabled title="Filter — not implemented yet">
    <Icon name="filter" /> Filter <span class="chev"><Icon name="chev-down" size={12} /></span>
  </button>
  <button class="cmd" disabled title="More"><Icon name="more" /></button>

  <span class="spacer" />

  <button
    class="cmd"
    title={showDetails ? "Hide details pane" : "Show details pane"}
    on:click={() => dispatch("toggleDetails")}
  >
    <Icon name="details" /> Details
  </button>
</div>
