<script lang="ts">
  import { createEventDispatcher, onMount } from "svelte";
  import Icon from "./Icon.svelte";

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
  /** True when the selection can be packed into a .zip (CPE-251). */
  export let compressible = false;
  /** True when exactly one archive file is selected — enables Extract (CPE-252). */
  export let extractable = false;

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
      <Icon name={openIcon} size={15} /> Open
    </button>
    {#if executableSelected}
      <button class="row" role="menuitem" on:click={() => run("execute")}>
        <Icon name="executable" size={15} /> Execute
      </button>
      <button class="row" role="menuitem" on:click={() => run("execute-admin")}>
        <Icon name="executable" size={15} /> Execute as administrator
      </button>
    {/if}
    {#if folderSelected}
      <button class="row" role="menuitem" on:click={() => run("open-new-tab")}>
        <Icon name="plus" size={15} /> Open in new tab
      </button>
    {/if}
    <button class="row" role="menuitem" on:click={() => run("duplicate")}>
      <Icon name="copy" size={15} /> Duplicate
      <span class="hint">Ctrl+D</span>
    </button>
    <button class="row" role="menuitem" on:click={() => run("copy-path")}>
      <Icon name="paste" size={15} /> Copy as path
      <span class="hint">Ctrl+Shift+C</span>
    </button>
    <button class="row" role="menuitem" on:click={() => run("copy-name")}>
      <Icon name="rename" size={15} /> Copy name
    </button>
    {#if extractable}
      <button class="row" role="menuitem" on:click={() => run("extract")}>
        <Icon name="archive" size={15} /> Extract
      </button>
    {/if}
    {#if compressible}
      <button class="row" role="menuitem" on:click={() => run("compress")}>
        <Icon name="archive" size={15} /> Compress to ZIP
      </button>
    {/if}
    {#if folderSelected}
      <button class="row" role="menuitem" on:click={() => run("pin")}>
        <Icon name="pin" size={15} /> {pinned ? "Unpin from Home" : "Pin to Home"}
      </button>
    {/if}
    <div class="sep" />
    <button class="row" role="menuitem" on:click={() => run("reveal")}>
      <Icon name="folder" size={15} /> Reveal in File Explorer
    </button>
    <button class="row" role="menuitem" on:click={() => run("properties")}>
      <Icon name="info" size={15} /> Properties
      <span class="hint">Alt+Enter</span>
    </button>
  {:else}
    <button class="row" role="menuitem" on:click={() => run("new-folder")}>
      <Icon name="folder" size={15} /> New folder
      <span class="hint">Ctrl+Shift+N</span>
    </button>
    <button class="row" role="menuitem" disabled={!canPaste} on:click={() => run("paste")}>
      <Icon name="paste" size={15} /> Paste
      <span class="hint">Ctrl+V</span>
    </button>
    <div class="sep" />
    <button class="row" role="menuitem" on:click={() => run("select-all")}>
      <Icon name="check" size={15} /> Select all
      <span class="hint">Ctrl+A</span>
    </button>
    <button class="row" role="menuitem" on:click={() => run("refresh")}>
      <Icon name="refresh" size={15} /> Refresh
      <span class="hint">F5</span>
    </button>
    <div class="sep" />
    <button class="row" role="menuitem" on:click={() => run("reveal")}>
      <Icon name="folder" size={15} /> Reveal in File Explorer
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
  }
  .hint {
    margin-left: auto;
    color: var(--text-faint);
    font-size: 12px;
  }
  .sep {
    height: 1px;
    background: var(--border);
    margin: 4px 6px;
  }
</style>
