<script lang="ts">
  /**
   * A real image thumbnail for the Icons/Gallery view (CPE-643, epic CPE-615), streamed through the
   * backend's priority queue + cache (CPE-1237, epic CPE-718) rather than decoded eagerly per tile.
   *
   * Two `IntersectionObserver`s split "near the viewport" from "actually on screen": a wide prefetch
   * margin requests the thumbnail at `Prefetch` priority as soon as the tile is within it (so scrolling
   * stays smooth — nothing decodes purely because it exists off-screen in a big folder), and a strict
   * (zero-margin) observer promotes the SAME request to `Visible` priority once the tile is truly on
   * screen, so what the user is looking at right now jumps the backend's `thumb_queue` ahead of anything
   * merely nearby. `requestThumbnail` (`../thumbnailClient`) batches every tile mounting/observing in the
   * same tick into one `thumbnails_stream` call, so the priority actually has something to arbitrate, and
   * resolves instantly from the shared cache on a repeat request (CPE-939's LRU, fronted by the on-disk
   * cache — thumbnails persist across sessions). On any failure (non-image, unreadable, decode error,
   * unsupported format) it shows the generic file Icon, so a tile is never blank.
   */
  import { onDestroy } from "svelte";
  import Icon from "./Icon.svelte";
  import { requestThumbnail, type ThumbPriority } from "../thumbnailClient";

  /** Absolute path of the image file to thumbnail. */
  export let path: string;
  /** Tile edge in px — also the requested thumbnail's longest edge. */
  export let size = 96;
  /** Icon glyph shown while loading or when no thumbnail can be produced. */
  export let fallback = "image";

  let src = "";
  let failed = false;
  let requestedAt: ThumbPriority | undefined; // undefined = not yet requested this mount
  let visibleObserver: IntersectionObserver | undefined;
  let prefetchObserver: IntersectionObserver | undefined;

  const PRIORITY_RANK: Record<ThumbPriority, number> = { visible: 0, prefetch: 1, background: 2 };

  async function load(priority: ThumbPriority): Promise<void> {
    // Already resolved (src/failed set), or already asked at this-or-better priority: nothing to do.
    if (src || failed) return;
    if (requestedAt !== undefined && PRIORITY_RANK[priority] >= PRIORITY_RANK[requestedAt]) return;
    requestedAt = priority;
    try {
      const dataUrl = await requestThumbnail(path, size, priority);
      if (dataUrl) src = dataUrl;
      else failed = true; // non-image / unreadable / unsupported / decode error → keep the fallback icon
    } catch {
      failed = true;
    }
  }

  /** Svelte action: `Prefetch`-priority request once the tile is within `margin` of the viewport;
      `Visible`-priority (promoting an in-flight Prefetch request) once it's strictly on screen. Falls
      back to an eager `Visible` load where IntersectionObserver is unavailable (jsdom in tests), so the
      feature still works everywhere. */
  function lazy(node: HTMLElement) {
    if (typeof IntersectionObserver === "undefined") {
      void load("visible");
      return;
    }
    prefetchObserver = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          if (e.isIntersecting) {
            void load("prefetch");
            prefetchObserver?.disconnect();
          }
        }
      },
      { rootMargin: "600px" },
    );
    visibleObserver = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          if (e.isIntersecting) {
            void load("visible");
            visibleObserver?.disconnect();
          }
        }
      },
      { rootMargin: "0px" },
    );
    prefetchObserver.observe(node);
    visibleObserver.observe(node);
    return {
      destroy: () => {
        prefetchObserver?.disconnect();
        visibleObserver?.disconnect();
      },
    };
  }

  onDestroy(() => {
    prefetchObserver?.disconnect();
    visibleObserver?.disconnect();
  });
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
