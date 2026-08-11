<script lang="ts">
  /**
   * Font preview (CPE-1586/CPE-1593, epic CPE-1568 slice 5): specimen + glyph grid + lightweight metadata
   * for `.ttf`/`.otf`/`.woff`/`.woff2`. Self-contained like CertPreview.svelte/JwtPreview.svelte — it
   * loads its own data from `path` and reports its two copyable values (the selected glyph's character
   * and its codepoint) up to PreviewPane's generic action bar (CPE-1570) via `onValues`, keyed to match
   * the `font` provider's declared action ids in `preview/provider.ts`.
   *
   * The specimen loads the font itself via `FontFace`-from-URL over the `asset://` protocol (`assetUrl`)
   * — this used to live inline in PreviewPane.svelte (CPE-117); moving it here keeps that pane thin, like
   * every other structured preview. Metadata (format/family/style/version/glyph count) and the glyph
   * grid's coverage are read separately, via targeted `read_file_range` byte-range reads (CPE-1593) rather
   * than re-fetching the whole file a second time: one read for a small leading chunk (sfnt header + table
   * directory, always near the front), then — for whichever of `name`/`maxp`/`cmap` the directory says
   * lies OUTSIDE that chunk — one further targeted range read per table (in parallel), each bounded to
   * that table's own declared extent. Real Windows fonts commonly put `name` right near EOF (verified
   * against arial.ttf/malgun.ttf/seguisym.ttf, CPE-1593), so that extra read is the norm, not an edge case
   * — but it's still a handful of small bounded reads, nowhere near the whole file. Parsed by the pure
   * helpers in `preview/font.ts` — see that module's doc comments for why WOFF/WOFF2 degrade to format +
   * size only (no new dependency for a real font/decompression parser) and why the grid falls back to a
   * fixed Latin sample when the font's `cmap` can't be read.
   */
  import { onDestroy } from "svelte";
  import { t } from "../i18n";
  import { formatSize } from "../format";
  import { commands } from "../bindings.gen";
  import { unwrap } from "../invoke";
  import {
    glyphGridFromCoverage,
    glyphChar,
    codepointLabel,
    sniffFontFormat,
    parseMaxpNumGlyphs,
    parseNameTable,
    locateSfntTable,
    parseCmapCoverage,
    formatLabelForExt,
    FONT_METADATA_HEAD_BYTES,
    FONT_TABLE_RANGE_MAX_BYTES,
    type FontFormat,
    type SfntMetadata,
    type SfntTableEntry,
    type GlyphGrid,
  } from "../preview/font";

  /** Resolve one sfnt table's bytes given its directory entry: reuse the slice from the already-read head
   *  chunk if it's fully contained there, otherwise issue one targeted `read_file_range` for exactly that
   *  table's own extent (capped — see FONT_TABLE_RANGE_MAX_BYTES). `null` when there's no entry to resolve
   *  (table absent from this font). CPE-1593: this is the "second targeted range read" the ticket blesses
   *  as fine when a table doesn't fall inside the leading chunk. */
  async function resolveTableBytes(entry: SfntTableEntry | null, head: Uint8Array): Promise<Uint8Array | null> {
    if (!entry) return null;
    if (entry.offset >= 0 && entry.offset + entry.length <= head.length) {
      return head.subarray(entry.offset, entry.offset + entry.length);
    }
    const len = Math.min(entry.length, FONT_TABLE_RANGE_MAX_BYTES);
    const arr = unwrap(await commands.readFileRange(path, entry.offset, len));
    return new Uint8Array(arr);
  }

  /** The font file's path. */
  export let path: string;
  /** The font file's extension (lowercase, no dot) — the format-label fallback before the byte sniff
   *  resolves (or if it never does). */
  export let extension: string;
  /** File size in bytes, for the metadata row — the pane already has this on the `DirEntry`, no extra call. */
  export let size = 0;
  /** Resolve a file path to a URL the webview can load (`convertFileSrc` in the app). */
  export let assetUrl: (path: string) => string = (p) => p;
  /** Reports this preview's copyable values up to PreviewPane (CPE-1570, epic CPE-1568), keyed to match
   *  the `font` provider's declared action ids in `preview/provider.ts`. */
  export let onValues: (values: Record<string, string>) => void = () => {};

  const FONT_SAMPLE_DEFAULT = "The quick brown fox jumps over the lazy dog";
  const FONT_SIZES = [12, 18, 24, 36, 48];

  let sampleText = FONT_SAMPLE_DEFAULT;
  let fontFamily = "";
  let fontState: "idle" | "loading" | "error" = "idle";
  let format: FontFormat | null = null;
  let metadata: SfntMetadata | null = null;
  let fontReqId = 0;

  // Starts as the fixed fallback sample so the grid paints immediately on file change (STREAMING.md); load()
  // swaps this to the font's real `cmap` coverage once that's been read+parsed, or leaves it as the
  // fallback if the font's coverage couldn't be determined (CPE-1593).
  let glyphGrid: GlyphGrid = glyphGridFromCoverage(null);
  let selectedGlyph: number | null = glyphGrid.shown[0] ?? null;
  // Once the user has clicked a specific cell, an in-flight coverage upgrade must not steal their
  // selection out from under them by resetting it back to the new grid's first cell.
  let userPickedGlyph = false;

  // Report the currently selected glyph's copyable values up to PreviewPane whenever it changes (CPE-1570)
  // — mirrors the jwt/json providers' own `values` convention. Nothing to copy until a glyph is selected.
  $: onValues(
    selectedGlyph !== null
      ? { "copy-glyph": glyphChar(selectedGlyph), "copy-codepoint": codepointLabel(selectedGlyph) }
      : {},
  );

  function selectGlyph(cp: number): void {
    selectedGlyph = cp;
    userPickedGlyph = true;
  }

  // The FontFace this component last registered with the document — removed before adding a new one (or
  // on teardown) so previewing many fonts across a session doesn't grow the webview's global FontFaceSet
  // without bound (PURPOSE.md fast/small/predictable).
  let addedFace: FontFace | null = null;
  function releaseFace(): void {
    if (addedFace && typeof document !== "undefined" && "fonts" in document) {
      (document as Document & { fonts: FontFaceSet }).fonts.delete(addedFace);
    }
    addedFace = null;
  }
  onDestroy(releaseFace);

  let loadedPath = "";
  $: if (path && path !== loadedPath) {
    loadedPath = path;
    glyphGrid = glyphGridFromCoverage(null); // reset to the fallback sample; load() upgrades it below
    selectedGlyph = glyphGrid.shown[0] ?? null;
    userPickedGlyph = false;
    void load();
  }

  async function load(): Promise<void> {
    const mine = ++fontReqId;
    fontState = "loading";
    format = null;
    metadata = null;
    releaseFace();
    const url = assetUrl(path);

    // jsdom (tests) has no FontFace; degrade to a plain specimen in the inherited font.
    if (typeof FontFace !== "undefined") {
      const family = `preview-font-${mine}`;
      try {
        const face = new FontFace(family, `url("${url}")`);
        await face.load();
        if (mine !== fontReqId) return; // stale — selection moved on while this was in flight
        (document as Document & { fonts: FontFaceSet }).fonts.add(face);
        addedFace = face;
        fontFamily = family;
        fontState = "idle";
      } catch {
        if (mine !== fontReqId) return;
        fontFamily = "";
        fontState = "error";
      }
    } else {
      fontFamily = "";
      fontState = "idle";
    }

    // Best-effort metadata + coverage: a failed/unavailable read (or a compressed WOFF/WOFF2 container —
    // see preview/font.ts's doc comments) just leaves `metadata`/`format`/`glyphGrid` at their graceful
    // defaults, never an error shown to the user — the specimen above already carries the preview. Reads
    // only small targeted byte ranges, never the whole file (CPE-1593) — the FontFace load above already
    // fetched the full bytes once for actual rendering; sniffing metadata off a second full read would
    // double that I/O for no reason.
    try {
      const headArr = unwrap(await commands.readFileRange(path, 0, FONT_METADATA_HEAD_BYTES));
      if (mine !== fontReqId) return;
      const head = new Uint8Array(headArr);
      format = sniffFontFormat(head);

      // Locate all three tables of interest in one small head read, then resolve each one's bytes — reused
      // from the head chunk if it happens to fall inside, or fetched with its own targeted range read
      // (in parallel) otherwise. `name` in particular routinely needs the extra read on a real font (see
      // this component's doc comment) — `maxp` usually doesn't (it's small and typically placed early).
      const nameEntry = locateSfntTable(head, "name");
      const maxpEntry = locateSfntTable(head, "maxp");
      const cmapEntry = locateSfntTable(head, "cmap");
      const [nameBytes, maxpBytes, cmapBytes] = await Promise.all([
        resolveTableBytes(nameEntry, head),
        resolveTableBytes(maxpEntry, head),
        resolveTableBytes(cmapEntry, head),
      ]);
      if (mine !== fontReqId) return;

      if (format === "TrueType" || format === "OpenType") {
        const numGlyphs = maxpBytes ? parseMaxpNumGlyphs(maxpBytes) : null;
        const names = nameBytes ? parseNameTable(nameBytes) : {};
        metadata = { family: names[1] ?? null, style: names[2] ?? null, version: names[5] ?? null, numGlyphs };
      }

      const coverage = cmapBytes ? parseCmapCoverage(cmapBytes) : null;
      glyphGrid = glyphGridFromCoverage(coverage);
      if (!userPickedGlyph) selectedGlyph = glyphGrid.shown[0] ?? null;
    } catch {
      /* metadata/coverage unavailable — degrade gracefully, see doc comment above (fallback grid already showing) */
    }
  }
