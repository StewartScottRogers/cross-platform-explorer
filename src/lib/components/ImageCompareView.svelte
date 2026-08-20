<script lang="ts">
  /**
   * Image compare pane (CPE-1508, epic CPE-722, parent CPE-1490). Renders the already-fetched
   * `ImageDiff` (from `diffImages`/`diff_images`, CPE-1490) as three toggled sub-views:
   *  - side-by-side: the two source images adjacent, sharing one zoom/pan (drag to pan, wheel to zoom).
   *  - onion-skin: the two source images stacked, blended by an opacity slider over the right image.
   *  - heatmap: `maskPng` rendered standalone, plus the diff stats and a "zoom to changed region" jump
   *    using `bbox` when present.
   *
   * The two SOURCE images (left/right) are loaded via `assetUrl` (`convertFileSrc` in the app) straight
   * from disk — no backend round trip needed for formats the webview already renders natively, mirroring
   * how `PreviewPane`'s default "decoded-image" path works. Only `maskPng` (which has no file on disk —
   * it's a value returned over IPC) goes through `maskPngToDataUrl`.
   *
   * Pointer events (not HTML5 drag-and-drop) drive panning, per CPE-1525's WebView2 finding that HTML5 DnD
   * is unreliable there.
   */
  import { maskPngToDataUrl, bboxRectPercent, zoomToBBox, clampZoom, formatPercentDifferent } from "../imageDiffView";
  import type { ImageDiff } from "../bindings.gen";

  export let left = "";
  export let right = "";
  export let diff: ImageDiff;
  /** Resolve a file path to a URL the webview can load (`convertFileSrc` in the app). */
  export let assetUrl: (path: string) => string = (p) => p;

  type SubView = "side-by-side" | "onion-skin" | "heatmap";
  let subView: SubView = "side-by-side";

  // Shared zoom/pan (side-by-side syncs both panes; onion-skin applies the same transform to its stack).
  let zoom = 1;
  let panX = 0;
  let panY = 0;
  let viewportEl: HTMLDivElement | undefined;
  let dragging = false;
  let dragStartX = 0;
  let dragStartY = 0;
  let panStartX = 0;
  let panStartY = 0;

  let onionOpacity = 50;

  $: leftSrc = assetUrl(left);
  $: rightSrc = assetUrl(right);
  $: maskSrc = maskPngToDataUrl(diff.maskPng);
  $: bboxRect = bboxRectPercent(diff.bbox ?? null, diff.width, diff.height);
  $: transformStyle = `transform: scale(${zoom}) translate(${panX}px, ${panY}px); transform-origin: 0 0;`;

  function resetView() {
    zoom = 1;
    panX = 0;
    panY = 0;
  }

  function zoomBy(factor: number, cx?: number, cy?: number) {
    const next = clampZoom(zoom * factor);
    if (next === zoom) return;
    // Keep the point under the cursor (or viewport center, if unspecified) fixed while zooming.
    const vw = viewportEl?.clientWidth ?? 0;
    const vh = viewportEl?.clientHeight ?? 0;
    const px = cx ?? vw / 2;
    const py = cy ?? vh / 2;
    // Image-space point currently under (px, py): px/zoom - panX. Solve for the new pan that keeps it fixed.
    const imgX = px / zoom - panX;
    const imgY = py / zoom - panY;
    panX = px / next - imgX;
    panY = py / next - imgY;
    zoom = next;
  }

  function onWheel(e: WheelEvent) {
    e.preventDefault();
    // Anchor on the pane actually under the cursor — NOT the shared `viewportEl` ref, which in
    // side-by-side is only ever bound to the LEFT pane. Both panes share this same handler, so
    // anchoring off `viewportEl` made wheel-zooming over the right pane compute the cursor position
    // against the left pane's rect (offset by ~one pane width). `currentTarget` is always the exact
    // element the listener is attached to, so this works correctly for every pane/view that wires
    // `on:wheel={onWheel}` directly (side-by-side's two panes, onion-skin's stack, the heatmap wrap).
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const cx = e.clientX - rect.left;
    const cy = e.clientY - rect.top;
    zoomBy(e.deltaY < 0 ? 1.2 : 1 / 1.2, cx, cy);
  }

  function onPointerDown(e: PointerEvent) {
    dragging = true;
    dragStartX = e.clientX;
    dragStartY = e.clientY;
    panStartX = panX;
    panStartY = panY;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function onPointerMove(e: PointerEvent) {
    if (!dragging) return;
    panX = panStartX + (e.clientX - dragStartX) / zoom;
    panY = panStartY + (e.clientY - dragStartY) / zoom;
  }

  function onPointerUp(e: PointerEvent) {
    dragging = false;
    (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
  }

  function zoomToChangedRegion() {
    if (!diff.bbox || !viewportEl) return;
    const { zoom: z, panX: px, panY: py } = zoomToBBox(
      diff.bbox,
      diff.width,
      diff.height,
      viewportEl.clientWidth,
      viewportEl.clientHeight,
    );
    zoom = z;
    panX = px;
    panY = py;
  }
</script>

<div class="ic" data-testid="image-compare">
  {#if diff.sizeMismatch}
    <div class="ic-note" data-testid="ic-size-mismatch">
      Images differ in size — showing the union canvas; the padded region counts as changed.
    </div>
  {/if}

  <div class="ic-toolbar">
    <div class="ic-tabs" role="tablist" aria-label="Image compare view">
      <button
        class="ic-tab"
        class:active={subView === "side-by-side"}
        role="tab"
        aria-selected={subView === "side-by-side"}
        data-testid="ic-subview-side-by-side"
        on:click={() => (subView = "side-by-side")}
      >Side-by-side</button>
      <button
        class="ic-tab"
        class:active={subView === "onion-skin"}
        role="tab"
        aria-selected={subView === "onion-skin"}
        data-testid="ic-subview-onion-skin"
        on:click={() => (subView = "onion-skin")}
      >Onion-skin</button>
      <button
        class="ic-tab"
        class:active={subView === "heatmap"}
        role="tab"
        aria-selected={subView === "heatmap"}
        data-testid="ic-subview-heatmap"
        on:click={() => (subView = "heatmap")}
      >Heatmap</button>
    </div>

    {#if subView === "onion-skin"}
      <div class="ic-onion-ctl">
        <span class="ic-onion-lbl">left</span>
        <input
          class="ic-slider"
          type="range"
          min="0"
          max="100"
          bind:value={onionOpacity}
          data-testid="ic-onion-slider"
          aria-label="Onion-skin blend"
        />
        <span class="ic-onion-lbl">right</span>
      </div>
    {/if}

    <!-- Zoom controls apply to every sub-view, including heatmap (the heatmap's own "Zoom to changed
         region" jumps the zoom/pan without any other way back out, so Reset must stay reachable). -->
    <div class="ic-zoom-ctl">
      <button class="btn" title="Zoom out" on:click={() => zoomBy(1 / 1.2)}>−</button>
      <span class="ic-zoom-pct" data-testid="ic-zoom-pct">{Math.round(zoom * 100)}%</span>
      <button class="btn" title="Zoom in" on:click={() => zoomBy(1.2)}>+</button>
      <button class="btn" title="Reset zoom" on:click={resetView}>Reset</button>
    </div>
  </div>

  {#if subView === "side-by-side"}
    <div class="ic-sbs">
      <!-- svelte-ignore a11y-no-static-element-interactions -->
      <div
        class="ic-pane"
        bind:this={viewportEl}
        on:wheel={onWheel}
        on:pointerdown={onPointerDown}
        on:pointermove={onPointerMove}
        on:pointerup={onPointerUp}
        on:pointercancel={onPointerUp}
      >
        <img class="ic-img" style={transformStyle} src={leftSrc} alt="Left" draggable="false" data-testid="ic-left-img" />
      </div>
      <!-- svelte-ignore a11y-no-static-element-interactions -->
      <div class="ic-pane" on:wheel={onWheel} on:pointerdown={onPointerDown} on:pointermove={onPointerMove} on:pointerup={onPointerUp} on:pointercancel={onPointerUp}>
        <img class="ic-img" style={transformStyle} src={rightSrc} alt="Right" draggable="false" data-testid="ic-right-img" />
      </div>
    </div>
  {:else if subView === "onion-skin"}
    <!-- svelte-ignore a11y-no-static-element-interactions -->
    <div
      class="ic-onion"
      bind:this={viewportEl}
      on:wheel={onWheel}
      on:pointerdown={onPointerDown}
      on:pointermove={onPointerMove}
      on:pointerup={onPointerUp}
      on:pointercancel={onPointerUp}
    >
      <img class="ic-img ic-stack" style={transformStyle} src={leftSrc} alt="Left" draggable="false" data-testid="ic-onion-left" />
      <img
        class="ic-img ic-stack"
        style="{transformStyle} opacity: {onionOpacity / 100};"
        src={rightSrc}
        alt="Right"
        draggable="false"
        data-testid="ic-onion-right"
      />
    </div>
  {:else}
    <div class="ic-heat">
      <!-- svelte-ignore a11y-no-static-element-interactions -->
      <div
        class="ic-heat-img-wrap"
        bind:this={viewportEl}
        on:wheel={onWheel}
        on:pointerdown={onPointerDown}
        on:pointermove={onPointerMove}
        on:pointerup={onPointerUp}
        on:pointercancel={onPointerUp}
      >
        <!-- The image + bbox highlight share ONE transformed container so "Zoom to changed region"
             (and manual zoom/pan) move them together — applying the same transform string to each
             independently would scale them from their own, DIFFERENT origins (the img is letterboxed
             and centered inside the wrap, the bbox is positioned by percent from the wrap's corner)
             and they'd visibly drift apart under zoom. -->
        <div
          class="ic-heat-canvas"
          style="{transformStyle} aspect-ratio: {diff.width} / {diff.height};"
          data-testid="ic-heat-canvas"
        >
          <img class="ic-heat-img" src={maskSrc} alt="Diff heatmap" draggable="false" data-testid="ic-heatmap-img" />
          {#if bboxRect}
            <div
              class="ic-bbox"
              data-testid="ic-bbox"
              style="left: {bboxRect.left}%; top: {bboxRect.top}%; width: {bboxRect.width}%; height: {bboxRect.height}%;"
            ></div>
          {/if}
        </div>
      </div>
      <div class="ic-heat-stats" data-testid="ic-heat-stats">
        <div class="ic-stat"><span class="ic-stat-k">Difference</span><span class="ic-stat-v">{formatPercentDifferent(diff.percentDifferent)}</span></div>
        <div class="ic-stat"><span class="ic-stat-k">Changed pixels</span><span class="ic-stat-v">{diff.changedPixels.toLocaleString()} / {diff.totalPixels.toLocaleString()}</span></div>
        {#if diff.bbox}
          <button class="btn" data-testid="ic-zoom-to-bbox" on:click={zoomToChangedRegion}>Zoom to changed region</button>
        {:else}
          <div class="ic-stat ic-stat-equal" data-testid="ic-no-change">No changed region — images match.</div>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .ic {
    display: flex;
    flex-direction: column;
    gap: 8px;
    height: 50vh;
  }
  .ic-note {
    padding: 6px 10px;
    font-size: 12px;
    color: var(--warn);
    background: color-mix(in srgb, var(--warn) 14%, transparent);
    border: 1px solid var(--warn);
    border-radius: var(--radius);
  }
  .ic-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    flex-wrap: wrap;
  }
  .ic-tabs {
    display: flex;
    gap: 4px;
  }
  .ic-tab {
    height: 26px;
    padding: 0 10px;
    font-size: 12px;
    color: var(--text-dim);
    background: var(--surface-alt);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    cursor: pointer;
  }
  .ic-tab.active {
    color: var(--text);
    background: var(--surface);
    border-color: var(--accent);
    box-shadow: inset 0 2px 0 var(--accent);
  }
  .ic-onion-ctl {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--text-dim);
  }
  .ic-slider {
    width: 160px;
    accent-color: var(--accent);
  }
  .ic-zoom-ctl {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .ic-zoom-pct {
    min-width: 42px;
    text-align: center;
    font-size: 11.5px;
    font-variant-numeric: tabular-nums;
    color: var(--text-dim);
  }
  .btn {
    height: 26px;
    padding: 0 10px;
    font-size: 12px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--surface-alt);
    color: var(--text);
    cursor: pointer;
  }
  .ic-sbs {
    flex: 1 1 auto;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
    min-height: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
  }
  .ic-pane {
    position: relative;
    overflow: hidden;
    background: repeating-conic-gradient(var(--surface-alt) 0% 25%, var(--surface) 0% 50%) 50% / 16px 16px;
    cursor: grab;
    touch-action: none;
  }
  .ic-pane:active {
    cursor: grabbing;
  }
  .ic-img {
    max-width: 100%;
    max-height: 100%;
    display: block;
  }
  .ic-onion {
    position: relative;
    flex: 1 1 auto;
    min-height: 0;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: repeating-conic-gradient(var(--surface-alt) 0% 25%, var(--surface) 0% 50%) 50% / 16px 16px;
    cursor: grab;
    touch-action: none;
  }
  .ic-onion:active {
    cursor: grabbing;
  }
  .ic-stack {
    position: absolute;
    inset: 0;
    max-width: 100%;
    max-height: 100%;
    margin: auto;
  }
  .ic-heat {
    flex: 1 1 auto;
    display: grid;
    grid-template-columns: 1fr 220px;
    gap: 10px;
    min-height: 0;
  }
  .ic-heat-img-wrap {
    position: relative;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
    display: grid;
    place-items: center;
    background: repeating-conic-gradient(var(--surface-alt) 0% 25%, var(--surface) 0% 50%) 50% / 16px 16px;
    cursor: grab;
    touch-action: none;
  }
  .ic-heat-img-wrap:active {
    cursor: grabbing;
  }
  /* The image + its bbox overlay share this box so a shared `transform` (zoom/pan, incl. "Zoom to
     changed region") moves them together — see the template comment above. `aspect-ratio` (set inline,
     data-driven from `ImageDiff.width`/`height`) plus `max-width/max-height: 100%` is the standard
     replacement for the old img-only "shrink to fit, preserve aspect ratio" trick, letting a plain div
     letterbox correctly without an explicit width/height. */
  .ic-heat-canvas {
    position: relative;
    max-width: 100%;
    max-height: 100%;
  }
  .ic-heat-img {
    display: block;
    width: 100%;
    height: 100%;
  }
  .ic-bbox {
    position: absolute;
    border: 2px solid var(--accent);
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    pointer-events: none;
  }
  .ic-heat-stats {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .ic-stat {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 12px;
  }
  .ic-stat-k {
    color: var(--text-dim);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }
  .ic-stat-v {
    font-family: ui-monospace, monospace;
  }
  .ic-stat-equal {
    color: var(--text-dim);
    font-size: 12px;
  }
</style>
