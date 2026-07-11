<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Icon from "./Icon.svelte";
  import type { DirEntry, Place } from "../types";

  export let places: Place[] = [];
  export let drives: Place[] = [];
  export let currentPath = "";
  export let isHome = false;
  /** Paths currently being dragged from the file list (CPE-043). */
  export let draggedPaths: string[] = [];

  const dispatch = createEventDispatcher<{
    navigate: string;
    home: void;
    drop: { paths: string[]; dest: string; copy: boolean };
  }>();

  /** The sidebar path currently hovered as a drop target, or "" for none. */
  let dropPath = "";

  /** Normalise separators so parent/child checks work on both platforms. */
  const norm = (p: string) => p.replace(/\\/g, "/").replace(/\/+$/, "");

  /**
   * A drop is valid only onto a folder that is not one of the dragged items and
   * not inside one of them — dropping a folder into its own descendant would
   * move a directory inside itself.
   */
  function validTarget(dest: string): boolean {
    if (draggedPaths.length === 0 || !dest) return false;
    const d = norm(dest);
    return !draggedPaths.some((p) => {
      const s = norm(p);
      return d === s || d.startsWith(s + "/");
    });
  }

  function onDragOver(e: DragEvent, dest: string) {
    if (!validTarget(dest)) return;
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = e.ctrlKey ? "copy" : "move";
    dropPath = dest;
  }

  function onDrop(e: DragEvent, dest: string) {
    if (!validTarget(dest)) return;
    e.preventDefault();
    const paths = [...draggedPaths];
    const copy = e.ctrlKey;
    dropPath = "";
    dispatch("drop", { paths, dest, copy });
  }

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
        children[path] = entries
          .filter((e) => e.is_dir)
          .sort((a, b) => a.name.localeCompare(b.name));
        children = children;
      } catch {
        // Unreadable folder: record an empty child list so the UI shows
        // "No folders" rather than spinning forever.
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

  {#each [...places, ...drives] as place, i (place.path)}
    {@const open = expanded.has(place.path)}
    {@const isDrive = i >= places.length}
    {#if isDrive && i === places.length}
      <div class="sidebar-sep" />
    {/if}
    <div>
      <!-- svelte-ignore a11y-no-static-element-interactions -->
      <div
        class="nav-item"
        class:active={!isHome && currentPath === place.path}
        class:droptarget={dropPath === place.path}
        on:dragover={(e) => onDragOver(e, place.path)}
        on:dragleave={() => (dropPath = dropPath === place.path ? "" : dropPath)}
        on:drop={(e) => onDrop(e, place.path)}
      >
        <button
          class="twisty"
          class:open
          title={open ? "Collapse" : "Expand"}
          on:click={() => toggle(place.path)}
        >
          <Icon name="chev-right" size={12} />
        </button>
        <Icon name={place.kind} />
        <button
          class="label"
          style="text-align:left"
          on:click={() => dispatch("navigate", place.path)}
        >
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
                class:droptarget={dropPath === child.path}
                on:click={() => dispatch("navigate", child.path)}
                on:dragover={(e) => onDragOver(e, child.path)}
                on:dragleave={() =>
                  (dropPath = dropPath === child.path ? "" : dropPath)}
                on:drop={(e) => onDrop(e, child.path)}
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

<style>
  /* Only valid targets ever highlight, so an illegal drop is visibly impossible
     rather than merely rejected after the fact. */
  .nav-item.droptarget {
    background: var(--selection);
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }
</style>
