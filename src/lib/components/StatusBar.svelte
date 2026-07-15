<script lang="ts">
  import { formatSize, formatDiskFree } from "../format";

  export let itemCount = 0;
  export let selectedCount = 0;
  export let selectedSize = 0;
  export let filtered = false;
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
  <span>{itemCount} item{itemCount === 1 ? "" : "s"}{filtered ? " (filtered)" : ""}</span>

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

  {#if diskLabel}
    <span class="dim disk" title="Free space on this drive">{diskLabel}</span>
  {/if}
</div>

<style>
  .dim { color: var(--text-faint); }
  /* Free space sits at the far right, away from the item/selection counts. */
  .disk { margin-left: auto; }
</style>
