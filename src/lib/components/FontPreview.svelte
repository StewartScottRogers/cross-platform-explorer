<script lang="ts">
  /**
   * Font preview (CPE-1586, epic CPE-1568 slice 5): specimen + glyph-sample grid + lightweight metadata
   * for `.ttf`/`.otf`/`.woff`/`.woff2`. Self-contained like CertPreview.svelte/JwtPreview.svelte — it
   * loads its own data from `path` and reports its two copyable values (the selected glyph's character
   * and its codepoint) up to PreviewPane's generic action bar (CPE-1570) via `onValues`, keyed to match
   * the `font` provider's declared action ids in `preview/provider.ts`.
   *
   * The specimen loads the font itself via `FontFace`-from-URL over the `asset://` protocol (`assetUrl`)
   * — this used to live inline in PreviewPane.svelte (CPE-117); moving it here keeps that pane thin, like
   * every other structured preview. Metadata (format/family/style/version/glyph count) is read separately,
   * straight off the file's raw bytes fetched from that SAME asset URL (mirroring PreviewPane's own
   * `copyImageToClipboard`, which already fetches an asset:// URL for its bytes) and parsed by the pure
   * helpers in `preview/font.ts` — see that module's doc comment for why WOFF/WOFF2 degrade to format +
   * size only rather than full metadata (no new dependency for a real font parser).
   */
  import { onDestroy } from "svelte";
  import { t } from "../i18n";
  import { formatSize } from "../format";
  import {
    FONT_GLYPH_CANDIDATES,
    capGlyphs,
    glyphChar,
    codepointLabel,
    sniffFontFormat,
    parseSfntMetadata,
    formatLabelForExt,
    type FontFormat,
    type SfntMetadata,
  } from "../preview/font";

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

  // Capped once — the candidate list is fixed size (see preview/font.ts), so this never actually
  // truncates today, but the cap is exercised (and unit-tested) independently of that fixed list so a
  // future larger candidate set can't silently stall the pane with an unbounded grid.
  const { shown: glyphCells, total: glyphTotal, truncated: glyphTruncated } = capGlyphs(FONT_GLYPH_CANDIDATES);

  let selectedGlyph: number | null = glyphCells[0] ?? null;

  // Report the currently selected glyph's copyable values up to PreviewPane whenever it changes (CPE-1570)
  // — mirrors the jwt/json providers' own `values` convention. Nothing to copy until a glyph is selected.
  $: onValues(
    selectedGlyph !== null
      ? { "copy-glyph": glyphChar(selectedGlyph), "copy-codepoint": codepointLabel(selectedGlyph) }
      : {},
  );

  function selectGlyph(cp: number): void {
    selectedGlyph = cp;
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
    selectedGlyph = glyphCells[0] ?? null;
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

    // Best-effort metadata: a failed/unavailable fetch (or a compressed WOFF/WOFF2 container — see
    // preview/font.ts's parseSfntMetadata doc comment) just leaves `metadata`/`format` at their graceful
    // defaults, never an error shown to the user — the specimen/grid above already carries the preview.
    try {
      const resp = await fetch(url);
      const buf = new Uint8Array(await resp.arrayBuffer());
      if (mine !== fontReqId) return;
      format = sniffFontFormat(buf);
      metadata = parseSfntMetadata(buf);
    } catch {
      /* metadata unavailable — degrade gracefully, see doc comment above */
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
      {#each glyphCells as cp (cp)}
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
    {#if glyphTruncated}
      <p class="fp-note">Showing {glyphCells.length} of {glyphTotal} sample characters.</p>
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
