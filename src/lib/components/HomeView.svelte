<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import Icon from "./Icon.svelte";
  import type { Place } from "../types";

  export let places: Place[] = [];
  export let drives: Place[] = [];

  const dispatch = createEventDispatcher<{ navigate: string }>();

  let quickOpen = true;
  let recentOpen = true;

  // Explorer shows Recent / Favorites / Shared here. We do not track recent
  // files, so rather than fabricate a list, Recent shows an honest empty state
  // and the other two pills are disabled.
  const tab = "recent";
</script>

<div class="home">
  <div class="section-head">
    <button class="twisty" class:open={quickOpen} on:click={() => (quickOpen = !quickOpen)}
            title={quickOpen ? "Collapse" : "Expand"}>
      <Icon name="chev-right" size={13} />
    </button>
    <span>Quick access</span>
  </div>

  {#if quickOpen}
    <div class="qa-grid">
      {#each [...places, ...drives] as place (place.path)}
        <button class="qa-card" on:click={() => dispatch("navigate", place.path)}>
          <Icon name={place.kind} size={28} />
          <span class="qa-text">
            <span class="qa-name">{place.name}</span>
            <span class="qa-sub">{place.path}</span>
          </span>
          <span class="pin"><Icon name="pin" size={13} /></span>
        </button>
      {/each}
    </div>
  {/if}

  <div class="section-head">
    <button class="twisty" class:open={recentOpen} on:click={() => (recentOpen = !recentOpen)}
            title={recentOpen ? "Collapse" : "Expand"}>
      <Icon name="chev-right" size={13} />
    </button>
    <span>Recent</span>
  </div>

  {#if recentOpen}
    <div class="pills">
      <button class="pill active" aria-pressed={tab === "recent"}>
        <Icon name="recent" size={14} /> Recent
      </button>
      <button class="pill" disabled title="Favorites — not implemented yet">
        <Icon name="star" size={14} /> Favorites
      </button>
      <button class="pill" disabled title="Shared — not implemented yet">
        <Icon name="people" size={14} /> Shared
      </button>
    </div>

    <div class="empty-state">
      <span class="empty-icon"><Icon name="recent" size={36} /></span>
      <p>No recent files yet</p>
      <p style="font-size:12px">Files you open in this app will appear here.</p>
    </div>
  {/if}
</div>
