<script lang="ts">
  import { createEventDispatcher, tick } from "svelte";
  import Icon from "./Icon.svelte";
  import { formatSize } from "../format";
  import { formatDate } from "../datetime";
  import { iconFor, typeName } from "../filetypes";
  import { canThumbnail, thumbKey, cachedThumb, makeThumbnail } from "../thumbnails";
  import { columnsTemplate, resizeColumnTo, boundaryOffsets, COLUMN_DEFAULTS } from "../columns";
  import { isSelected } from "../selection";
  import type { Selection } from "../selection";
  import type { DirEntry, SortKey, SortDir, ViewMode } from "../types";
  import type { AgentActivity } from "../agentActivity";

  export let entries: DirEntry[] = [];
  /** Agent Watch (CPE-399): per-path live activity, keyed by absolute path. Empty ⇒ no
      annotations, so the list is visually unchanged when not watching an agent. */
  export let activity: Record<string, AgentActivity> = {};
  /** Human labels for the row badge, by activity kind. */
  const ACTIVITY_LABEL: Record<AgentActivity["kind"], string> = {
    created: "new",
    modified: "edited",
    removed: "deleted",
    renamed: "moved",
  };
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
    resizeColumns: number[];
  }>();

  /** Details-view column widths (Name/Date/Type/Size), bound from the parent so they
      persist; the trailing spacer is implicit (CPE-350). */
  export let columnWidths: number[] = COLUMN_DEFAULTS.slice();
  $: colTemplate = columnsTemplate(columnWidths);
  // Right-edge offset of each column, for placing the drag handles. 10px = .columns pad-left.
  $: handleOffsets = boundaryOffsets(columnWidths, 10);

  /** Drag a column's right edge to resize it; the layout updates live and persists on
      release. `stopPropagation` keeps the click off the sort-header button. */
  function startColResize(e: PointerEvent, i: number) {
    e.preventDefault();
    e.stopPropagation();
    const startX = e.clientX;
    const startW = columnWidths[i];
    const move = (ev: PointerEvent) => {
      columnWidths = resizeColumnTo(columnWidths, i, startW + (ev.clientX - startX));
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      dispatch("resizeColumns", columnWidths);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }

  /** Keyboard resize for a focused column divider — ← / → nudge the width (Shift = bigger
      step), so the columns are usable without a mouse (CPE-314 a11y). */
  function onResizeKey(e: KeyboardEvent, i: number) {
    if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
    e.preventDefault();
    const step = (e.shiftKey ? 32 : 8) * (e.key === "ArrowLeft" ? -1 : 1);
    columnWidths = resizeColumnTo(columnWidths, i, columnWidths[i] + step);
    dispatch("resizeColumns", columnWidths);
  }

  /** Paths being dragged, and the folder row currently hovered as a target. */
  export let draggedPaths: string[] = [];

  /** Maps a filesystem path to a URL the WebView can load (Tauri convertFileSrc).
      Used to draw image thumbnails in the Icons view (CPE-257). */
  export let assetUrl: (path: string) => string = (p) => p;

  // Ready thumbnails, keyed by path. Populated lazily as image tiles scroll in.
  let thumbs: Record<string, string> = {};

  /** Decode + cache a thumbnail for one image entry, then show it. Failures keep
      the generic icon — a broken image must never blank the tile. */
  function loadThumb(entry: DirEntry): void {
    const key = thumbKey(entry.path, entry.modified);
    const hit = cachedThumb(key);
    if (hit) {
      thumbs[entry.path] = hit;
      thumbs = thumbs;
      return;
    }
    makeThumbnail(assetUrl(entry.path), key)
      .then((data) => {
        thumbs[entry.path] = data;
        thumbs = thumbs;
      })
      .catch(() => {});
  }

  /** Svelte action: load an image tile's thumbnail once it scrolls into view.
      Falls back to an eager load where IntersectionObserver is unavailable
      (e.g. jsdom in tests), so the feature still works everywhere. */
  function lazyThumb(node: HTMLElement, entry: DirEntry) {
    if (typeof IntersectionObserver === "undefined") {
      loadThumb(entry);
      return;
    }
    const io = new IntersectionObserver(
      (obs) => {
        for (const o of obs) {
          if (o.isIntersecting) {
            loadThumb(entry);
            io.disconnect();
            break;
          }
        }
      },
      { rootMargin: "150px" },
    );
    io.observe(node);
    return { destroy: () => io.disconnect() };
  }
  let dropIndex = -1;

  // Double-click vs drag (CPE-236): in a webview the second press of a double-
  // click, with a hair of movement, can start a native drag and eat the "open".
  // Suppress dragging briefly when a press lands right after another on the same
  // row (i.e. the 2nd click of a double-click), so dblclick reliably fires. A
  // real drag — single press then actual movement — is unaffected.
  let lastPressAt = 0;
  let lastPressIndex = -1;
  let suppressDragUntil = 0;

  function onRowPointerDown(i: number) {
    const now = Date.now();
    if (now - lastPressAt < 450 && lastPressIndex === i) suppressDragUntil = now + 600;
    lastPressAt = now;
    lastPressIndex = i;
  }

  function onDragStart(e: DragEvent, i: number) {
    if (renamingPath || Date.now() < suppressDragUntil) {
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
  <div class="columns" style="--filelist-cols: {colTemplate}">
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
    {#each handleOffsets as x, i (i)}
      <!-- A focusable separator is the valid ARIA "window splitter" pattern; the lint
           flags the tabindex/handlers as if it were plain text, so suppress those. -->
      <!-- svelte-ignore a11y-no-static-element-interactions a11y-no-noninteractive-tabindex a11y-no-noninteractive-element-interactions -->
      <span
        class="col-resize"
        style="left: {x}px"
        role="separator"
        aria-orientation="vertical"
        aria-label="Resize {COLUMNS[i]?.label ?? 'column'} column"
        aria-valuenow={Math.round(columnWidths[i])}
        tabindex="0"
        title="Drag, or focus and use ← / → to resize"
        on:pointerdown={(e) => startColResize(e, i)}
        on:keydown={(e) => onResizeKey(e, i)}
      />
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
  <div class="rows" class:grid={view === "icons"} style="--filelist-cols: {colTemplate}" on:contextmenu={emptyContext}>
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
        class:agent-active={!!activity[entry.path]}
        data-agent-kind={activity[entry.path]?.kind ?? ""}
        bind:this={rowEls[i]}
        role="button"
        tabindex="0"
        draggable={!renamingPath}
        on:pointerdown={() => onRowPointerDown(i)}
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
          {#if view === "icons" && !entry.is_dir && canThumbnail(entry.extension)}
            <span class="thumb-slot" use:lazyThumb={entry}>
              {#if thumbs[entry.path]}
                <img class="thumb" src={thumbs[entry.path]} alt="" draggable="false" />
              {:else}
                <Icon name={iconFor(entry)} size={40} />
              {/if}
            </span>
          {:else}
            <Icon name={iconFor(entry)} size={view === "icons" ? 40 : 16} />
          {/if}
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
          {#if activity[entry.path]}
            <span class="agent-badge {activity[entry.path].kind}">{ACTIVITY_LABEL[activity[entry.path].kind]}</span>
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

  /* Agent Watch (CPE-399): a file the agent just touched gets a left accent bar + a kind
     badge, pulsing briefly on appearance so a live change draws the eye. Purely additive —
     rows with no activity are untouched (off means off). */
  .row.agent-active {
    box-shadow: inset 3px 0 0 var(--agent-accent, #3a9d4a);
    animation: agent-pulse 900ms ease-out;
  }
  @keyframes agent-pulse {
    from { background: color-mix(in srgb, var(--agent-accent, #3a9d4a) 26%, transparent); }
    to { background: transparent; }
  }
  .agent-badge {
    flex: 0 0 auto;
    margin-left: 8px;
    padding: 0 6px;
    border-radius: 999px;
    font-size: 10px;
    font-weight: 600;
    line-height: 16px;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: #fff;
    white-space: nowrap;
  }
  .agent-badge.created { background: #3a9d4a; }
  .agent-badge.modified { background: #b5872b; }
  .agent-badge.renamed { background: #3a72b5; }
  .agent-badge.removed { background: #b5433a; }
  /* Per-kind left accent, driven by the row's data attribute. */
  .row.agent-active[data-agent-kind="created"] { --agent-accent: #3a9d4a; }
  .row.agent-active[data-agent-kind="modified"] { --agent-accent: #b5872b; }
  .row.agent-active[data-agent-kind="renamed"] { --agent-accent: #3a72b5; }
  .row.agent-active[data-agent-kind="removed"] { --agent-accent: #b5433a; }

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

  /* Column resize handles — thin hit-targets straddling each column's right edge (CPE-350).
     .columns is position:sticky, so these absolute handles are contained by it. */
  .col-resize {
    position: absolute;
    top: 0;
    height: 100%;
    width: 7px;
    margin-left: -3px;
    cursor: col-resize;
    z-index: 6;
  }
  .col-resize:hover {
    background: var(--accent);
    opacity: 0.5;
  }

  .thumb-slot {
    width: 48px;
    height: 48px;
    display: grid;
    place-items: center;
  }
  .thumb {
    max-width: 48px;
    max-height: 48px;
    object-fit: contain;
    border-radius: 4px;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.25);
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
