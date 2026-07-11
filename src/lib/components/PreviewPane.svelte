<script lang="ts">
  import type { DirEntry } from "../types";
  import { pickProvider } from "../preview/provider";
  import { parseCsv } from "../preview/csv";

  /** The single selected entry to preview, or null. */
  export let entry: DirEntry | null = null;
  /** Resolve a file path to a URL the webview can load (convertFileSrc in the app). */
  export let assetUrl: (path: string) => string = (p) => p;
  /** Read a text file's contents (a size-capped backend command in the app). */
  export let loadText: (path: string) => Promise<string> = async () => "";

  /** Cap the number of CSV rows rendered so a huge sheet can't lock the pane. */
  const CSV_ROW_CAP = 200;

  $: provider = pickProvider(entry);
  $: needsText =
    provider.kind === "text" ||
    provider.kind === "markdown" ||
    provider.kind === "json" ||
    provider.kind === "csv";

  let text = "";
  let textState: "idle" | "loading" | "error" = "idle";
  let reqId = 0;

  // Load text whenever the selected entry (for a text-based provider) changes.
  // A monotonically increasing request id discards any load superseded by a
  // newer selection.
  $: if (entry && needsText) loadTextFor(entry);

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

  // Pretty-print JSON, falling back to the raw text if it does not parse.
  function prettyJson(raw: string): string {
    try {
      return JSON.stringify(JSON.parse(raw), null, 2);
    } catch {
      return raw;
    }
  }

  $: csvRows = provider.kind === "csv" && textState === "idle" ? parseCsv(text) : [];
</script>

<aside class="preview">
  {#if provider.kind === "image" && entry}
    <img class="preview-img" src={assetUrl(entry.path)} alt={entry.name} />
  {:else if provider.kind === "audio" && entry}
    <!-- svelte-ignore a11y-media-has-caption -->
    <audio class="preview-media" controls src={assetUrl(entry.path)}></audio>
  {:else if provider.kind === "video" && entry}
    <!-- svelte-ignore a11y-media-has-caption -->
    <video class="preview-media" controls src={assetUrl(entry.path)}></video>
  {:else if provider.kind === "pdf" && entry}
    <iframe class="preview-pdf" title={entry.name} src={assetUrl(entry.path)}></iframe>
  {:else if needsText && entry}
    {#if textState === "loading"}
      <p class="preview-note">Loading preview…</p>
    {:else if textState === "error"}
      <p class="preview-note">Can't preview this file.</p>
    {:else if provider.kind === "csv"}
      <div class="preview-table-wrap">
        <table class="preview-table">
          <tbody>
            {#each csvRows.slice(0, CSV_ROW_CAP) as r}
              <tr>{#each r as cell}<td>{cell}</td>{/each}</tr>
            {/each}
          </tbody>
        </table>
        {#if csvRows.length > CSV_ROW_CAP}
          <p class="preview-note">Showing first {CSV_ROW_CAP} of {csvRows.length} rows.</p>
        {/if}
      </div>
    {:else if provider.kind === "json"}
      <pre class="preview-text">{prettyJson(text)}</pre>
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
  .preview-media {
    width: 100%;
    margin: auto;
  }
  .preview-pdf {
    flex: 1;
    width: 100%;
    border: none;
  }
  .preview-text {
    margin: 0;
    padding: 12px;
    white-space: pre-wrap;
    word-break: break-word;
    font-family: var(--mono, ui-monospace, monospace);
    font-size: 12px;
  }
  .preview-table-wrap {
    overflow: auto;
    padding: 8px;
  }
  .preview-table {
    border-collapse: collapse;
    font-size: 12px;
  }
  .preview-table td {
    border: 1px solid var(--border);
    padding: 2px 6px;
    white-space: nowrap;
  }
  .preview-note {
    margin: auto;
    color: var(--text-faint);
    padding: 12px;
  }
</style>
