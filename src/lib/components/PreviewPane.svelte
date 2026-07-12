<script lang="ts">
  import { tick } from "svelte";
  import type { DirEntry } from "../types";
  import { pickProvider, type ArchiveEntry } from "../preview/provider";
  import { parseCsv } from "../preview/csv";
  import { highlightForFile, ensureLanguageForName } from "../preview/highlight";
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

  // Async-rendered HTML for code (lazy grammar) and markdown (lazy renderer).
  let codeHtml = "";
  let mdHtml = "";
  let codeReq = 0;
  let mdReq = 0;

  $: if (entry && textState === "idle" && provider.kind === "text") {
    renderCode(entry.name, text);
  }
  $: if (entry && textState === "idle" && provider.kind === "markdown") {
    renderMd(text);
  }

  async function renderCode(name: string, src: string) {
    const mine = ++codeReq;
    codeHtml = highlightForFile(src, name); // escaped immediately
    const ok = await ensureLanguageForName(name);
    if (ok && mine === codeReq) codeHtml = highlightForFile(src, name); // now highlighted
  }

  async function renderMd(src: string) {
    const mine = ++mdReq;
    const html = await renderMarkdown(src);
    if (mine === mdReq) mdHtml = html;
  }

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

  // ---- text context menu (Cut / Copy / Paste / Select All) ----
  let editorEl: HTMLTextAreaElement | undefined;
  let textContentEl: HTMLElement | undefined;
  let ctxMenu: { x: number; y: number } | null = null;

  // Cut/Paste only apply while editing (view is read-only). The menu items are
  // always present; these flags disable them.
  $: canModify = editing;

  $: isTextKind =
    provider.kind === "text" ||
    provider.kind === "markdown" ||
    provider.kind === "json" ||
    provider.kind === "csv" ||
    provider.kind === "tsv";

  function openTextMenu(e: MouseEvent) {
    if (!isTextKind || textState !== "idle") return; // let non-text use the native menu
    e.preventDefault();
    ctxMenu = { x: e.clientX, y: e.clientY };
  }
  function closeTextMenu() {
    ctxMenu = null;
  }

  /** Currently selected text — from the editor when editing, else the window selection. */
  function selectedText(): string {
    if (editing && editorEl) {
      return draft.slice(editorEl.selectionStart, editorEl.selectionEnd);
    }
    return window.getSelection()?.toString() ?? "";
  }

  async function menuCopy() {
    const sel = selectedText();
    const all = editing ? draft : text;
    try {
      await navigator.clipboard.writeText(sel || all);
    } catch {
      /* clipboard unavailable — ignore */
    }
    closeTextMenu();
  }

  async function menuCut() {
    if (!editing || !editorEl) return closeTextMenu();
    const start = editorEl.selectionStart;
    const end = editorEl.selectionEnd;
    const cut = draft.slice(start, end);
    if (cut) {
      try {
        await navigator.clipboard.writeText(cut);
      } catch {
        /* ignore */
      }
      draft = draft.slice(0, start) + draft.slice(end);
      await tick();
      editorEl.focus();
      editorEl.setSelectionRange(start, start);
    }
    closeTextMenu();
  }

  async function menuPaste() {
    if (!editing || !editorEl) return closeTextMenu();
    let clip = "";
    try {
      clip = await navigator.clipboard.readText();
    } catch {
      return closeTextMenu();
    }
    const start = editorEl.selectionStart;
    const end = editorEl.selectionEnd;
    draft = draft.slice(0, start) + clip + draft.slice(end);
    await tick();
    editorEl.focus();
    const caret = start + clip.length;
    editorEl.setSelectionRange(caret, caret);
    closeTextMenu();
  }

  function menuSelectAll() {
    if (editing && editorEl) {
      editorEl.focus();
      editorEl.select();
    } else if (textContentEl) {
      const range = document.createRange();
      range.selectNodeContents(textContentEl);
      const sel = window.getSelection();
      sel?.removeAllRanges();
      sel?.addRange(range);
    }
    closeTextMenu();
  }
</script>

<svelte:window on:click={closeTextMenu} on:keydown={(e) => e.key === "Escape" && closeTextMenu()} />

<!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
<aside class="preview" on:contextmenu={openTextMenu}>
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
        bind:this={editorEl}
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
        <div class="preview-table-wrap" bind:this={textContentEl}>
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
        <pre class="preview-text" bind:this={textContentEl}>{prettyJson(text)}</pre>
      {:else if provider.kind === "markdown"}
        <!-- mdHtml is DOMPurify-sanitized (lazy renderer), safe to inject. -->
        <div class="preview-markdown" bind:this={textContentEl}>{@html mdHtml}</div>
      {:else}
        <!-- codeHtml is escaped or hljs output (lazy grammar), safe to inject. -->
        <pre class="preview-text" bind:this={textContentEl}><code>{@html codeHtml}</code></pre>
      {/if}
    {/if}
  {:else}
    <slot />
  {/if}
</aside>

{#if ctxMenu}
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-noninteractive-element-interactions -->
  <div
    class="text-ctx"
    role="menu"
    tabindex="-1"
    style="left:{ctxMenu.x}px; top:{ctxMenu.y}px"
    on:click|stopPropagation
    on:contextmenu|preventDefault|stopPropagation
  >
    <button role="menuitem" disabled={!canModify} on:click={menuCut}>Cut</button>
    <button role="menuitem" on:click={menuCopy}>Copy</button>
    <button role="menuitem" disabled={!canModify} on:click={menuPaste}>Paste</button>
    <div class="text-ctx-sep"></div>
    <button role="menuitem" on:click={menuSelectAll}>Select all</button>
  </div>
{/if}

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
  .text-ctx {
    position: fixed;
    z-index: 100;
    min-width: 160px;
    padding: 4px;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-lg);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.16);
  }
  .text-ctx button {
    display: flex;
    align-items: center;
    width: 100%;
    height: 30px;
    padding: 0 12px;
    text-align: left;
    border-radius: var(--radius);
    font-size: 13px;
  }
  .text-ctx button:disabled { opacity: 0.4; cursor: default; }
  .text-ctx-sep { height: 1px; background: var(--border); margin: 4px 6px; }
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
