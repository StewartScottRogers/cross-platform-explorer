<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import Icon from "./Icon.svelte";
  import { formatDate } from "../datetime";
  import { categoryOf } from "../filetypes";
  import type { Place, RecentFile } from "../types";

  export let places: Place[] = [];
  export let drives: Place[] = [];
  /** User-pinned folder paths. */
  export let pins: string[] = [];
  export let recents: RecentFile[] = [];

  const dispatch = createEventDispatcher<{
    navigate: string;
    openFile: string;
    unpin: string;
    clearRecents: void;
  }>();

  let quickOpen = true;
  let recentOpen = true;

  // Pinned folders appear alongside the built-in places.
  $: pinned = pins.map((p) => ({
    name: p.split(/[\\/]/).filter(Boolean).pop() ?? p,
    path: p,
    kind: "folder",
  }));
  $: cards = [...places, ...drives, ...pinned];

  const extOf = (name: string) => {
    const i = name.lastIndexOf(".");
    return i > 0 ? name.slice(i + 1).toLowerCase() : "";
  };
</script>

<div class="home">
  <div class="section-head">
    <button
      class="twisty"
      class:open={quickOpen}
      title={quickOpen ? "Collapse" : "Expand"}
      on:click={() => (quickOpen = !quickOpen)}
    >
      <Icon name="chev-right" size={13} />
    </button>
    <span>Quick access</span>
  </div>

  {#if quickOpen}
    <div class="qa-grid">
      {#each cards as place (place.path)}
        <button class="qa-card" on:click={() => dispatch("navigate", place.path)}>
          <Icon name={place.kind} size={28} />
          <span class="qa-text">
            <span class="qa-name">{place.name}</span>
            <span class="qa-sub">{place.path}</span>
          </span>
          {#if pins.includes(place.path)}
            <!-- svelte-ignore a11y-no-static-element-interactions a11y-click-events-have-key-events -->
            <span
              class="pin pinned"
              role="button"
              tabindex="-1"
              title="Unpin from Quick access"
              on:click|stopPropagation={() => dispatch("unpin", place.path)}
            >
              <Icon name="pin" size={13} />
            </span>
          {:else}
            <span class="pin"><Icon name="pin" size={13} /></span>
          {/if}
        </button>
      {/each}
    </div>
  {/if}

  <div class="section-head">
    <button
      class="twisty"
      class:open={recentOpen}
      title={recentOpen ? "Collapse" : "Expand"}
      on:click={() => (recentOpen = !recentOpen)}
    >
      <Icon name="chev-right" size={13} />
    </button>
    <span>Recent</span>
    {#if recents.length > 0}
      <button class="clear" on:click={() => dispatch("clearRecents")}>Clear</button>
    {/if}
  </div>

  {#if recentOpen}
    <div class="pills">
      <button class="pill active"><Icon name="recent" size={14} /> Recent</button>
      <button class="pill" disabled title="Favorites — not implemented yet">
        <Icon name="star" size={14} /> Favorites
      </button>
      <button class="pill" disabled title="Shared — not implemented yet">
        <Icon name="people" size={14} /> Shared
      </button>
    </div>

    {#if recents.length === 0}
      <div class="empty-state">
        <span class="empty-icon"><Icon name="recent" size={36} /></span>
        <p>No recent files yet</p>
        <p style="font-size:12px">Files you open in this app will appear here.</p>
      </div>
    {:else}
      <div class="recent-list">
        <div class="recent-head">
          <span>Name</span><span>Date opened</span>
        </div>
        {#each recents as r (r.path)}
          <button class="recent-row" on:dblclick={() => dispatch("openFile", r.path)} on:click={() => dispatch("openFile", r.path)}>
            <span class="rname">
              <Icon name={categoryOf({ is_dir: false, extension: extOf(r.name) })} />
              <span class="ellip">{r.name}</span>
            </span>
            <span class="rdate">{formatDate(r.opened)}</span>
          </button>
        {/each}
      </div>
    {/if}
  {/if}
</div>

<style>
  .clear {
    margin-left: auto;
    font-size: 12px;
    color: var(--accent);
    padding: 2px 8px;
    border-radius: 4px;
  }
  .pin.pinned { color: var(--accent); }
  .recent-list { max-width: 860px; }
  .recent-head {
    display: grid;
    grid-template-columns: 1fr 170px;
    padding: 4px 8px;
    color: var(--text-dim);
    font-size: 12px;
    border-bottom: 1px solid var(--border);
  }
  .recent-row {
    display: grid;
    grid-template-columns: 1fr 170px;
    align-items: center;
    width: 100%;
    height: 30px;
    padding: 0 8px;
    border-radius: 4px;
    text-align: left;
  }
  .rname { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .rdate { color: var(--text-dim); font-size: 12px; }
  .ellip { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
