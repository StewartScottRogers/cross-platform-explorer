<script lang="ts">
  import { createEventDispatcher, onMount } from "svelte";
  import Icon from "./Icon.svelte";
  import Submenu from "./Submenu.svelte";
  import { t } from "../i18n";
  import type { ViewMode, SortKey, SortDir } from "../types";

  export let x = 0;
  export let y = 0;
  /** "item" when opened on a row, "empty" when opened on blank space. */
  export let target: "item" | "empty" = "item";
  export let canPaste = false;
  export let selectionCount = 0;
  /** True when exactly one folder is selected — enables "Open in new tab". */
  export let folderSelected = false;
  /** True when exactly one executable file is selected — enables Execute (CPE-241). */
  export let executableSelected = false;
  /** Icon for the "Open" item — the selected entry's own icon (CPE-243). */
  export let openIcon = "folder";
  /** Whether the selected folder is pinned to Home — toggles Pin/Unpin (CPE-249). */
  export let pinned = false;
  /** Whether the single selected item is a favorite — toggles the label (CPE-338). */
  export let favorited = false;
  /** True when the selection can be packed into a .zip (CPE-251). */
  export let compressible = false;
  /** True when exactly two files (no folders) are selected — enables Compare (CPE-418). */
  export let comparable = false;
  /** True when exactly one archive file is selected — enables Extract (CPE-252). */
  export let extractable = false;
  /** True when Open-in-Terminal applies (a real folder, not Home/archive) (CPE-253). */
  export let canTerminal = false;
  /** Extension (no dot) to offer "Select all .ext"; empty hides the row (CPE-258). */
  export let sameTypeExt = "";
  /** True when the selection contains at least one image file (of 2+ selected) — enables "Batch media…"
   *  (CPE-1093). The dialog itself pre-filters any non-image files out of the eligible set. */
  export let mediaEligible = false;
  /** Current view mode — the empty-area View ▸ submenu checkmarks it (CPE-1153). Single source of
   *  truth: the same `view` state the toolbar/CommandBar drive; selecting here dispatches `view:<mode>`. */
  export let view: ViewMode = "details";
  /** Current sort key + direction — Sort by ▸ checkmarks them; selecting dispatches `sort:<key>` /
   *  `sortdir:<dir>` into the same state the column headers use (CPE-1153). */
  export let sortKey: SortKey = "name";
  export let sortDir: SortDir = "asc";
  /** Whether the undo stack has anything to undo — gates the empty-area Undo row (CPE-1153). */
  export let canUndo = false;
  /** Human label of the top undo entry (e.g. "Rename to report.txt"); appended after "Undo" when set. */
  export let undoLabel = "";

  const dispatch = createEventDispatcher<{
    action: string;
    close: void;
  }>();

  let el: HTMLDivElement;
  let left = x;
  let top = y;

  // Keep the menu on screen — a menu that opens half off the edge is useless.
  onMount(() => {
    const rect = el.getBoundingClientRect();
    const pad = 6;
    left = Math.min(x, window.innerWidth - rect.width - pad);
    top = Math.min(y, window.innerHeight - rect.height - pad);
    left = Math.max(pad, left);
    top = Math.max(pad, top);
    el.focus();
  });

  function run(action: string) {
    dispatch("action", action);
    dispatch("close");
  }
</script>

<svelte:window
  on:click={() => dispatch("close")}
  on:contextmenu|preventDefault={() => dispatch("close")}
  on:keydown={(e) => e.key === "Escape" && dispatch("close")}
/>

<!-- svelte-ignore a11y-no-noninteractive-element-interactions a11y-click-events-have-key-events -->
<div
  class="ctx"
  role="menu"
  tabindex="-1"
  bind:this={el}
  style="left:{left}px; top:{top}px"
  on:click|stopPropagation
  on:contextmenu|stopPropagation|preventDefault
