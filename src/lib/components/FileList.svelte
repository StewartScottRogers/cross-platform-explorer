<script lang="ts">
  import { createEventDispatcher, tick } from "svelte";
  import Icon from "./Icon.svelte";
  import { formatSize } from "../format";
  import { formatDate } from "../datetime";
  import { categoryOf, typeName } from "../filetypes";
  import { isSelected } from "../selection";
  import type { Selection } from "../selection";
  import type { DirEntry, SortKey, SortDir, ViewMode } from "../types";

  export let entries: DirEntry[] = [];
  export let selection: Selection;
  export let sortKey: SortKey = "name";
  export let sortDir: SortDir = "asc";
  export let view: ViewMode = "details";
  export let error = "";
  export let loading = false;
  export let searching = false;
  export let cutPaths: string[] = [];

  /** Path currently being renamed inline, or "" for none. */
  export let renamingPath = "";
  /** Initial text for the inline editor. */
  export let renameValue = "";

  export let rowEls: HTMLElement[] = [];

  const dispatch = createEventDispatcher<{
    click: { index: number; ctrl: boolean; shift: boolean };
    open: DirEntry;
    sort: { key: SortKey; dir: SortDir };
    context: { x: number; y: number; index: number };
    contextEmpty: { x: number; y: number };
    commitRename: string;
    cancelRename: void;
    drop: { paths: string[]; dest: string; copy: boolean };
  }>();

  /** Paths being dragged, and the folder row currently hovered as a target. */
  export let draggedPaths: string[] = [];
  let dropIndex = -1;

  function onDragStart(e: DragEvent, i: number) {
    if (renamingPath) {
      e.preventDefault();
      return;
    }
    // Drag the whole selection if the grabbed row is part of it; otherwise
    // just the grabbed row (Explorer's behaviour).
    const paths = isSelected(selection, i)
      ? entries.filter((_, j) => isSelected(selection, j)).map((x) => x.path)
      : [entries[i].path];
    draggedPaths = paths;
    e.dataTransfer?.setData("text/plain", paths.join("\n"));
    if (e.dataTransfer) e.dataTransfer.effectAllowed = "copyMove";
  }

  function onDragEnd() {
    draggedPaths = [];
    dropIndex = -1;
  }

  /** Only folders are valid targets, and never a folder being dragged itself. */
  function validTarget(i: number): boolean {
    const entry = entries[i];
    if (!entry?.is_dir) return false;
    if (draggedPaths.includes(entry.path)) return false;
    return draggedPaths.length > 0;
  }

  function onDragOver(e: DragEvent, i: number) {
    if (!validTarget(i)) return;
    e.preventDefault();
    if (e.dataTransfer) {
      e.dataTransfer.dropEffect = e.ctrlKey ? "copy" : "move";
    }
    dropIndex = i;
  }

  function onDrop(e: DragEvent, i: number) {
    if (!validTarget(i)) return;
    e.preventDefault();
    const paths = [...draggedPaths];
    const dest = entries[i].path;
    const copy = e.ctrlKey;
    onDragEnd();
    dispatch("drop", { paths, dest, copy });
  }

  const COLUMNS: { key: SortKey; label: string; num?: boolean }[] = [
    { key: "name", label: "Name" },
    { key: "modified", label: "Date modified" },
    { key: "type", label: "Type" },
    { key: "size", label: "Size", num: true },
  ];

  function headerClick(key: SortKey) {
    const dir: SortDir = key === sortKey && sortDir === "asc" ? "desc" : "asc";
    dispatch("sort", { key, dir });
  }

  let editEl: HTMLInputElement | undefined;
  $: if (renamingPath && editEl) focusEditor();

  async function focusEditor() {
    await tick();
    if (!editEl) return;
    editEl.focus();
    // Select the stem, not the extension — renaming "photo.png" shouldn't make
    // it trivially easy to destroy the extension by typing over it.
    const dot = renameValue.lastIndexOf(".");
    if (dot > 0) editEl.setSelectionRange(0, dot);
    else editEl.select();
  }

  function onEditorKey(e: KeyboardEvent) {
    e.stopPropagation(); // list shortcuts must never fire while editing
    if (e.key === "Enter") {
      e.preventDefault();
      dispatch("commitRename", (e.currentTarget as HTMLInputElement).value);
    } else if (e.key === "Escape") {
      e.preventDefault();
      dispatch("cancelRename");
    }
  }

  function rowClick(e: MouseEvent, i: number) {
    dispatch("click", {
      index: i,
      ctrl: e.ctrlKey || e.metaKey,
      shift: e.shiftKey,
    });
  }

  function rowContext(e: MouseEvent, i: number) {
    e.preventDefault();
    e.stopPropagation();
    dispatch("context", { x: e.clientX, y: e.clientY, index: i });
  }

  function emptyContext(e: MouseEvent) {
    e.preventDefault();
    dispatch("contextEmpty", { x: e.clientX, y: e.clientY });
  }

  const isCut = (p: string) => cutPaths.includes(p);
