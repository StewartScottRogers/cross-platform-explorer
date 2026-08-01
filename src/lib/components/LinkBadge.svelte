<script lang="ts">
  /**
   * Link glyph + "resolves to …" target indicator for a FileList row (CPE-1208, epic CPE-715).
   *
   * The caller renders this ONLY for entries where `DirEntry.is_symlink` is true (CPE-1206) — a folder
   * with zero symlinks mounts zero of these, so the hot listing/virtualization path pays nothing for the
   * common case (PURPOSE.md). Within a symlink row, the `linkStatus` backend call (which does a real
   * stat of the target) is fetched LAZILY once the badge nears the viewport — mirroring ThumbnailImage's
   * IntersectionObserver pattern (CPE-643) — NOT eagerly for every symlink row, so a folder with hundreds
   * of links never fires hundreds of status calls at once; scrolling pulls more in on demand. A hover
   * also kicks the fetch, for the (non-virtualized-window) case a badge is reached by mouse before the
   * observer's rootMargin would have fired it.
   *
   * Until the fetch resolves (and forever, if it fails) the badge shows a neutral "link" glyph with a
   * generic tooltip. Once resolved: a broken target (`link_status.broken`) flips it to the distinct
   * broken-badge state; an intact target shows "Resolves to …" as the tooltip.
   */
  import { onDestroy } from "svelte";
  import Icon from "./Icon.svelte";
  import { commands, type LinkStatus } from "../bindings.gen";
  import { t } from "../i18n";

  /** Absolute path of the symlink entry. */
  export let path: string;

  let status: LinkStatus | null = null;
  let started = false;
  let observer: IntersectionObserver | undefined;

  async function load(): Promise<void> {
    if (started) return;
    started = true;
    try {
      status = await commands.linkStatus(path);
    } catch {
      status = null; // never blocks the row — falls back to the neutral badge
    }
  }

  /** Svelte action: kick the fetch only when the badge scrolls near the viewport. Falls back to an
      eager load where IntersectionObserver is unavailable (jsdom in tests), so the feature still works
      everywhere — matches ThumbnailImage's `lazy` action (CPE-643). */
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

  $: broken = status?.broken ?? false;
  $: title = !status
    ? $t("fl.link")
    : status.broken
      ? status.target
        ? $t("fl.linkBroken", { target: status.target })
        : $t("fl.link")
      : status.target
        ? $t("fl.linkResolvesTo", { target: status.target })
        : $t("fl.link");
</script>

<!-- Not an interactive control — `mouseenter` here only opportunistically kicks the same lazy fetch
     the IntersectionObserver already triggers, so a11y's "needs an interactive role" rule doesn't
     apply; `title`/`aria-label` already make the glyph's meaning available to assistive tech. -->
<!-- svelte-ignore a11y-no-static-element-interactions -->
<span
  class="link-badge"
  class:broken
  use:lazy
  on:mouseenter={load}
  {title}
  data-testid="link-badge"
  aria-label={title}
>
  <Icon name={broken ? "link-broken" : "link"} size={12} />
</span>

<style>
  /* A small monochrome pill, sized like the other inline row badges (agent-inside-dot/tag chips) —
     never competes with the name for space (tick-tacks convention: fixed size, never grows/shrinks). */
  .link-badge {
    flex: 0 0 auto;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    margin-left: 6px;
    width: 18px;
    height: 18px;
    border-radius: 999px;
    color: var(--text-dim);
    background: var(--surface-alt);
    border: 1px solid var(--border);
  }
  /* Broken state (CPE-1208): a distinct, unmistakably "warning" treatment — same shape/position as the
     intact badge so it never jumps the row layout, just recoloured (mirrors `.agent-badge.removed`'s use
     of a warning colour for a destructive/negative state). */
  .link-badge.broken {
    color: #b5433a;
    background: color-mix(in srgb, #b5433a 12%, transparent);
    border-color: color-mix(in srgb, #b5433a 40%, transparent);
  }
</style>
