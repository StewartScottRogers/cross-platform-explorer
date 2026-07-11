<script lang="ts">
  import type { DirEntry } from "../types";
  import { pickProvider } from "../preview/provider";

  /** The single selected entry to preview, or null. */
  export let entry: DirEntry | null = null;
  /** Resolve a file path to a URL the webview can load (convertFileSrc in the app). */
  export let assetUrl: (path: string) => string = (p) => p;
  /** Read a text file's contents (a size-capped backend command in the app). */
  export let loadText: (path: string) => Promise<string> = async () => "";

  $: provider = pickProvider(entry);
  $: isText = provider.kind === "text" || provider.kind === "markdown";

  let text = "";
  let textState: "idle" | "loading" | "error" = "idle";
  let reqId = 0;

  // Load text whenever the selected entry (for a text/markdown provider) changes.
  // A monotonically increasing request id discards the result of any load that
  // has been superseded by a newer selection.
  $: if (entry && isText) loadTextFor(entry);

  async function loadTextFor(e: DirEntry) {
    const mine = ++reqId;
    textState = "loading";
    try {
      const t = await loadText(e.path);
      if (mine !== reqId) return;
      text = t;
      textState = "idle";
    } catch {
      if (mine !== reqId) return;
      textState = "error";
    }
  }
</script>

<aside class="preview">
  {#if provider.kind === "image" && entry}
    <img class="preview-img" src={assetUrl(entry.path)} alt={entry.name} />
  {:else if isText && entry}
    {#if textState === "loading"}
      <p class="preview-note">Loading preview…</p>
    {:else if textState === "error"}
      <p class="preview-note">Can't preview this file.</p>
    {:else}
      <pre class="preview-text">{text}</pre>
    {/if}
  {:else}
    <slot />
  {/if}
</aside>

<style>
  .preview {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: auto;
  }
  .preview-img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    margin: auto;
  }
  .preview-text {
    margin: 0;
    padding: 12px;
    white-space: pre-wrap;
    word-break: break-word;
    font-family: var(--mono, ui-monospace, monospace);
    font-size: 12px;
  }
  .preview-note {
    margin: auto;
    color: var(--text-faint);
  }
</style>