</script>

{#if view === "details" && !error && !loading && entries.length > 0}
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
{/if}

{#if error}
  <div class="empty-state">
    <span class="empty-icon"><Icon name="ban" size={40} /></span>
    <p class="error">{error}</p>
  </div>
{:else if loading}
  <div class="empty-state"><p>Loading…</p></div>
{:else if entries.length === 0}
  <!-- svelte-ignore a11y-no-static-element-interactions -->
  <div class="empty-state" on:contextmenu={emptyContext}>
    <span class="empty-icon"><Icon name="folder" size={40} /></span>
    <p>{searching ? "No items match your search" : "This folder is empty"}</p>
  </div>
{:else}
  <!-- svelte-ignore a11y-no-static-element-interactions -->
  <div class="rows" class:grid={view === "icons"} on:contextmenu={emptyContext}>
    {#each entries as entry, i (entry.path)}
      <!--
        The view class MUST stay namespaced as "view-{view}".
        Interpolating the bare view name gave every row the class `details`,
        which collides with the global `.details` DetailsPane rule
        (display:flex; padding:20px) — that overrode the row's grid layout and
        clipped every row to nothing. The list rendered 18 blank strips while
        the status bar correctly reported "18 items". Shipped in v0.5.0. CPE-045.
      -->
      <div
        class="row view-{view}"
        class:selected={isSelected(selection, i)}
        class:cut={isCut(entry.path)}
        class:lead={selection.lead === i}
        class:droptarget={dropIndex === i}
        class:dragging={draggedPaths.includes(entry.path)}
        bind:this={rowEls[i]}
        role="button"
        tabindex="0"
        draggable={!renamingPath}
        on:dragstart={(e) => onDragStart(e, i)}
        on:dragend={onDragEnd}
        on:dragover={(e) => onDragOver(e, i)}
        on:dragleave={() => (dropIndex = dropIndex === i ? -1 : dropIndex)}
        on:drop={(e) => onDrop(e, i)}
        on:click|stopPropagation={(e) => rowClick(e, i)}
        on:dblclick={() => dispatch("open", entry)}
        on:contextmenu={(e) => rowContext(e, i)}
        on:keydown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            dispatch("open", entry);
          }
        }}
      >
        <span class="cell name">
          <Icon name={categoryOf(entry)} size={view === "icons" ? 40 : 16} />
          {#if renamingPath === entry.path}
            <input
              class="rename"
              bind:this={editEl}
              value={renameValue}
              on:keydown={onEditorKey}
              on:click|stopPropagation
              on:dblclick|stopPropagation
              on:blur={(e) => dispatch("commitRename", e.currentTarget.value)}
            />
          {:else}
            <span class="ellip">{entry.name}</span>
          {/if}
        </span>

        {#if view === "details"}
          <span class="cell dim">{formatDate(entry.modified)}</span>
          <span class="cell dim">{typeName(entry)}</span>
          <span class="cell num">{entry.is_dir ? "" : formatSize(entry.size)}</span>
        {/if}
      </div>
    {/each}
  </div>
{/if}

<style>
  .ellip {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .rename {
    flex: 1;
    min-width: 0;
    font: inherit;
    padding: 1px 4px;
    border: 1px solid var(--accent);
    border-radius: 3px;
    background: #fff;
    color: var(--text);
    outline: none;
  }

  /* Cut items dim until the paste completes — the affordance Explorer uses, so
     a pending move is visible rather than invisible state. */
  .row.cut {
    opacity: 0.45;
  }

  .row.lead:not(.selected) {
    outline: 1px dotted var(--text-faint);
    outline-offset: -1px;
  }

  /* Only valid drop targets ever highlight, so an invalid drop is visibly
     impossible rather than merely rejected after the fact. */
  .row.droptarget {
    background: var(--selection);
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }

  .row.dragging {
    opacity: 0.5;
  }

  .row.view-list {
    grid-template-columns: 1fr;
  }

  .rows.grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(124px, 1fr));
    gap: 6px;
    padding: 10px;
  }

  .row.view-icons {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    height: auto;
    padding: 12px 6px;
    text-align: center;
  }

  .row.view-icons :global(.cell.name) {
    flex-direction: column;
    gap: 6px;
    width: 100%;
  }

  .row.view-icons .ellip {
    width: 100%;
    white-space: normal;
    overflow: hidden;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    line-height: 1.25;
  }
</style>
