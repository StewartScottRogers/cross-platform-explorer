<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import Icon from "./Icon.svelte";
  import type { PathSegment } from "../format";

  export let crumbs: PathSegment[] = [];
  export let canBack = false;
  export let canForward = false;
  export let search = "";
  export let searchScope = "Home";

  const dispatch = createEventDispatcher<{
    back: void; forward: void; up: void; refresh: void;
    navigate: string; search: string;
  }>();
</script>

<div class="navbar">
  <button class="iconbtn" title="Back (Alt+Left)" disabled={!canBack} on:click={() => dispatch("back")}>
    <Icon name="back" />
  </button>
  <button class="iconbtn" title="Forward (Alt+Right)" disabled={!canForward} on:click={() => dispatch("forward")}>
    <Icon name="forward" />
  </button>
  <button class="iconbtn" title="Up (Backspace)" on:click={() => dispatch("up")}>
    <Icon name="up" />
  </button>
  <button class="iconbtn" title="Refresh (F5)" on:click={() => dispatch("refresh")}>
    <Icon name="refresh" />
  </button>

  <nav class="address" aria-label="Current path">
    {#each crumbs as crumb, i (crumb.path)}
      {#if i === crumbs.length - 1}
        <span class="crumb current" aria-current="page">{crumb.name}</span>
      {:else}
        <button class="crumb" on:click={() => dispatch("navigate", crumb.path)}>{crumb.name}</button>
        <span class="crumb-sep" aria-hidden="true"><Icon name="chev-right" size={12} /></span>
      {/if}
    {/each}
  </nav>

  <div class="search">
    <Icon name="search" size={14} />
    <input
      type="text"
      placeholder="Search {searchScope}"
      aria-label="Search {searchScope}"
      value={search}
      on:input={(e) => dispatch("search", e.currentTarget.value)}
    />
  </div>
</div>
