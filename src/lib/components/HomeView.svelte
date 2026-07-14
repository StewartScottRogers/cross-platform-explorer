<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import Icon from "./Icon.svelte";
  import { formatDate } from "../datetime";
  import { iconFor } from "../filetypes";
  import type { Place, RecentFile, Favorite } from "../types";

  export let places: Place[] = [];
  export let drives: Place[] = [];
  /** User-pinned folder paths. */
  export let pins: string[] = [];
  export let recents: RecentFile[] = [];
  /** User-starred files and folders. */
  export let favorites: Favorite[] = [];

  const dispatch = createEventDispatcher<{
    navigate: string;
    openFile: string;
    unpin: string;
    unfavorite: string;
    clearRecents: void;
  }>();

  let quickOpen = true;
  let recentOpen = true;
  /** Which pill tab is showing in the lower section. */
  let tab: "recent" | "favorites" = "recent";

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
    <span>{tab === "favorites" ? "Favorites" : "Recent"}</span>
    {#if tab === "recent" && recents.length > 0}
      <button class="clear" on:click={() => dispatch("clearRecents")}>Clear</button>
    {/if}
  </div>

  {#if recentOpen}
    <div class="pills">
      <button class="pill" class:active={tab === "recent"} on:click={() => (tab = "recent")}>
        <Icon name="recent" size={14} /> Recent
      </button>
      <button class="pill" class:active={tab === "favorites"} on:click={() => (tab = "favorites")}>
        <Icon name="star" size={14} /> Favorites
      </button>
      <button class="pill" disabled title="Shared — not implemented yet">
        <Icon name="people" size={14} /> Shared
      </button>
    </div>

    {#if tab === "recent"}
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
                <Icon name={iconFor({ is_dir: false, extension: extOf(r.name) })} />
                <span class="ellip">{r.name}</span>
              </span>
              <span class="rdate">{formatDate(r.opened)}</span>
            </button>
          {/each}
        </div>
      {/if}
    {:else}
      {#if favorites.length === 0}
        <div class="empty-state">
          <span class="empty-icon"><Icon name="star" size={36} /></span>
          <p>No favorites yet</p>
          <p style="font-size:12px">Right-click any file or folder → Add to Favorites.</p>
        </div>
      {:else}
        <div class="recent-list">
          {#each favorites as f (f.path)}
            <button
              class="recent-row fav-row"
              on:dblclick={() => dispatch(f.is_dir ? "navigate" : "openFile", f.path)}
              on:click={() => dispatch(f.is_dir ? "navigate" : "openFile", f.path)}
            >
              <span class="rname">
                <Icon name={f.is_dir ? "folder" : iconFor({ is_dir: false, extension: extOf(f.name) })} />
                <span class="ellip">{f.name}</span>
                <span class="fav-path ellip">{f.path}</span>
              </span>
              <!-- svelte-ignore a11y-no-static-element-interactions a11y-click-events-have-key-events -->
              <span
                class="pin pinned"
                role="button"
                tabindex="-1"
                title="Remove from Favorites"
                on:click|stopPropagation={() => dispatch("unfavorite", f.path)}
              >
                <Icon name="star" size={14} />
              </span>
            </button>
          {/each}
        </div>
      {/if}
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
  /* Favorites rows: name grows, star sits at the right edge (CPE-338). */
  .fav-row { grid-template-columns: 1fr auto; }
  .fav-path { color: var(--text-faint); font-size: 12px; margin-left: 4px; min-width: 0; }
  .fav-row .pin { opacity: 0; }
  .fav-row:hover .pin, .fav-row .pin.pinned { opacity: 1; }
</style>
