<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Icon from "./Icon.svelte";
  import SidebarNode from "./SidebarNode.svelte";
  import { iconFor } from "../filetypes";
  import type { DirEntry, Place, Favorite } from "../types";

  export let places: Place[] = [];
  export let drives: Place[] = [];
  /** User-starred files and folders, shown in the quick-access section (CPE-340). */
  export let favorites: Favorite[] = [];
  export let currentPath = "";
  export let isHome = false;
  /** The middle pane's currently selected folder (or ""), for two-way highlight
      sync (CPE-236). */
  export let selectedPath = "";
  /** Paths currently being dragged from the file list (CPE-043). */
  export let draggedPaths: string[] = [];

  const dispatch = createEventDispatcher<{
    navigate: string;
    openFile: string;
    home: void;
    drop: { paths: string[]; dest: string; copy: boolean };
  }>();

  /** Favorites section collapse state (transient, like the Home twisties). */
  let favOpen = true;
  const extOf = (name: string) => {
    const i = name.lastIndexOf(".");
    return i > 0 ? name.slice(i + 1).toLowerCase() : "";
  };

  /** The navigation-pane path currently hovered as a drop target, or "" for none. */
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

  /** Load a node's sub-folders once, tolerating unreadable dirs. */
  async function loadChildren(path: string): Promise<void> {
    if (children[path]) return;
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

  async function toggle(path: string) {
    if (expanded.has(path)) {
      expanded.delete(path);
      expanded = expanded;
      return;
    }
    expanded.add(path);
    expanded = expanded;
    await loadChildren(path);
  }

  const isAncestorOrSelf = (anc: string, p: string) => {
    const a = norm(anc), b = norm(p);
    return b === a || b.startsWith(a + "/");
  };

  // Two-way sync (CPE-236): reveal a path by expanding the tree from its root
  // place/drive down to it, loading each level lazily. Keeps the left tree in
  // step with where the middle pane is (current folder) and what's selected.
  let revealing = new Set<string>();
  async function revealPath(path: string): Promise<void> {
    if (!path || isHome || revealing.has(path)) return;
    revealing.add(path);
    try {
      const roots = [...places, ...drives];
      let cur = roots.find((r) => isAncestorOrSelf(r.path, path))?.path;
      let guard = 0;
      while (cur && norm(cur) !== norm(path) && guard++ < 64) {
        await loadChildren(cur);
        expanded.add(cur);
        expanded = expanded;
        const next = (children[cur] ?? []).find((c) => isAncestorOrSelf(c.path, path));
        if (!next) break;
        cur = next.path;
      }
    } finally {
      revealing.delete(path);
    }
  }

  // Reveal the current folder and any selected subfolder as they change.
  $: revealPath(currentPath);
  $: if (selectedPath) revealPath(selectedPath);

  /** A node is highlighted when it is the current folder or the selected one. */
  const isMarked = (p: string) =>
    !isHome && (p === currentPath || (selectedPath !== "" && p === selectedPath));
</script>

<div class="navigation-pane" role="region" aria-label="Navigation">
  {#if favorites.length > 0}
    <div class="nav-item fav-head">
      <button class="twisty" class:open={favOpen} title={favOpen ? "Collapse" : "Expand"} on:click={() => (favOpen = !favOpen)}>
        <Icon name="chev-right" size={12} />
      </button>
      <Icon name="star" />
      <span class="label fav-title">Favorites</span>
    </div>
    {#if favOpen}
      <div class="nav-children">
        {#each favorites as f (f.path)}
          <button
            class="nav-item fav-item"
            class:active={isMarked(f.path)}
            title={f.path}
            on:click={() => dispatch(f.is_dir ? "navigate" : "openFile", f.path)}
          >
            <span class="twisty hidden" />
            <Icon name={f.is_dir ? "folder" : iconFor({ is_dir: false, extension: extOf(f.name) })} />
            <span class="label">{f.name}</span>
          </button>
        {/each}
      </div>
    {/if}
    <div class="navigation-pane-sep" />
  {/if}
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

  <div class="navigation-pane-sep" />

  {#each [...places, ...drives] as place, i (place.path)}
    {@const open = expanded.has(place.path)}
    {@const isDrive = i >= places.length}
    {#if isDrive && i === places.length}
      <div class="navigation-pane-sep" />
    {/if}
    <div>
      <!-- svelte-ignore a11y-no-static-element-interactions -->
      <div
        class="nav-item"
        class:active={isMarked(place.path)}
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
              <SidebarNode
                node={child}
                {expanded}
                {children}
                {loadingPaths}
                {dropPath}
                marked={isMarked}
                onToggle={toggle}
                onNavigate={(p) => dispatch("navigate", p)}
                {onDragOver}
                onDragLeave={(p) => (dropPath = dropPath === p ? "" : dropPath)}
                {onDrop}
              />
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
  /* The Favorites section header reads as a heading, not a navigable row (CPE-340). */
  .fav-head { cursor: default; }
  .fav-title { font-weight: 600; }
</style>
