<script lang="ts">
  /**
   * A real image thumbnail for the Icons view (CPE-643, epic CPE-615).
   *
   * Fetches a downscaled PNG data URL from the backend `thumbnail(path, max_edge)`
   * command (CPE-642) — but only once the tile nears the viewport, so a folder of
   * hundreds of photos never fires hundreds of decodes at once. Until then (and on
   * any error — a non-image, an unreadable file, a decode failure) it shows the
   * generic file Icon, so a tile is never blank.
   *
   * It renders its own image, so per the BUSY-CURSOR convention it calls
   * `rawInvoke` (the untracked invoke) rather than the busy-tracking `invoke`, so a
   * background thumbnail decode never raises the app-wide wait cursor.
   */
  import { onDestroy } from "svelte";
  import Icon from "./Icon.svelte";
  import { rawInvoke } from "../invoke";

  /** Absolute path of the image file to thumbnail. */
  export let path: string;
  /** Tile edge in px — also the requested thumbnail's longest edge. */
  export let size = 96;
  /** Icon glyph shown while loading or when no thumbnail can be produced. */
  export let fallback = "image";

  let src = "";
  let failed = false;
  let started = false;
  let observer: IntersectionObserver | undefined;

  async function load(): Promise<void> {
    if (started) return;
    started = true;
    try {
      src = await rawInvoke<string>("thumbnail", { path, maxEdge: Math.round(size) });
      if (!src) failed = true;
    } catch {
      failed = true; // non-image / unreadable / decode error → keep the fallback icon
    }
  }

  /** Svelte action: kick the fetch only when the tile scrolls near the viewport.
      Falls back to an eager load where IntersectionObserver is unavailable (jsdom
      in tests), so the feature still works everywhere. */
  function lazy(node: HTMLElement) {
    if (typeof IntersectionObserver === "undefined") {
      void load();
      return;
    }
    observer = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          if (e.isIntersecting) {
            void load();
            observer?.disconnect();
            break;
          }
        }
      },
      { rootMargin: "150px" },
    );
    observer.observe(node);
    return { destroy: () => observer?.disconnect() };
  }

  onDestroy(() => observer?.disconnect());
</script>

<span class="thumb" style="--thumb-size: {size}px" use:lazy>
  {#if src && !failed}
    <img
      class="thumb-img"
      {src}
      alt=""
      draggable="false"
      on:error={() => (failed = true)}
    />
  {:else}
    <Icon name={fallback} size={size} />
  {/if}
</span>

<style>
  .thumb {
    width: var(--thumb-size);
    height: var(--thumb-size);
    display: grid;
    place-items: center;
    flex: none;
  }
  .thumb-img {
    width: var(--thumb-size);
    height: var(--thumb-size);
    object-fit: cover;
    border-radius: 6px;
    border: 1px solid var(--border);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.25);
    background: var(--surface);
  }
</style>
