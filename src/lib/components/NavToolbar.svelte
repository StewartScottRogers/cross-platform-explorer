<script lang="ts">
  import { createEventDispatcher, tick } from "svelte";
  import Icon from "./Icon.svelte";
  import type { PathSegment } from "../format";

  export let crumbs: PathSegment[] = [];
  export let canBack = false;
  export let canForward = false;
  export let search = "";
  export let searchScope = "Home";
  export let currentPath = "";
  /** Bound from the parent so Ctrl+L can switch us into edit mode. */
  export let editingPath = false;
  /** Recent folder paths, offered as address-bar autocomplete (CPE-361). */
  export let recentPaths: string[] = [];

  const dispatch = createEventDispatcher<{
    back: void; forward: void; up: void; refresh: void;
    navigate: string; search: string; pathError: string;
  }>();

  let pathInput: HTMLInputElement | undefined;
  let searchInput: HTMLInputElement | undefined;
  let addressEl: HTMLElement | undefined;
  let draft = "";

  // On a deep path the crumb strip overflows (address is overflow-x:auto with a
  // hidden scrollbar). Default scroll is the left/root end, which hides the crumb
  // you're actually in. Scroll to the end so the current folder stays visible
  // whenever the path changes (CPE-343).
  $: if (crumbs && !editingPath) scrollAddressToEnd();
  async function scrollAddressToEnd() {
    await tick();
    if (addressEl) addressEl.scrollLeft = addressEl.scrollWidth;
  }

  // When the parent flips editingPath on (Ctrl+L / Alt+D), seed and focus.
  $: if (editingPath) startEdit();

  async function startEdit() {
    draft = currentPath;
    await tick();
    pathInput?.focus();
    pathInput?.select();
  }

  export function focusSearch() {
    searchInput?.focus();
    searchInput?.select();
  }

  function commit() {
    const value = draft.trim();
    editingPath = false;
    if (!value || value === currentPath) return;
    dispatch("navigate", value);
  }

  function onKey(e: KeyboardEvent) {
    e.stopPropagation(); // don't let list shortcuts fire while typing a path
    if (e.key === "Enter") {
      e.preventDefault();
      commit();
    } else if (e.key === "Escape") {
      e.preventDefault();
      editingPath = false;
    }
  }
</script>

<div class="navbar">
  <button class="iconbtn" title="Back (Alt+Left)" disabled={!canBack} on:click={() => dispatch("back")}>
    <Icon name="back" />
  </button>
  <button class="iconbtn" title="Forward (Alt+Right)" disabled={!canForward} on:click={() => dispatch("forward")}>
    <Icon name="forward" />
  </button>
  <button class="iconbtn" title="Up (Alt+Up / Backspace)" on:click={() => dispatch("up")}>
    <Icon name="up" />
  </button>
  <button class="iconbtn" title="Refresh (F5)" on:click={() => dispatch("refresh")}>
    <Icon name="refresh" />
  </button>

  {#if editingPath}
    <input
      class="pathedit"
      list="recent-paths"
      bind:this={pathInput}
      bind:value={draft}
      spellcheck="false"
      aria-label="Address"
      placeholder="Type a path"
      on:keydown={onKey}
      on:blur={() => (editingPath = false)}
    />
    <datalist id="recent-paths">
      {#each recentPaths as p (p)}<option value={p}></option>{/each}
    </datalist>
  {:else}
    <!-- svelte-ignore a11y-no-static-element-interactions a11y-click-events-have-key-events a11y-no-noninteractive-element-interactions -->
    <nav
      class="address"
      bind:this={addressEl}
      aria-label="Current path"
      title="Click the empty area to type a path (Ctrl+L)"
      on:click={(e) => {
        // Clicking the blank part of the bar (not a crumb) starts editing,
        // which is how Explorer behaves.
        if (e.target === e.currentTarget) editingPath = true;
      }}
    >
      {#each crumbs as crumb, i (crumb.path)}
        {#if i === crumbs.length - 1}
          <span class="crumb current" aria-current="page">{crumb.name}</span>
        {:else}
          <button class="crumb" on:click|stopPropagation={() => dispatch("navigate", crumb.path)}>
            {crumb.name}
          </button>
          <span class="crumb-sep" aria-hidden="true"><Icon name="chev-right" size={12} /></span>
        {/if}
      {/each}
    </nav>
  {/if}

  <div class="search">
    <Icon name="search" size={14} />
    <input
      type="text"
      bind:this={searchInput}
      placeholder="Search {searchScope}"
      aria-label="Search {searchScope}"
      value={search}
      on:keydown|stopPropagation={(e) => {
        if (e.key === "Escape") { dispatch("search", ""); e.currentTarget.blur(); }
      }}
      on:input={(e) => dispatch("search", e.currentTarget.value)}
    />
  </div>
</div>

<style>
  .pathedit {
    flex: 1;
    height: 34px;
    margin-left: 4px;
    padding: 0 10px;
    font: inherit;
    font-family: ui-monospace, monospace;
    font-size: 12.5px;
    color: var(--text);
    background: #fff;
    border: 1px solid var(--accent);
    border-radius: var(--radius);
    outline: none;
  }
</style>
