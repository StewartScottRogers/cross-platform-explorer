<script lang="ts">
  import Icon from "./Icon.svelte";
  import { formatSize } from "../format";
  import { formatDate } from "../datetime";
  import { categoryOf, typeName } from "../filetypes";
  import type { DirEntry } from "../types";

  /** The current selection: 0, 1, or many. */
  export let selected: DirEntry[] = [];
  export let folderName = "";
  export let itemCount = 0;

  $: one = selected.length === 1 ? selected[0] : null;
  $: totalSize = selected.reduce((n, e) => n + (e.is_dir ? 0 : e.size), 0);
</script>

<aside class="details">
  {#if one}
    <div class="hero"><Icon name={categoryOf(one)} size={72} /></div>
    <h2>{one.name}</h2>
    <div class="meta">
      <div class="meta-row"><span class="meta-k">Type</span><span class="meta-v">{typeName(one)}</span></div>
      {#if !one.is_dir}
        <div class="meta-row">
          <span class="meta-k">Size</span><span class="meta-v">{formatSize(one.size) || "0 B"}</span>
        </div>
      {/if}
      <div class="meta-row">
        <span class="meta-k">Date modified</span>
        <span class="meta-v">{formatDate(one.modified) || "—"}</span>
      </div>
      <div class="meta-row"><span class="meta-k">Path</span><span class="meta-v">{one.path}</span></div>
    </div>
  {:else if selected.length > 1}
    <div class="hero"><Icon name="copy" size={72} /></div>
    <h2>{selected.length} items selected</h2>
    <div class="meta">
      <div class="meta-row">
        <span class="meta-k">Folders</span>
        <span class="meta-v">{selected.filter((e) => e.is_dir).length}</span>
      </div>
      <div class="meta-row">
        <span class="meta-k">Files</span>
        <span class="meta-v">{selected.filter((e) => !e.is_dir).length}</span>
      </div>
      <div class="meta-row">
        <span class="meta-k">Size of files</span>
        <span class="meta-v">{formatSize(totalSize) || "0 B"}</span>
      </div>
    </div>
    <div class="hint">
      <Icon name="info" size={15} />
      <span>Folder contents aren't included in the size. Press Alt+Enter for full properties.</span>
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
