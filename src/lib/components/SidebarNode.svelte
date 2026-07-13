<script lang="ts">
  /**
   * A recursive navigation-tree node (CPE-236). Renders a folder with a twisty
   * that expands to its sub-folders, which are themselves SidebarNodes — so the
   * tree goes arbitrarily deep and can reveal/highlight the current folder no
   * matter how nested. Shared state (expanded/children/loading) and the
   * handlers live in Sidebar and are threaded down as props/callbacks.
   */
  import Icon from "./Icon.svelte";
  import type { DirEntry } from "../types";

  export let node: { path: string; name: string };
  export let expanded: Set<string>;
  export let children: Record<string, DirEntry[]>;
  export let loadingPaths: Set<string>;
  export let dropPath = "";
  export let marked: (p: string) => boolean;
  export let onToggle: (p: string) => void;
  export let onNavigate: (p: string) => void;
  export let onDragOver: (e: DragEvent, p: string) => void;
  export let onDragLeave: (p: string) => void;
  export let onDrop: (e: DragEvent, p: string) => void;

  $: open = expanded.has(node.path);
  $: kids = children[node.path] ?? [];
</script>

<!-- svelte-ignore a11y-no-static-element-interactions -->
<div
  class="nav-item"
  class:active={marked(node.path)}
  class:droptarget={dropPath === node.path}
  role="treeitem"
  tabindex="-1"
  aria-selected={marked(node.path)}
  aria-expanded={open}
  on:dragover={(e) => onDragOver(e, node.path)}
  on:dragleave={() => onDragLeave(node.path)}
  on:drop={(e) => onDrop(e, node.path)}
>
  <button class="twisty" class:open title={open ? "Collapse" : "Expand"} on:click={() => onToggle(node.path)}>
    <Icon name="chev-right" size={12} />
  </button>
  <Icon name="folder" />
  <button class="label" style="text-align:left" on:click={() => onNavigate(node.path)}>{node.name}</button>
</div>

{#if open}
  <div class="nav-children">
    {#if loadingPaths.has(node.path)}
      <div class="nav-empty">Loading…</div>
    {:else if kids.length === 0}
      <div class="nav-empty">No folders</div>
    {:else}
      {#each kids as child (child.path)}
        <svelte:self
          node={child}
          {expanded}
          {children}
          {loadingPaths}
          {dropPath}
          {marked}
          {onToggle}
          {onNavigate}
          {onDragOver}
          {onDragLeave}
          {onDrop}
        />
      {/each}
    {/if}
  </div>
{/if}
