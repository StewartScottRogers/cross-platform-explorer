<script lang="ts">
  export let itemCount = 0;
  export let selectedCount = 0;
  export let filtered = false;
  export let notice = "";
</script>

<div class="statusbar">
  <span>{itemCount} item{itemCount === 1 ? "" : "s"}{filtered ? " (filtered)" : ""}</span>
  {#if selectedCount > 0}
    <span>{selectedCount} item{selectedCount === 1 ? "" : "s"} selected</span>
  {/if}
  {#if notice}
    <span class="error">{notice}</span>
  {/if}
</div>
