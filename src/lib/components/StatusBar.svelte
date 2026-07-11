<script lang="ts">
  import { formatSize } from "../format";

  export let itemCount = 0;
  export let selectedCount = 0;
  export let selectedSize = 0;
  export let filtered = false;
  export let hiddenShown = false;
  export let notice = "";
  export let noticeIsError = false;
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
</div>

<style>
  .dim { color: var(--text-faint); }
</style>
