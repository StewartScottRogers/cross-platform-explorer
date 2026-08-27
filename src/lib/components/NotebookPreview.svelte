<script lang="ts">
  /**
   * Jupyter `.ipynb` notebook preview (CPE-1616, epic CPE-1568 slice 6): renders a notebook as an
   * ordered list of cells — markdown rendered via the same sanitized `marked` pipeline as the markdown
   * provider, code cells run through the shared `highlight.ts` grammars (`highlightCode`), and outputs
   * (stream text, text/plain + image/png results, error tracebacks) shown per cell. Self-contained like
   * CertPreview.svelte/FontPreview.svelte — fetches its own file content from `path` rather than routing
   * through PreviewPane's shared text-loading state.
   *
   * **Syntax highlighting is currently invisible** (CPE-1631, found by the Visual Critic reviewing this
   * ticket): `highlightCode` genuinely runs highlight.js and emits `hljs-*` classed markup, but no
   * stylesheet in the app defines any `.hljs-*` rule, so every code cell renders flat monochrome in both
   * themes. This is a pre-existing, app-wide gap — every surface that routes through `highlight.ts` is
   * affected, not something this ticket introduced or fixes — tracked separately as CPE-1631.
   *
   * A `.ipynb` is untrusted input: `parseNotebook` (preview/notebook.ts) never throws, and a parse
   * failure here degrades to the raw file text (same idea as the plain text/JSON view) with a clear
   * reason banner, rather than a blank pane. Rendered markdown reuses `renderMarkdown`, which sanitizes
   * with DOMPurify before returning — the same sanitization every other markdown surface in the app
   * relies on, so notebook markdown is no more of an injection vector than a plain `.md` file. Stream
   * text, `text/plain` results, and error tracebacks are run through `stripAnsi` (preview/notebook.ts)
   * before they ever reach this component, so raw ANSI colour-code garbage (`[0;31m`, …) that a real
   * executed notebook routinely embeds never reaches the `<pre>` — see that module's doc comment.
   */
  import { commands } from "../bindings.gen";
  import { unwrap } from "../invoke";
  import { renderMarkdown } from "../preview/markdown";
  import { ensureLanguage, highlightCode } from "../preview/highlight";
  import { parseNotebook, NOTEBOOK_READ_MAX_BYTES, type ParsedNotebook } from "../preview/notebook";

  /** The notebook file's path. */
  export let path: string;

  /** How much of the raw file text to show as the degrade-to-text fallback when parsing fails — capped
   *  independently of {@link NOTEBOOK_READ_MAX_BYTES} so a huge-but-invalid file can't stall the DOM
   *  either. */
  const RAW_FALLBACK_CHARS = 100_000;

  let loading = false;
  let loadError = "";
  let parseError = "";
  let notebook: ParsedNotebook | null = null;
  let cellHtml: Record<number, string> = {};
  let rawFallback = "";
  let rawFallbackTruncated = false;

  // Request-id guard (mirrors PreviewPane's mdReq/codeReq): renderCells awaits per-cell, so a fast
  // path-change mid-render must stop touching state for the superseded notebook.
  let reqId = 0;

  let loadedPath = "";
  $: if (path && path !== loadedPath) { loadedPath = path; void load(); }

  async function load() {
    const mine = ++reqId;
    loading = true;
    loadError = "";
    parseError = "";
    notebook = null;
    cellHtml = {};
    rawFallback = "";
    rawFallbackTruncated = false;

    let text: string;
    try {
      text = unwrap(await commands.readFileText(path, NOTEBOOK_READ_MAX_BYTES));
    } catch (e) {
      if (mine === reqId) {
        loadError = String(e);
        loading = false;
      }
      return;
    }
    if (mine !== reqId) return;

    const result = parseNotebook(text);
    if (!result.ok) {
      parseError = result.error;
      rawFallbackTruncated = text.length > RAW_FALLBACK_CHARS;
      rawFallback = rawFallbackTruncated ? text.slice(0, RAW_FALLBACK_CHARS) : text;
      loading = false;
      return;
    }

    notebook = result.notebook;
    await renderCells(result.notebook, mine);
    if (mine === reqId) loading = false;
  }

  async function renderCells(nb: ParsedNotebook, mine: number): Promise<void> {
    await ensureLanguage(nb.language);
    const html: Record<number, string> = {};
    for (const cell of nb.cells) {
      if (mine !== reqId) return; // superseded by a newer path — stop touching shared state
      if (cell.type === "markdown") html[cell.index] = await renderMarkdown(cell.source);
      else if (cell.type === "code") html[cell.index] = highlightCode(cell.source, nb.language);
    }
    if (mine === reqId) cellHtml = html;
  }
</script>

