<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { formatSize, formatDiskFree } from "../format";

  /** Begin an OS-window resize from the bottom-right corner when the grip is pressed (CPE-842).
      Guarded so it's a harmless no-op outside Tauri (e.g. the jsdom test harness). */
  async function startCornerResize(e: MouseEvent) {
    if (e.button !== 0) return; // left button only
    e.preventDefault();
    try {
      await getCurrentWindow().startResizeDragging("SouthEast");
    } catch {
      /* not running under Tauri, or the window can't be resized — ignore */
    }
  }

  /** Git sync status of the current folder (CPE-462), or null when it isn't a repo. Shape mirrors
      the host `forge_repo_status` command: { is_repo, branch, upstream, ahead, behind, dirty, ... }. */
  export let git: { is_repo?: boolean; branch?: string; upstream?: string; ahead?: number; behind?: number; dirty?: boolean; conflicted?: boolean } | null = null;
  const dispatch = createEventDispatcher<{ pull: void; push: void; sync: void; resolve: void }>();

  export let itemCount = 0;
  /** The folder's total item count before filtering; when it exceeds itemCount the status
      reads "X of Y items" so the filter's effect is visible (CPE-407). */
  export let totalCount = 0;
  export let selectedCount = 0;
  export let selectedSize = 0;

  $: isFiltered = totalCount > itemCount;
  export let hiddenShown = false;
  export let notice = "";
  export let noticeIsError = false;
  /** Free / total bytes on the current drive (CPE-403); null ⇒ unknown (Home/archive/error). */
  export let diskFree: number | null = null;
  export let diskTotal: number | null = null;

  $: diskLabel =
    diskFree !== null && diskTotal !== null ? formatDiskFree(diskFree, diskTotal) : "";
</script>

<div class="statusbar">
  <span>
    {#if isFiltered}
      {itemCount} of {totalCount} items
    {:else}
      {itemCount} item{itemCount === 1 ? "" : "s"}
    {/if}
  </span>

  {#if selectedCount > 0}
    <span>
      {selectedCount} selected{selectedSize > 0 ? ` — ${formatSize(selectedSize)}` : ""}
    </span>
  {/if}

  {#if hiddenShown}
    <span class="dim">Hidden files shown</span>
  {/if}

  {#if notice}
    <span class:error={noticeIsError}>{notice}</span>
  {/if}

  {#if git && git.is_repo}
    <span class="git" title={git.upstream ? `Tracking ${git.upstream}` : "No upstream branch"}>
      <span class="git-branch">⎇ {git.branch || "detached"}</span>
      {#if git.behind}<span class="git-ct" title="{git.behind} behind">↓{git.behind}</span>{/if}
      {#if git.ahead}<span class="git-ct" title="{git.ahead} ahead">↑{git.ahead}</span>{/if}
      {#if git.dirty}<span class="git-dirty" title="Uncommitted changes">●</span>{/if}
      {#if git.conflicted}
        <span class="git-conflict" title="Unmerged files from a merge/rebase">conflicts</span>
        <button class="git-btn resolve" on:click={() => dispatch("resolve")} title="Resolve merge/rebase conflicts in-app">Resolve…</button>
      {:else}
        {#if git.behind}<button class="git-btn" on:click={() => dispatch("pull")} title="Fast-forward pull from the remote">Pull</button>{/if}
        {#if git.ahead}<button class="git-btn" on:click={() => dispatch("push")} title="Push local commits to the remote">Push</button>{/if}
        <button class="git-btn" on:click={() => dispatch("sync")} title="Two-way sync: preview the plan, set the on-diverge policy, then run">Sync…</button>
      {/if}
    </span>
  {/if}

  {#if diskLabel}
    <span class="dim disk" title="Free space on this drive">{diskLabel}</span>
  {/if}

  <!-- Bottom-right sizing grip: drag to resize the window (CPE-842). A pure mouse-drag affordance
       (the OS window edges remain keyboard/other-input resizable), so the a11y interaction rule is
       intentionally suppressed. -->
  <!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
  <div
    class="resize-grip"
    role="separator"
    aria-label="Resize window"
    title="Drag to resize the window"
    on:mousedown={startCornerResize}
  ></div>
</div>

<style>
  .dim { color: var(--text-faint); }
  /* Git sync + free space sit at the far right, away from the item/selection counts. */
  .git { display: flex; align-items: center; gap: 6px; margin-left: auto; }
  .git-branch { opacity: 0.85; }
  .git-ct { font-variant-numeric: tabular-nums; opacity: 0.8; }
  .git-dirty { color: #b5872b; }
  .git-conflict { color: #b5872b; font-weight: 600; }
  .git-btn.resolve { border-color: #b5872b; }
  .git-btn { font-size: 11px; padding: 1px 7px; cursor: pointer; border: 1px solid var(--border-strong, #555);
             background: transparent; color: inherit; border-radius: 4px; }
  .git-btn:hover { background: var(--selection, rgba(128,128,128,0.2)); }
  .disk { margin-left: 12px; }

  /* Classic bottom-right sizing grip: three diagonal strokes in the corner, clipped to the
     lower-right triangle. Theme-variable coloured so it reads identically light/dark (CPE-842). */
  .resize-grip {
    position: absolute;
    right: 0;
    bottom: 0;
    width: 16px;
    height: 16px;
    cursor: nwse-resize;
    background-image: repeating-linear-gradient(
      -45deg,
      var(--text-faint) 0 1.5px,
      transparent 1.5px 4px
    );
    clip-path: polygon(100% 0, 100% 100%, 0 100%);
    opacity: 0.7;
  }
  .resize-grip:hover { opacity: 1; }
</style>
