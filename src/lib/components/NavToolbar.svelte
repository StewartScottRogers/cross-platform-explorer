<script lang="ts">
  import { createEventDispatcher, tick } from "svelte";
  import Icon from "./Icon.svelte";
  import type { PathSegment } from "../format";
  import { t } from "../i18n";

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
    back: void; forward: void; up: void; refresh: void; browse: void; help: void; diskusage: void;
    navigate: string; search: string; pathError: string; searchDeep: string; searchDocs: void;
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
  <button class="iconbtn" title="{$t('nav.back')} (Alt+Left)" disabled={!canBack} on:click={() => dispatch("back")}>
    <Icon name="back" />
  </button>
  <button class="iconbtn" title="{$t('nav.forward')} (Alt+Right)" disabled={!canForward} on:click={() => dispatch("forward")}>
    <Icon name="forward" />
  </button>
  <button class="iconbtn" title="{$t('nav.up')} (Alt+Up / Backspace)" on:click={() => dispatch("up")}>
    <Icon name="up" />
  </button>
  <button class="iconbtn" title="{$t('nav.refresh')} (F5)" on:click={() => dispatch("refresh")}>
    <Icon name="refresh" />
  </button>
  <button class="iconbtn" title="Disk usage — analyze folder sizes" aria-label="Disk usage" on:click={() => dispatch("diskusage")}>
    <Icon name="disk" size={18} />
  </button>
  <button class="iconbtn docsbtn" title="Documents for this section (F1)" aria-label="Documents for this section" on:click={() => dispatch("help")}>
    <Icon name="book" size={18} /><span class="docsbtn-label">Docs</span>
  </button>
  <button class="iconbtn" title="Browse for a folder…" aria-label="Browse for a folder" on:click={() => dispatch("browse")}>
    <Icon name="folder" />
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
      placeholder="{$t('nav.search')} {searchScope}"
      aria-label="{$t('nav.search')} {searchScope}"
      title={$t("nav.searchHint")}
      value={search}
      on:keydown|stopPropagation={(e) => {
        if (e.key === "Escape") { dispatch("search", ""); e.currentTarget.blur(); }
        // Enter escalates to a recursive, wildcard-capable search of this folder + subfolders (CPE-866).
        else if (e.key === "Enter") { const v = e.currentTarget.value.trim(); if (v) dispatch("searchDeep", v); }
      }}
      on:input={(e) => dispatch("search", e.currentTarget.value)}
    />
    <button
      class="search-docs"
      type="button"
      title="Search options — open documentation"
      aria-label="Search options documentation"
      on:click={() => dispatch("searchDocs")}
    >
      <Icon name="book" size={13} />
    </button>
  </div>
</div>

<style>
  /* Docs affordance inside the search box (CPE-927): opens the search-options page. */
  .search-docs {
    flex: 0 0 auto; display: inline-flex; align-items: center; justify-content: center;
    width: 20px; height: 20px; padding: 0; border: none; border-radius: 4px;
    background: transparent; color: var(--text-dim); cursor: pointer;
  }
  .search-docs:hover { background: rgba(128, 128, 128, 0.18); color: var(--text); }
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