<div class="nb-preview" data-testid="notebook-preview">
  {#if loading}
    <p class="nb-note">Loading…</p>
  {:else if loadError}
    <p class="nb-error" data-testid="notebook-load-error">Can't preview this file: {loadError}</p>
  {:else if parseError}
    <div class="nb-banner warn" data-testid="notebook-parse-error">
      <span>This doesn't look like a valid Jupyter notebook: {parseError} Showing the raw file content instead.</span>
    </div>
    <pre class="nb-raw-fallback" data-testid="notebook-raw-fallback">{rawFallback}</pre>
    {#if rawFallbackTruncated}
      <p class="nb-note">Showing the first {RAW_FALLBACK_CHARS.toLocaleString()} characters.</p>
    {/if}
  {:else if notebook}
    <div class="nb-banner">
      <span>Notebook preview — read-only. Only text and PNG image outputs are rendered.</span>
    </div>
    {#if notebook.cellsCapped}
      <p class="nb-note" data-testid="notebook-cells-capped">
        Showing the first {notebook.cells.length} of {notebook.totalCells} cells.
      </p>
    {/if}
    {#if notebook.cells.length === 0}
      <p class="nb-note">This notebook has no cells.</p>
    {/if}
    <div class="nb-cells">
      {#each notebook.cells as cell (cell.index)}
        <div class="nb-cell" data-testid="notebook-cell" data-cell-type={cell.type}>
          <div class="nb-cell-head">
            <span class="nb-cell-badge">{cell.type}</span>
            {#if cell.type === "code"}
              <span class="nb-exec-count">{cell.executionCount != null ? `In [${cell.executionCount}]` : "In [ ]"}</span>
            {/if}
          </div>

          {#if cell.type === "markdown"}
            <!-- cellHtml is DOMPurify-sanitized via renderMarkdown, safe to inject. -->
            <div class="nb-markdown">{@html cellHtml[cell.index] ?? ""}</div>
          {:else if cell.type === "code"}
            <!-- cellHtml is hljs-escaped-or-highlighted output, safe to inject (same convention as
                 PreviewPane's own codeHtml). -->
            <pre class="nb-code"><code>{@html cellHtml[cell.index] ?? ""}</code></pre>
          {:else if cell.type === "raw"}
            <pre class="nb-raw">{cell.source}</pre>
          {:else}
            <p class="nb-note">Unrecognized cell type — nothing to render.</p>
          {/if}

          {#if cell.sourceTruncated}
            <p class="nb-note">Cell source truncated (too large to render in full).</p>
          {/if}

          {#if cell.outputs.length > 0}
            <div class="nb-outputs">
              {#each cell.outputs as output, i (i)}
                {#if output.kind === "stream"}
                  <pre class="nb-output nb-stream" class:stderr={output.name === "stderr"}>{output.text}</pre>
                {:else if output.kind === "error"}
                  <div class="nb-output nb-error-output" data-testid="notebook-error-output">
                    <div class="nb-error-head">{output.ename}: {output.evalue}</div>
                    {#if output.traceback}<pre class="nb-traceback">{output.traceback}</pre>{/if}
                  </div>
                {:else if output.kind === "result"}
                  {#if output.imageDataUrl}
                    <img class="nb-output-image" src={output.imageDataUrl} alt="Cell output" />
                  {/if}
                  {#if output.imageOmitted}
                    <p class="nb-note">Image output too large to display.</p>
                  {/if}
                  {#if output.text}<pre class="nb-output">{output.text}</pre>{/if}
                  {#if output.otherMimeTypes.length > 0}
                    <p class="nb-note">
                      {output.otherMimeTypes.length} other output type(s) not shown ({output.otherMimeTypes.join(", ")}) —
                      only text/plain and image/png are rendered.
                    </p>
                  {/if}
                {/if}
                {#if output.truncated}<p class="nb-note">Output truncated.</p>{/if}
              {/each}
            </div>
          {/if}
          {#if cell.outputsCapped}
            <p class="nb-note">Showing the first {cell.outputs.length} of {cell.outputsTotal} outputs.</p>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .nb-preview { padding: 12px; font-size: 12.5px; }
  .nb-note { color: var(--text-faint); font-size: 11.5px; margin: 4px 0; }
  .nb-error { color: var(--danger); white-space: pre-wrap; overflow-wrap: anywhere; }
  .nb-banner {
    display: flex; align-items: center; gap: 8px; padding: 7px 10px; border-radius: var(--radius);
    background: var(--surface-alt); border: 1px solid var(--border); color: var(--text-dim);
    margin-bottom: 12px; font-size: 11.5px;
  }
  .nb-banner.warn {
    color: var(--danger); border-color: var(--danger);
    background: color-mix(in srgb, var(--danger) 8%, var(--surface));
  }
  .nb-raw-fallback {
    background: var(--surface-alt); border: 1px solid var(--border); border-radius: var(--radius);
    padding: 8px; overflow-x: auto; white-space: pre-wrap; overflow-wrap: anywhere;
    font-family: var(--mono); font-size: 11.5px;
  }
  .nb-cells { display: flex; flex-direction: column; gap: 12px; }
  .nb-cell { border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); overflow: hidden; }
  .nb-cell-head {
    display: flex; align-items: center; gap: 8px; padding: 4px 10px;
    background: var(--surface-alt); border-bottom: 1px solid var(--border);
    font-size: 10.5px; color: var(--text-dim); text-transform: uppercase; letter-spacing: 0.03em;
  }
  .nb-cell-badge { font-weight: 600; }
  .nb-exec-count { margin-left: auto; text-transform: none; font-family: var(--mono); }
  .nb-markdown { padding: 10px 12px; line-height: 1.5; overflow-wrap: anywhere; color: var(--text); }
  .nb-markdown :global(h1),
  .nb-markdown :global(h2),
  .nb-markdown :global(h3) { margin: 0.6em 0 0.3em; }
  .nb-markdown :global(p) { margin: 0.5em 0; }
  .nb-markdown :global(pre) {
    background: var(--surface-alt); padding: 8px; border-radius: var(--radius); overflow-x: auto;
  }
  .nb-markdown :global(code) { font-family: var(--mono); }
  .nb-markdown :global(a) { color: var(--accent); }
  .nb-code, .nb-raw {
    margin: 0; padding: 10px 12px; overflow-x: auto; white-space: pre; overflow-wrap: normal;
    font-family: var(--mono); font-size: 12px; color: var(--text);
  }
  .nb-outputs { border-top: 1px dashed var(--border); padding: 6px 12px 8px; display: flex; flex-direction: column; gap: 6px; }
  /* CPE-1616 Visual Critic finding: an unbounded output (e.g. a 300-line stream, well within the
     20,000-char cap so never marked `truncated`) used to render every line inline, forcing the user to
     scroll past the whole dump in the notebook's own scroll container to reach the next cell. Bounded to
     a fixed height + its own scroll region so one noisy cell can't bury the rest of the notebook — short
     outputs are unaffected (they never reach this height, so no scrollbar appears). `resize: vertical`
     adds a visible drag handle (native browser affordance) so the bound reads as a deliberate, reachable
     "more below" rather than silent truncation; the content itself is never cut — every byte within the
     text cap is still in the DOM, just scrolled/resized into view. Applies to stream/result text AND the
     error/traceback block (`.nb-error-output` shares this class), so a giant traceback is bounded too. */
  .nb-output {
    margin: 0; padding: 6px 8px; background: var(--surface-alt); border-radius: var(--radius);
    white-space: pre-wrap; overflow-wrap: anywhere; overflow-x: auto;
    font-family: var(--mono); font-size: 11.5px; color: var(--text-dim);
    max-height: 260px; overflow-y: auto; resize: vertical;
  }
  .nb-stream.stderr { color: var(--danger); }
  .nb-output-image { max-width: 100%; border-radius: var(--radius); border: 1px solid var(--border); }
  /* CPE-1616 correction: this used to read `color: var(--danger)`. The traceback's real background is
     `color-mix(in srgb, var(--danger) 8%, var(--surface))` below (a slight red tint of --surface, not
     --surface itself), and in dark theme --danger (hex ff6659) only measured 4.41:1 against that specific
     blended background — under the 4.5:1 AA floor for normal text. An earlier revision "fixed" this by
     nudging the SHARED --pal-dark-red-400 (--danger) token itself, which silently regressed an unrelated,
     already-failing surface app-wide (white text on solid --danger buttons/pills/fills, tracked
     separately as CPE-1632) while softening the app-wide destructive red. Reverted that shared-token
     move and switched this rule to `--danger-on-tint` instead (src/app.css) — a token that resolves to
     --danger everywhere except dark theme, where it resolves to a slightly lighter red used ONLY here.
     Measured after the fix: 5.02:1 in dark theme against the real 8%-mixed background (up from 4.41:1,
     using the reverted/original --danger for the background itself); light theme unchanged at 4.99:1.
     See src/lib/components/NotebookPreview.test.ts's dedicated contrast guard, retargeted to this token. */
  .nb-error-output {
    padding: 6px 8px; border-radius: var(--radius); color: var(--danger-on-tint);
    background: color-mix(in srgb, var(--danger) 8%, var(--surface));
    border: 1px solid var(--danger);
  }
  .nb-error-head { font-weight: 600; margin-bottom: 4px; overflow-wrap: anywhere; }
  .nb-traceback {
    margin: 0; white-space: pre-wrap; overflow-wrap: anywhere; overflow-x: auto;
    font-family: var(--mono); font-size: 11.5px;
  }
</style>
