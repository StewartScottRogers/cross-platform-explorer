<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import Icon from "./Icon.svelte";
  import { formatSize } from "../format";
  import { formatDate } from "../datetime";
  import { categoryOf, typeName } from "../filetypes";
  import type { DirEntry, SortKey, SortDir } from "../types";

  export let entries: DirEntry[] = [];
  export let selected = -1;
  export let sortKey: SortKey = "name";
  export let sortDir: SortDir = "asc";
  export let error = "";
  export let loading = false;
  export let searching = false;

  export let rowEls: HTMLElement[] = [];

  const dispatch = createEventDispatcher<{
    select: number;
    open: DirEntry;
    sort: { key: SortKey; dir: SortDir };
  }>();

  const COLUMNS: { key: SortKey; label: string; num?: boolean }[] = [
    { key: "name", label: "Name" },
    { key: "modified", label: "Date modified" },
    { key: "type", label: "Type" },
    { key: "size", label: "Size", num: true },
  ];

  // Clicking the active column flips direction; a new column starts ascending.
  function headerClick(key: SortKey) {
    const dir: SortDir = key === sortKey && sortDir === "asc" ? "desc" : "asc";
    dispatch("sort", { key, dir });
  }
</script>

<div class="columns">
  {#each COLUMNS as col (col.key)}
    <button
      class="col"
      class:num={col.num}
      on:click={() => headerClick(col.key)}
      title="Sort by {col.label}"
    >
      {col.label}
      {#if sortKey === col.key}
        <span class="sortchev">
          <Icon name={sortDir === "asc" ? "chev-up" : "chev-down"} size={12} />
        </span>
      {/if}
    </button>
  {/each}
</div>

{#if error}
  <div class="empty-state">
    <span class="empty-icon"><Icon name="ban" size={40} /></span>
    <p class="error">{error}</p>
  </div>
{:else if loading}
  <div class="empty-state"><p>Loading…</p></div>
{:else if entries.length === 0}
  <div class="empty-state">
    <span class="empty-icon"><Icon name="folder" size={40} /></span>
    <p>{searching ? "No items match your search" : "This folder is empty"}</p>
  </div>
{:else}
  <div class="rows">
    {#each entries as entry, i (entry.path)}
      <div
        class="row"
        class:selected={i === selected}
        bind:this={rowEls[i]}
        role="button"
        tabindex="0"
        on:click={() => dispatch("select", i)}
        on:dblclick={() => dispatch("open", entry)}
        on:keydown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            dispatch("select", i);
            dispatch("open", entry);
          }
        }}
      >
        <span class="cell name">
          <Icon name={categoryOf(entry)} />
          <span class="ellip">{entry.name}</span>
        </span>
        <span class="cell dim">{formatDate(entry.modified)}</span>
        <span class="cell dim">{typeName(entry)}</span>
        <span class="cell num">{entry.is_dir ? "" : formatSize(entry.size)}</span>
      </div>
    {/each}
  </div>
{/if}

<style>
  .ellip { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
