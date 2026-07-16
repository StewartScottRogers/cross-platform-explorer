<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import Icon from "./Icon.svelte";
  import { t } from "../i18n";
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
  /** Recently-visited folders (MRU). */
  export let recentFolders: RecentFile[] = [];

  const dispatch = createEventDispatcher<{
    navigate: string;
    openFile: string;
    unpin: string;
    unfavorite: string;
    removeRecent: string;
    removeRecentFolder: string;
    clearRecents: void;
  }>();

  let quickOpen = true;
  let recentOpen = true;
  /** Which pill tab is showing in the lower section. */
  let tab: "recent" | "favorites" | "folders" = "recent";

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
      title={quickOpen ? $t("home.collapse") : $t("home.expand")}
      on:click={() => (quickOpen = !quickOpen)}
    >
      <Icon name="chev-right" size={13} />
    </button>
    <span>{$t("home.quickAccess")}</span>
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
              title={$t("home.unpinQuick")}
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
      title={recentOpen ? $t("home.collapse") : $t("home.expand")}
      on:click={() => (recentOpen = !recentOpen)}
    >
      <Icon name="chev-right" size={13} />
    </button>
    <span>{tab === "favorites" ? $t("home.favorites") : tab === "folders" ? $t("home.recentFolders") : $t("home.recent")}</span>
    {#if tab === "recent" && recents.length > 0}
      <button class="clear" on:click={() => dispatch("clearRecents")}>{$t("home.clear")}</button>
    {/if}
  </div>

  {#if recentOpen}
    <div class="pills">
      <button class="pill" class:active={tab === "recent"} on:click={() => (tab = "recent")}>
        <Icon name="recent" size={14} /> {$t("home.recent")}
      </button>
      <button class="pill" class:active={tab === "favorites"} on:click={() => (tab = "favorites")}>
        <Icon name="star" size={14} /> {$t("home.favorites")}
      </button>
      <button class="pill" class:active={tab === "folders"} on:click={() => (tab = "folders")}>
        <Icon name="folder" size={14} /> {$t("home.folders")}
      </button>
      <button class="pill" disabled title={$t("home.sharedTip")}>
        <Icon name="people" size={14} /> {$t("home.shared")}
      </button>
    </div>

    {#if tab === "recent"}
      {#if recents.length === 0}
        <div class="empty-state">
          <span class="empty-icon"><Icon name="recent" size={36} /></span>
          <p>{$t("home.noRecent")}</p>
          <p style="font-size:12px">{$t("home.noRecentSub")}</p>
        </div>
      {:else}
        <div class="recent-list">
          <div class="recent-head">
            <span>{$t("home.name")}</span><span>{$t("home.dateOpened")}</span>
          </div>
          {#each recents as r (r.path)}
            <button class="recent-row" on:dblclick={() => dispatch("openFile", r.path)} on:click={() => dispatch("openFile", r.path)}>
              <span class="rname">
                <Icon name={iconFor({ is_dir: false, extension: extOf(r.name) })} />
                <span class="ellip">{r.name}</span>
              </span>
              <span class="rdate">{formatDate(r.opened)}</span>
              <!-- svelte-ignore a11y-no-static-element-interactions a11y-click-events-have-key-events -->
              <span
                class="rmv"
                role="button"
                tabindex="-1"
                aria-label={$t("home.removeFromRecent")}
                title={$t("home.removeFromRecent")}
                on:click|stopPropagation={() => dispatch("removeRecent", r.path)}
              >
                <Icon name="close" size={13} />
              </span>
            </button>
          {/each}
        </div>
      {/if}
    {:else if tab === "favorites"}
      {#if favorites.length === 0}
        <div class="empty-state">
          <span class="empty-icon"><Icon name="star" size={36} /></span>
          <p>{$t("home.noFavorites")}</p>
          <p style="font-size:12px">{$t("home.noFavoritesSub")}</p>
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
                title={$t("home.removeFromFavorites")}
                on:click|stopPropagation={() => dispatch("unfavorite", f.path)}
              >
                <Icon name="star" size={14} />
              </span>
            </button>
          {/each}
        </div>
      {/if}
    {:else}
      {#if recentFolders.length === 0}
        <div class="empty-state">
          <span class="empty-icon"><Icon name="folder" size={36} /></span>
          <p>{$t("home.noRecentFolders")}</p>
          <p style="font-size:12px">{$t("home.noRecentFoldersSub")}</p>
        </div>
      {:else}
        <div class="recent-list">
          {#each recentFolders as d (d.path)}
            <button
              class="recent-row fav-row"
              on:dblclick={() => dispatch("navigate", d.path)}
              on:click={() => dispatch("navigate", d.path)}
            >
              <span class="rname">
                <Icon name="folder" />
                <span class="ellip">{d.name}</span>
                <span class="fav-path ellip">{d.path}</span>
              </span>
              <!-- svelte-ignore a11y-no-static-element-interactions a11y-click-events-have-key-events -->
              <span
                class="rmv"
                role="button"
                tabindex="-1"
                aria-label={$t("home.removeFromRecentFolders")}
                title={$t("home.removeFromRecentFolders")}
                on:click|stopPropagation={() => dispatch("removeRecentFolder", d.path)}
              >
                <Icon name="close" size={13} />
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
    grid-template-columns: 1fr 170px 24px;
    padding: 4px 8px;
    color: var(--text-dim);
    font-size: 12px;
    border-bottom: 1px solid var(--border);
  }
  .recent-row {
    display: grid;
    grid-template-columns: 1fr 170px 24px;
    align-items: center;
    width: 100%;
    height: 30px;
    padding: 0 8px;
    border-radius: 4px;
    text-align: left;
  }
  .rmv { display: grid; place-items: center; color: var(--text-faint); border-radius: 4px; opacity: 0; }
  .recent-row:hover .rmv { opacity: 1; }
  .rmv:hover { background: var(--active); color: var(--text); }
  .rname { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .rdate { color: var(--text-dim); font-size: 12px; }
  .ellip { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  /* Favorites rows: name grows, star sits at the right edge (CPE-338). */
  .fav-row { grid-template-columns: 1fr auto; }
  .fav-path { color: var(--text-faint); font-size: 12px; margin-left: 4px; min-width: 0; }
  .fav-row .pin { opacity: 0; }
  .fav-row:hover .pin, .fav-row .pin.pinned { opacity: 1; }
</style>
