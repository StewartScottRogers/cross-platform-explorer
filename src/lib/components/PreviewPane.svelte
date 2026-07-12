<script lang="ts">
  import type { DirEntry } from "../types";
  import { pickProvider, type ArchiveEntry } from "../preview/provider";
  import { parseCsv } from "../preview/csv";
  import { highlightForFile } from "../preview/highlight";
  import { renderMarkdown } from "../preview/markdown";
  import { formatSize } from "../format";

  /** The single selected entry to preview, or null. */
  export let entry: DirEntry | null = null;
  /** Resolve a file path to a URL the webview can load (convertFileSrc in the app). */
  export let assetUrl: (path: string) => string = (p) => p;
  /** Read a text file's contents (a size-capped backend command in the app). */
  export let loadText: (path: string) => Promise<string> = async () => "";
  /** List an archive's entries (a backend command in the app). */
  export let loadEntries: (path: string) => Promise<ArchiveEntry[]> = async () => [];
  /** Save edited text back to a file (a backend command in the app). */
  export let saveText: (path: string, contents: string) => Promise<void> = async () => {};

  /** Cap the number of CSV rows rendered so a huge sheet can't lock the pane. */
  const CSV_ROW_CAP = 200;

  $: provider = pickProvider(entry);
  $: needsText =
    provider.kind === "text" ||
    provider.kind === "markdown" ||
    provider.kind === "json" ||
    provider.kind === "csv" ||
    provider.kind === "tsv";

  let text = "";
  let textState: "idle" | "loading" | "error" = "idle";
  let reqId = 0;

  let entries: ArchiveEntry[] = [];
  let entriesState: "idle" | "loading" | "error" = "idle";
  let entryReqId = 0;

  // Load text whenever the selected entry (for a text-based provider) changes.
  // A monotonically increasing request id discards any load superseded by a
  // newer selection.
  $: if (entry && needsText) loadTextFor(entry);
  $: if (entry && provider.kind === "archive") loadEntriesFor(entry);

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

  async function loadEntriesFor(e: DirEntry) {
    const mine = ++entryReqId;
    entriesState = "loading";
    try {
      const list = await loadEntries(e.path);
      if (mine !== entryReqId) return;
      entries = list;
      entriesState = "idle";
    } catch {
      if (mine !== entryReqId) return;
      entriesState = "error";
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

  $: tableRows =
    textState === "idle" && (provider.kind === "csv" || provider.kind === "tsv")
      ? parseCsv(text, provider.kind === "tsv" ? "\t" : ",")
      : [];

  // ---- editing ----
  let editing = false;
  let draft = "";
  let saving = false;
  let saveError = "";
  let lastPath = "";

  $: dirty = draft !== text;

  // Leave edit mode (without saving) whenever the selected file changes.
  $: if (entry && entry.path !== lastPath) {
    lastPath = entry.path;
    editing = false;
    saveError = "";
  }

  function startEdit() {
    draft = text;
    saveError = "";
    editing = true;
  }

  function cancelEdit() {
    editing = false;
    saveError = "";
  }

  async function save() {
    if (!entry || !dirty || saving) return;
    saving = true;
    saveError = "";
    try {
      await saveText(entry.path, draft);
      text = draft;
      editing = false;
    } catch {
      saveError = "Couldn't save the file.";
    } finally {
      saving = false;
    }
  }

  function onEditorKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
      e.preventDefault();
      save();
    }
  }
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
  {:else if provider.kind === "archive" && entry}
    {#if entriesState === "loading"}
      <p class="preview-note">Loading preview…</p>
    {:else if entriesState === "error"}
      <p class="preview-note">Can't read this archive.</p>
    {:else}
      <div class="preview-table-wrap">
        <table class="preview-table">
          <tbody>
            {#each entries as e}
              <tr>
                <td>{e.name}</td>
                <td class="num">{e.is_dir ? "" : formatSize(e.size)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
        <p class="preview-note">{entries.length} item{entries.length === 1 ? "" : "s"}</p>
      </div>
    {/if}
  {:else if needsText && entry}
    {#if textState === "loading"}
      <p class="preview-note">Loading preview…</p>
    {:else if textState === "error"}
      <p class="preview-note">Can't preview this file.</p>
    {:else if editing}
      <div class="preview-edit-bar">
        <button class="editbtn primary" disabled={!dirty || saving} on:click={save}
          >{saving ? "Saving…" : "Save"}</button>
        <button class="editbtn" on:click={cancelEdit}>Cancel</button>
        {#if saveError}<span class="edit-err">{saveError}</span>{/if}
      </div>
      <textarea
        class="preview-editor"
        bind:value={draft}
        on:keydown={onEditorKeydown}
        spellcheck="false"
      ></textarea>
    {:else}
      {#if provider.editable}
        <div class="preview-edit-bar">
          <button class="editbtn" on:click={startEdit}>Edit</button>
        </div>
      {/if}
      {#if provider.kind === "csv" || provider.kind === "tsv"}
        <div class="preview-table-wrap">
          <table class="preview-table">
            <tbody>
              {#each tableRows.slice(0, CSV_ROW_CAP) as r}
                <tr>{#each r as cell}<td>{cell}</td>{/each}</tr>
              {/each}
            </tbody>
          </table>
          {#if tableRows.length > CSV_ROW_CAP}
            <p class="preview-note">Showing first {CSV_ROW_CAP} of {tableRows.length} rows.</p>
          {/if}
        </div>
      {:else if provider.kind === "json"}
        <pre class="preview-text">{prettyJson(text)}</pre>
      {:else if provider.kind === "markdown"}
        <!-- Sanitized by DOMPurify in renderMarkdown before injection. -->
        <div class="preview-markdown">{@html renderMarkdown(text)}</div>
      {:else}
        <!-- highlightCode escapes the source, so the HTML is safe to inject. -->
        <pre class="preview-text"><code>{@html highlightForFile(text, entry.name)}</code></pre>
      {/if}
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
  .preview-table td.num { text-align: right; color: var(--text-dim); }
  .preview-note {
    margin: auto;
    color: var(--text-faint);
    padding: 12px;
  }
  .preview-edit-bar {
    display: flex;
    gap: 6px;
    align-items: center;
    padding: 6px 8px;
    border-bottom: 1px solid var(--border);
    flex: none;
  }
  .editbtn {
    padding: 4px 10px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    font-size: 12px;
  }
  .editbtn.primary { background: var(--accent); color: #fff; border-color: var(--accent); }
  .editbtn:disabled { opacity: 0.5; }
  .edit-err { color: #c42b1c; font-size: 12px; }
  .preview-editor {
    flex: 1;
    width: 100%;
    resize: none;
    border: none;
    outline: none;
    padding: 12px;
    font-family: var(--mono, ui-monospace, monospace);
    font-size: 12px;
    line-height: 1.5;
    color: var(--text);
    background: var(--surface);
    tab-size: 2;
  }
  .preview-markdown {
    padding: 12px 16px;
    font-size: 13px;
    line-height: 1.5;
    overflow-wrap: anywhere;
  }
  .preview-markdown :global(h1),
  .preview-markdown :global(h2),
  .preview-markdown :global(h3) { margin: 0.6em 0 0.3em; }
  .preview-markdown :global(p) { margin: 0.5em 0; }
  .preview-markdown :global(pre) {
    background: var(--surface-alt);
    padding: 8px;
    border-radius: var(--radius);
    overflow-x: auto;
  }
  .preview-markdown :global(code) { font-family: var(--mono, ui-monospace, monospace); }
  .preview-markdown :global(a) { color: var(--accent); }
</style>
