<script lang="ts">
  import Icon from "./Icon.svelte";
  import { formatSize } from "../format";
  import { formatDate } from "../datetime";
  import { categoryOf, typeName } from "../filetypes";
  import type { DirEntry } from "../types";

  export let entry: DirEntry | null = null;
  export let folderName = "";
  export let itemCount = 0;
</script>

<aside class="details">
  {#if entry}
    <div class="hero"><Icon name={categoryOf(entry)} size={72} /></div>
    <h2>{entry.name}</h2>
    <div class="meta">
      <div class="meta-row">
        <span class="meta-k">Type</span><span class="meta-v">{typeName(entry)}</span>
      </div>
      {#if !entry.is_dir}
        <div class="meta-row">
          <span class="meta-k">Size</span>
          <span class="meta-v">{formatSize(entry.size) || "0 B"}</span>
        </div>
      {/if}
      <div class="meta-row">
        <span class="meta-k">Date modified</span>
        <span class="meta-v">{formatDate(entry.modified) || "—"}</span>
      </div>
      <div class="meta-row">
        <span class="meta-k">Path</span><span class="meta-v">{entry.path}</span>
      </div>
    </div>
  {:else}
    <div class="hero"><Icon name="home" size={72} /></div>
    <h2>{folderName} ({itemCount} item{itemCount === 1 ? "" : "s"})</h2>
    <div class="hint">
      <Icon name="info" size={15} />
      <span>Select a single file to get more information.</span>
    </div>
  {/if}
</aside>
