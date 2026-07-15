<script lang="ts">
  import { formatSize, formatDiskFree } from "../format";

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

  {#if diskLabel}
    <span class="dim disk" title="Free space on this drive">{diskLabel}</span>
  {/if}
</div>

<style>
  .dim { color: var(--text-faint); }
  /* Free space sits at the far right, away from the item/selection counts. */
  .disk { margin-left: auto; }
</style>