>
  {#if target === "item"}
    <!-- Win11 puts the common actions in a quick-action icon row at the top. -->
    <div class="quickrow">
      <button title="Cut (Ctrl+X)" on:click={() => run("cut")}><Icon name="cut" /></button>
      <button title="Copy (Ctrl+C)" on:click={() => run("copy")}><Icon name="copy" /></button>
      <button
        title={selectionCount > 1 ? "Rename one item at a time" : "Rename (F2)"}
        disabled={selectionCount !== 1}
        on:click={() => run("rename")}><Icon name="rename" /></button>
      <button title="Delete (Del)" on:click={() => run("delete")}><Icon name="delete" /></button>
    </div>
    <div class="sep" />
    <button class="row" role="menuitem" on:click={() => run("open")}>
      <Icon name={openIcon} size={15} /> {$t('ctx.open')}
    </button>
    {#if executableSelected}
      <button class="row" role="menuitem" on:click={() => run("execute")}>
        <Icon name="executable" size={15} /> {$t('ctx.execute')}
      </button>
      <button class="row" role="menuitem" on:click={() => run("execute-admin")}>
        <Icon name="executable" size={15} /> {$t('ctx.executeAdmin')}
      </button>
    {/if}
    {#if folderSelected}
      <button class="row" role="menuitem" on:click={() => run("open-new-tab")}>
        <Icon name="plus" size={15} /> {$t('ctx.openNewTab')}
      </button>
      {#if canTerminal}
        <button class="row" role="menuitem" on:click={() => run("terminal-folder")}>
          <Icon name="code" size={15} /> {$t('ctx.openInTerminal')}
        </button>
      {/if}
    {/if}
    {#if canTerminal}
      <button class="row" role="menuitem" on:click={() => run("open-in-console")}>
        <Icon name="code" size={15} /> {$t('ctx.workOnThis')}
      </button>
    {/if}
    <div class="sep" />
    <!-- New ▸ on the on-item menu (CPE-1156) — the same CPE-1153 submenu as the empty-area menu, so
         right-clicking ANY item still exposes New. When a single FOLDER is selected the create targets
         INSIDE it (`new-*-in`, handled with `selectedEntries[0].path`); on a file / multi-selection it
         creates in the current folder (`new-*`), same as the empty-area menu. -->
    <Submenu label={$t('cmd.new')} icon="plus">
      <button class="row" role="menuitem" on:click={() => run(folderSelected ? "new-folder-in" : "new-folder")}>
        <Icon name="folder" size={15} /> {$t('ctx.folder')}
        {#if !folderSelected}<span class="hint">Ctrl+Shift+N</span>{/if}
      </button>
      <button class="row" role="menuitem" on:click={() => run(folderSelected ? "new-file-in" : "new-file")}>
        <Icon name="document" size={15} /> {$t('ctx.textFile')}
      </button>
    </Submenu>
    <div class="sep" />
    <button class="row" role="menuitem" on:click={() => run("duplicate")}>
      <Icon name="copy" size={15} /> {$t('ctx.duplicate')}
      <span class="hint">Ctrl+D</span>
    </button>
    <button class="row" role="menuitem" on:click={() => run("copy-path")}>
      <Icon name="paste" size={15} /> {$t('ctx.copyAsPath')}
      <span class="hint">Ctrl+Shift+C</span>
    </button>
    {#if canTerminal}
      <button class="row" role="menuitem" on:click={() => run("copy-to")}>
        <Icon name="copy" size={15} /> {$t('ctx.copyToFolder')}
      </button>
      <button class="row" role="menuitem" on:click={() => run("move-to")}>
        <Icon name="cut" size={15} /> {$t('ctx.moveToFolder')}
      </button>
    {/if}
    <button class="row" role="menuitem" on:click={() => run("copy-name")}>
      <Icon name="rename" size={15} /> {$t('ctx.copyName')}
    </button>
    {#if selectionCount > 1}
      <button class="row" role="menuitem" on:click={() => run("batch-rename")}>
        <Icon name="rename" size={15} /> {$t('ctx.rename')}
      </button>
    {/if}
    {#if selectionCount > 1 && mediaEligible}
      <button class="row" role="menuitem" on:click={() => run("batch-media")}>
        <Icon name="image" size={15} /> {$t('ctx.batchMedia')}
      </button>
    {/if}
    {#if comparable}
      <button class="row" role="menuitem" on:click={() => run("compare")}>
        <Icon name="copy" size={15} /> {$t('ctx.compareFiles')}
      </button>
    {/if}
    {#if sameTypeExt}
      <button class="row" role="menuitem" on:click={() => run("select-type")}>
        <Icon name="filter" size={15} /> {$t('ctx.selectAllExt', { ext: sameTypeExt })}
      </button>
    {/if}
    {#if extractable}
      <button class="row" role="menuitem" on:click={() => run("extract")}>
        <Icon name="archive" size={15} /> {$t('ctx.extract')}
      </button>
    {/if}
    {#if compressible}
      <button class="row" role="menuitem" on:click={() => run("compress")}>
        <Icon name="archive" size={15} /> {$t('ctx.compressZip')}
      </button>
    {/if}
    {#if folderSelected}
      <button class="row" role="menuitem" on:click={() => run("pin")}>
        <Icon name="pin" size={15} /> {pinned ? $t('ctx.unpinFromHome') : $t('ctx.pinToHome')}
      </button>
    {/if}
    {#if selectionCount === 1}
      <button class="row" role="menuitem" on:click={() => run("favorite")}>
        <Icon name="star" size={15} /> {favorited ? $t('ctx.removeFavorite') : $t('ctx.addFavorite')}
      </button>
    {/if}
    <!-- Tags apply to a single item or a whole multi-selection (batch add, CPE-656). -->
    <button class="row" role="menuitem" on:click={() => run("tags")}>
      <Icon name="tag" size={15} /> {$t('ctx.tags')}
    </button>
    <div class="sep" />
    <button class="row" role="menuitem" on:click={() => run("reveal")}>
      <Icon name="folder" size={15} /> {$t('ctx.reveal')}
    </button>
    <button class="row" role="menuitem" on:click={() => run("properties")}>
      <Icon name="info" size={15} /> {$t('ctx.properties')}
      <span class="hint">Alt+Enter</span>
    </button>
    <button class="row" role="menuitem" on:click={() => run("metadataStudio")}>
      <Icon name="info" size={15} /> {$t('studio.menu')}
    </button>
    <div class="sep" />
    <button class="row" role="menuitem" on:click={() => run("help-docs")}>
      <Icon name="book" size={15} /> Documents for this view
    </button>
  {:else}
    <!-- New ▸ — Windows 11 shape: create actions live behind a submenu (CPE-1153). -->
    <Submenu label={$t('cmd.new')} icon="plus">
      <button class="row" role="menuitem" on:click={() => run("new-folder")}>
        <Icon name="folder" size={15} /> {$t('ctx.folder')}
        <span class="hint">Ctrl+Shift+N</span>
      </button>
      <button class="row" role="menuitem" on:click={() => run("new-file")}>
        <Icon name="document" size={15} /> {$t('ctx.textFile')}
      </button>
    </Submenu>
    <div class="sep" role="separator" />
    <button class="row" role="menuitem" disabled={!canPaste} on:click={() => run("paste")}>
      <Icon name="paste" size={15} /> {$t('ctx.paste')}
      <span class="hint">Ctrl+V</span>
    </button>
    <button class="row" role="menuitem" disabled={!canUndo} on:click={() => run("undo")}>
      <Icon name="back" size={15} /> {$t('ctx.undo')}{undoLabel ? ` ${undoLabel}` : ''}
      <span class="hint">Ctrl+Z</span>
    </button>
    <div class="sep" role="separator" />
    <!-- View ▸ — drives the SAME `view` state the toolbar/CommandBar use; current mode checkmarked. -->
    <Submenu label={$t('cmd.view')} icon="view">
      <button class="row" role="menuitemradio" aria-checked={view === 'details'} on:click={() => run("view:details")}>
        <Icon name="details" size={15} /> {$t('view.details')}
        {#if view === 'details'}<span class="check"><Icon name="check" size={14} /></span>{/if}
      </button>
      <button class="row" role="menuitemradio" aria-checked={view === 'list'} on:click={() => run("view:list")}>
        <Icon name="sort" size={15} /> {$t('view.list')}
        {#if view === 'list'}<span class="check"><Icon name="check" size={14} /></span>{/if}
      </button>
      <button class="row" role="menuitemradio" aria-checked={view === 'icons'} on:click={() => run("view:icons")}>
        <Icon name="view" size={15} /> {$t('view.icons')}
        {#if view === 'icons'}<span class="check"><Icon name="check" size={14} /></span>{/if}
      </button>
      <button class="row" role="menuitemradio" aria-checked={view === 'gallery'} on:click={() => run("view:gallery")}>
        <Icon name="gallery" size={15} /> {$t('view.gallery')}
        {#if view === 'gallery'}<span class="check"><Icon name="check" size={14} /></span>{/if}
      </button>
    </Submenu>
    <!-- Sort by ▸ — drives the SAME `sortKey`/`sortDir` the column headers use; both checkmarked. -->
    <Submenu label={$t('cmd.sort')} icon="sort">
      <button class="row" role="menuitemradio" aria-checked={sortKey === 'name'} on:click={() => run("sort:name")}>
        <Icon name="rename" size={15} /> {$t('sort.name')}
        {#if sortKey === 'name'}<span class="check"><Icon name="check" size={14} /></span>{/if}
      </button>
      <button class="row" role="menuitemradio" aria-checked={sortKey === 'modified'} on:click={() => run("sort:modified")}>
        <Icon name="recent" size={15} /> {$t('sort.modified')}
        {#if sortKey === 'modified'}<span class="check"><Icon name="check" size={14} /></span>{/if}
      </button>
      <button class="row" role="menuitemradio" aria-checked={sortKey === 'type'} on:click={() => run("sort:type")}>
        <Icon name="filter" size={15} /> {$t('sort.type')}
        {#if sortKey === 'type'}<span class="check"><Icon name="check" size={14} /></span>{/if}
      </button>
      <button class="row" role="menuitemradio" aria-checked={sortKey === 'size'} on:click={() => run("sort:size")}>
        <Icon name="disk" size={15} /> {$t('sort.size')}
        {#if sortKey === 'size'}<span class="check"><Icon name="check" size={14} /></span>{/if}
      </button>
      <div class="sep" role="separator" />
      <button class="row" role="menuitemradio" aria-checked={sortDir === 'asc'} on:click={() => run("sortdir:asc")}>
        <Icon name="chev-up" size={15} /> {$t('cmd.ascending')}
        {#if sortDir === 'asc'}<span class="check"><Icon name="check" size={14} /></span>{/if}
      </button>
      <button class="row" role="menuitemradio" aria-checked={sortDir === 'desc'} on:click={() => run("sortdir:desc")}>
        <Icon name="chev-down" size={15} /> {$t('cmd.descending')}
        {#if sortDir === 'desc'}<span class="check"><Icon name="check" size={14} /></span>{/if}
      </button>
    </Submenu>
    <div class="sep" role="separator" />
    <button class="row" role="menuitem" on:click={() => run("select-all")}>
      <Icon name="check" size={15} /> {$t('ctx.selectAll')}
      <span class="hint">Ctrl+A</span>
    </button>
    <button class="row" role="menuitem" on:click={() => run("invert-selection")}>
      <Icon name="check" size={15} /> {$t('ctx.invertSelection')}
    </button>
    <button class="row" role="menuitem" on:click={() => run("select-pattern")}>
      <Icon name="filter" size={15} /> {$t('ctx.selectByPattern')}
    </button>
    <button class="row" role="menuitem" on:click={() => run("refresh")}>
      <Icon name="refresh" size={15} /> {$t('ctx.refresh')}
      <span class="hint">F5</span>
    </button>
    <div class="sep" role="separator" />
    {#if canTerminal}
      <button class="row" role="menuitem" on:click={() => run("terminal")}>
        <Icon name="code" size={15} /> {$t('ctx.openInTerminal')}
      </button>
      <button class="row" role="menuitem" on:click={() => run("open-folder-in-console")}>
        <Icon name="code" size={15} /> {$t('ctx.workOnFolder')}
      </button>
    {/if}
    <button class="row" role="menuitem" on:click={() => run("reveal")}>
      <Icon name="folder" size={15} /> {$t('ctx.reveal')}
    </button>
    {#if canTerminal}
      <button class="row" role="menuitem" on:click={() => run("properties-folder")}>
        <Icon name="info" size={15} /> {$t('ctx.properties')}
      </button>
    {/if}
    <div class="sep" role="separator" />
    <button class="row" role="menuitem" on:click={() => run("help-docs")}>
      <Icon name="book" size={15} /> Documents for this view
    </button>
  {/if}
</div>

<style>
  .ctx {
    position: fixed;
    z-index: 100;
    min-width: 210px;
    padding: 5px;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-lg);
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.16);
    outline: none;
  }
  .quickrow {
    display: flex;
    gap: 2px;
    padding: 2px;
  }
  .quickrow button {
    flex: 1;
    height: 34px;
    display: grid;
    place-items: center;
    border-radius: var(--radius);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    height: 32px;
    padding: 0 10px;
    text-align: left;
    border-radius: var(--radius);
    white-space: nowrap; /* menu items are always one line — the menu grows to fit (CPE-753) */
  }
  .hint {
    margin-left: auto;
    color: var(--text-faint);
    font-size: 12px;
  }
  /* Trailing ✓ marking the active view / sort key / direction — accent, per MENUS.md. */
  .check {
    margin-left: auto;
    display: inline-flex;
    color: var(--accent);
  }
  .sep {
    height: 1px;
    background: var(--border);
    margin: 4px 6px;
  }
</style>
