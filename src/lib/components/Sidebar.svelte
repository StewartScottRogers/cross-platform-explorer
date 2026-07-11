<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Icon from "./Icon.svelte";
  import type { DirEntry, Place } from "../types";

  export let places: Place[] = [];
  export let drives: Place[] = [];
  export let currentPath = "";
  export let isHome = false;

  const dispatch = createEventDispatcher<{ navigate: string; home: void }>();

  // Lazily-loaded children per path, and which nodes are expanded.
  let expanded = new Set<string>();
  let children: Record<string, DirEntry[]> = {};
  let loadingPaths = new Set<string>();

  async function toggle(path: string) {
    if (expanded.has(path)) {
      expanded.delete(path);
      expanded = expanded;
      return;
    }
    expanded.add(path);
    expanded = expanded;

    if (!children[path]) {
      loadingPaths.add(path);
      loadingPaths = loadingPaths;
      try {
        const entries = await invoke<DirEntry[]>("list_dir", { path });
        children[path] = entries.filter((e) => e.is_dir).sort((a, b) => a.name.localeCompare(b.name));
        children = children;
      } catch {
        // Unreadable folder: record an empty child list so the UI shows
        // "empty" rather than spinning forever.
        children[path] = [];
        children = children;
      } finally {
        loadingPaths.delete(path);
        loadingPaths = loadingPaths;
      }
    }
  }
</script>

<div class="sidebar">
  <button class="nav-item" class:active={isHome} on:click={() => dispatch("home")}>
    <span class="twisty hidden" />
    <Icon name="home" />
    <span class="label">Home</span>
  </button>
  <button class="nav-item" disabled title="Gallery — not implemented yet">
    <span class="twisty hidden" />
    <Icon name="gallery" />
    <span class="label">Gallery</span>
  </button>

  <div class="sidebar-sep" />

  {#each places as place (place.path)}
    {@const open = expanded.has(place.path)}
    <div>
      <div class="nav-item" class:active={!isHome && currentPath === place.path}>
        <button class="twisty" class:open title={open ? "Collapse" : "Expand"} on:click={() => toggle(place.path)}>
          <Icon name="chev-right" size={12} />
        </button>
        <Icon name={place.kind} />
        <button class="label" style="text-align:left" on:click={() => dispatch("navigate", place.path)}>
          {place.name}
        </button>
      </div>
      {#if open}
        <div class="nav-children">
          {#if loadingPaths.has(place.path)}
            <div class="nav-empty">Loading…</div>
          {:else if (children[place.path] ?? []).length === 0}
            <div class="nav-empty">No folders</div>
          {:else}
            {#each children[place.path] as child (child.path)}
              <button
                class="nav-item"
                class:active={!isHome && currentPath === child.path}
                on:click={() => dispatch("navigate", child.path)}
              >
                <span class="twisty hidden" />
                <Icon name="folder" />
                <span class="label">{child.name}</span>
              </button>
            {/each}
          {/if}
        </div>
      {/if}
    </div>
  {/each}

  <div class="sidebar-sep" />

  {#each drives as drive (drive.path)}
    {@const open = expanded.has(drive.path)}
    <div>
      <div class="nav-item" class:active={!isHome && currentPath === drive.path}>
        <button class="twisty" class:open title={open ? "Collapse" : "Expand"} on:click={() => toggle(drive.path)}>
          <Icon name="chev-right" size={12} />
        </button>
        <Icon name="drive" />
        <button class="label" style="text-align:left" on:click={() => dispatch("navigate", drive.path)}>
          {drive.name}
        </button>
      </div>
      {#if open}
        <div class="nav-children">
          {#if loadingPaths.has(drive.path)}
            <div class="nav-empty">Loading…</div>
          {:else if (children[drive.path] ?? []).length === 0}
            <div class="nav-empty">No folders</div>
          {:else}
            {#each children[drive.path] as child (child.path)}
              <button
                class="nav-item"
                class:active={!isHome && currentPath === child.path}
                on:click={() => dispatch("navigate", child.path)}
              >
                <span class="twisty hidden" />
                <Icon name="folder" />
                <span class="label">{child.name}</span>
              </button>
            {/each}
          {/if}
        </div>
      {/if}
    </div>
  {/each}
</div>