</script>

<div class="font-preview" data-testid="font-preview">
  {#if fontState === "error"}
    <p class="fp-error" data-testid="font-load-error">{$t("pv.cantFont")}</p>
  {/if}

  <div class="fp-section">
    <div class="fp-title">Specimen</div>
    <input
      class="fp-sample-input"
      type="text"
      bind:value={sampleText}
      spellcheck="false"
      aria-label="Specimen sample text"
    />
    <div class="fp-specimen">
      {#each FONT_SIZES as fsize}
        <p style="font-family: {fontFamily || 'inherit'}; font-size: {fsize}px">{sampleText}</p>
      {/each}
    </div>
  </div>

  <div class="fp-section">
    <div class="fp-title">Metadata</div>
    <dl class="fp-rows">
      <div><dt>Format</dt><dd>{format ?? formatLabelForExt(extension)}</dd></div>
      {#if metadata?.family}<div><dt>Family</dt><dd>{metadata.family}</dd></div>{/if}
      {#if metadata?.style}<div><dt>Style</dt><dd>{metadata.style}</dd></div>{/if}
      {#if metadata?.version}<div><dt>Version</dt><dd class="wrap">{metadata.version}</dd></div>{/if}
      {#if metadata?.numGlyphs != null}
        <div><dt>Glyphs</dt><dd>{metadata.numGlyphs.toLocaleString()}</dd></div>
      {/if}
      <div><dt>File size</dt><dd>{formatSize(size)}</dd></div>
    </dl>
    {#if !metadata && (format === "WOFF" || format === "WOFF2")}
      <p class="fp-note">Family/style/version aren't read for compressed web-font containers (WOFF/WOFF2).</p>
    {/if}
  </div>

  <div class="fp-section">
    <div class="fp-title">
      Glyphs
      {#if selectedGlyph !== null}
        — “{glyphChar(selectedGlyph)}” ({codepointLabel(selectedGlyph)})
      {/if}
    </div>
    <div class="fp-glyph-grid" data-testid="font-glyph-grid">
      {#each glyphGrid.shown as cp (cp)}
        <button
          type="button"
          class="fp-glyph"
          class:selected={selectedGlyph === cp}
          style="font-family: {fontFamily || 'inherit'}"
          title={codepointLabel(cp)}
          aria-label={`Glyph ${codepointLabel(cp)}`}
          aria-pressed={selectedGlyph === cp}
          on:click={() => selectGlyph(cp)}
        >{glyphChar(cp)}</button>
      {/each}
    </div>
    {#if glyphGrid.source === "coverage"}
      {#if glyphGrid.truncated}
        <p class="fp-note" data-testid="font-glyph-note">
          Showing {glyphGrid.shown.length} of {glyphGrid.total.toLocaleString()} characters this font
          actually defines, evenly sampled across its coverage.
        </p>
      {:else}
        <p class="fp-note" data-testid="font-glyph-note">
          This font defines {glyphGrid.total.toLocaleString()}
          {glyphGrid.total === 1 ? "character" : "characters"}, shown here.
        </p>
      {/if}
    {:else}
      <p class="fp-note" data-testid="font-glyph-note">
        This font's own character coverage couldn't be read — showing a fixed sample of
        {glyphGrid.total} common Latin characters instead.
      </p>
    {/if}
  </div>
</div>

<style>
  .font-preview { padding: 12px; font-size: 12px; }
  .fp-error { color: var(--danger); white-space: pre-wrap; overflow-wrap: anywhere; margin: 0 0 12px; }
  .fp-section { margin-bottom: 16px; }
  .fp-section:last-child { margin-bottom: 0; }
  .fp-title {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-dim);
    text-transform: uppercase;
    letter-spacing: 0.03em;
    margin-bottom: 8px;
  }
  .fp-sample-input {
    width: 100%;
    box-sizing: border-box;
    height: 28px;
    padding: 0 8px;
    margin-bottom: 8px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text);
    font-size: 12px;
  }
  .fp-specimen {
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface-alt);
    overflow: hidden;
  }
  .fp-specimen p { margin: 0 0 12px; line-height: 1.3; color: var(--text); overflow-wrap: anywhere; }
  .fp-specimen p:last-child { margin-bottom: 0; }
  .fp-rows { display: grid; gap: 6px; margin: 0; }
  .fp-rows > div { display: flex; gap: 10px; align-items: baseline; }
  .fp-rows dt { color: var(--text-dim); width: 90px; flex: none; margin: 0; }
  .fp-rows dd { flex: 1; overflow-wrap: anywhere; margin: 0; }
  .wrap { overflow-wrap: anywhere; }
  .fp-note { margin: 8px 0 0; color: var(--text-faint); font-size: 11px; }
  /* Glyph grid: a wrapping tile grid, capped in JS (see preview/font.ts) so it can never grow unbounded. */
  .fp-glyph-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(32px, 1fr));
    gap: 4px;
  }
  .fp-glyph {
    aspect-ratio: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text);
    font-size: 15px;
    overflow: hidden;
  }
  .fp-glyph:hover { border-color: var(--border-strong); background: var(--surface-alt); }
  .fp-glyph.selected { border-color: var(--accent); background: color-mix(in srgb, var(--accent) 14%, var(--surface)); }
</style>
