<script lang="ts">
  import { createEventDispatcher, tick } from "svelte";
  import Icon from "./Icon.svelte";
  import type { PathSegment } from "../format";
  import type { DensityMode } from "../types";
  import { t } from "../i18n";
  import { displaySafeName } from "../filename";

  export let crumbs: PathSegment[] = [];
  // Row/chrome density (CPE-1526 threaded the prop, CPE-1528 consumes it): "compact" applies the
  // `.compact` CSS class on the root `.navbar` (see app.css's "Compact density (CPE-1528)" rules) —
  // tighter control padding + icon-only buttons (the Docs button's text label hides; every button
  // still carries its `title`/`aria-label` so it stays reachable/labelled for accessibility).
  // "comfortable" (the default) renders pixel-identical to before this ticket.
  export let density: DensityMode = "comfortable";
  export let canBack = false;
  export let canForward = false;
  export let search = "";
  export let searchScope = "Home";
  export let currentPath = "";
  /** Bound from the parent so Ctrl+L can switch us into edit mode. */
  export let editingPath = false;
  /** CPE-1979 — true when a VIEW is layered over `currentPath`'s own listing (the in-app archive
   *  browser, a smart folder, a saved structured search). App.svelte derives it as `pathOverlaidByView`
   *  and that is the single declaration; it is passed in rather than recomputed here because this
   *  component cannot see any of the three.
   *
   *  {@link commit} needs it. Its "nothing would change, don't re-navigate" short-circuit compares the
   *  typed value against `currentPath`, and `currentPath` stops being the thing on screen the moment one
   *  of those views opens: entering an archive never moves `currentPath` (App.svelte's `enterArchive`
   *  sets `archive` and leaves history alone), so the address bar goes on displaying the CONTAINING
   *  folder while the listing shows the archive's inner entries. Re-entering that same folder path is
   *  then the user's most natural "get me back out", and the equality test swallowed it — no `navigate`
   *  dispatch, so `onCrumbNavigate`'s `exitArchive()` and `loadPath`'s `archive = null` (the single
   *  chokepoint that dismisses all three views) were both unreachable from here.
   *
   *  Measured, not theorised: this is the whole of CPE-1979. Of the 81 completed `gui-smoke` shard-2
   *  jobs in the 16h50m window 2026-08-28T00:21Z–17:11Z, 77 have a retrievable log and reached this
   *  transition (the other 4 were cancelled and were never inspected); in **77 of those 77** the
   *  harness's between-spec `resetAppState` failed
   *  on exactly this — `expected the breadcrumb to show "cpe-gui-smoke-XXXXXX"` — because
   *  `archive-browse.smoke.ts` leaves the app inside a `.tar.gz` and the reset's address-bar navigation
   *  back to the same tmp dir was a no-op for 15s while `[aria-current="page"]` kept reading
   *  `CPE-1181-archive.tar.gz`. */
  export let pathOverlaidByView = false;
  /** Recent folder paths, offered as address-bar autocomplete (CPE-361). */
  export let recentPaths: string[] = [];

  const dispatch = createEventDispatcher<{
    back: void; forward: void; up: void; refresh: void; browse: void; help: void; diskusage: void;
    navigate: string; search: string; pathError: string; searchDeep: string; searchDocs: void;
    density: DensityMode;
  }>();

  let pathInput: HTMLInputElement | undefined;
  let searchInput: HTMLInputElement | undefined;
  let addressEl: HTMLElement | undefined;
  let draft = "";

  // On a deep path the crumb strip overflows the bar (`.address` is `min-width: 0; overflow-x: auto`
  // with a hidden scrollbar, so it scrolls INSIDE the bar rather than widening the page — CPE-1249
  // overflow review). Anchor the scroll at the LEFT so the Home / drive-root crumbs stay visible and
  // clickable, and the deep tail scrolls off the RIGHT edge (reachable by scrolling the bar). Anchoring
  // at the end instead would left-truncate Home out of view, breaking "click Home/a parent crumb" — the
  // reachability the address bar exists for (revises CPE-343's scroll-to-end now that the bar truly
  // scrolls internally instead of expanding the whole navbar).
  $: if (crumbs && !editingPath) scrollAddressToStart();
  async function scrollAddressToStart() {
    await tick();
    if (addressEl) addressEl.scrollLeft = 0;
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
    // CPE-1979: the equality short-circuit only holds while `currentPath` IS what is on screen — see
    // {@link pathOverlaidByView}. With a view layered over it, re-entering the same path is the one
    // gesture that dismisses that view, so it must reach the parent.
    if (!value || (value === currentPath && !pathOverlaidByView)) return;
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

<div class="navbar" class:compact={density === "compact"}>
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
  <!-- Instant density toggle (CPE-1529, capstone of epic CPE-1488): flips comfortable <-> compact on a
       dime — no dialog. Dispatches the new value; the parent's `setDensity` (App.svelte, CPE-1526)
       updates the reactive `density` prop threaded to every consumer and persists it via
       `settings.saveDensity`. `aria-pressed` + the `.on` class (same convention as CommandBar's
       `.cmd.on`) reflect the CURRENT state so the control reads as a toggle, not a one-shot action. -->
  <button
    class="iconbtn"
    class:on={density === "compact"}
    title={density === "compact" ? "Switch to comfortable density" : "Switch to compact density"}
    aria-label={density === "compact" ? "Switch to comfortable density" : "Switch to compact density"}
    aria-pressed={density === "compact"}
    on:click={() => dispatch("density", density === "compact" ? "comfortable" : "compact")}
  >
    <Icon name="density" size={18} />
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
          <span class="crumb current" aria-current="page">{displaySafeName(crumb.name)}</span>
        {:else}
          <button class="crumb" on:click|stopPropagation={() => dispatch("navigate", crumb.path)}>
            {displaySafeName(crumb.name)}
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
      aria-label="Open the search-options documentation"
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
  /* Compact density (CPE-1528): the address-edit input mirrors the compact `.address`/`.search`
     height set globally in app.css, so it doesn't stick out taller than its siblings. */
  .navbar.compact .pathedit { height: 26px; font-size: 11.5px; }
</style>
