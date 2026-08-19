<script lang="ts">
  /**
   * Full-screen quick-look media player (CPE-1430, epic CPE-720). Opened with Space over a selected
   * audio/video file; steps through the current folder's media with ←/→. Navigation + repeat/shuffle
   * state live in App's {@link Playlist} (the keyboard is owned by App's handler, like the image
   * quick-look, so there's no dual-listener race) — this component renders the current track through the
   * shared {@link MediaPlayer} transport and dispatches intent events.
   *
   * Conventions: a dialog-standard overlay — dimmed backdrop, a bordered light-theme panel
   * (`--dialog-border`), click-outside / the App's Esc to close, and a focus trap so Tab can't escape to
   * the file list behind it. No screen-hijack: it's an in-app overlay, never a native window.
   */
  import { createEventDispatcher, onMount, tick } from "svelte";
  import MediaPlayer from "./MediaPlayer.svelte";
  import Icon from "./Icon.svelte";
  import type { MediaTrack, RepeatMode } from "../mediaQuickLook";
  import { displaySafeName } from "../filename";

  /** The track under the cursor to play (its `src` is resolved via {@link assetUrl}). */
  export let track: MediaTrack;
  /** 0-based cursor position + total, for the "3 / 12" counter and enabling the step buttons. */
  export let position = 0;
  export let count = 1;
  /** Current repeat mode + shuffle flag, mirrored from App's Playlist for the toggle buttons. */
  export let repeat: RepeatMode = "off";
  export let shuffled = false;
  /** Resolve a file path to a webview-loadable URL (convertFileSrc in the app; identity in tests). */
  export let assetUrl: (path: string) => string = (p) => p;
  /** Open the current file in the OS default handler (the unsupported-codec fallback path). */
  export let openExternal: (path: string) => void = () => {};

  const dispatch = createEventDispatcher<{
    close: void;
    prev: void;
    next: void;
    cycleRepeat: void;
    toggleShuffle: void;
  }>();

  $: multiple = count > 1;
  $: repeatLabel = repeat === "off" ? "Repeat off" : repeat === "one" ? "Repeat one" : "Repeat all";

  // ---- focus trap: keep Tab within the panel so the file list behind can't be reached (CPE-1430). ----
  let panelEl: HTMLElement | undefined;
  onMount(async () => {
    await tick();
    panelEl?.focus();
  });
  function trapTab(e: KeyboardEvent) {
    if (e.key !== "Tab" || !panelEl) return;
    const focusables = panelEl.querySelectorAll<HTMLElement>(
      'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
    );
    if (focusables.length === 0) {
      e.preventDefault();
      panelEl.focus();
      return;
    }
    const first = focusables[0];
    const last = focusables[focusables.length - 1];
    const active = document.activeElement;
    if (e.shiftKey && (active === first || active === panelEl)) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && active === last) {
      e.preventDefault();
      first.focus();
    }
  }
</script>

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="mq-backdrop" data-testid="media-quicklook" on:click={() => dispatch("close")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-noninteractive-element-interactions -->
  <div
    class="mq-panel"
    role="dialog"
    aria-modal="true"
    aria-label={`Media player — ${displaySafeName(track.name)}`}
    tabindex="-1"
    bind:this={panelEl}
    on:click|stopPropagation
    on:keydown={trapTab}
  >
    <div class="mq-head">
      <span class="mq-name" title={displaySafeName(track.name)}>{displaySafeName(track.name)}</span>
      {#if multiple}<span class="mq-counter">{position + 1} / {count}</span>{/if}
      <button class="mq-icon mq-close" title="Close (Esc / Space)" aria-label="Close" on:click={() => dispatch("close")}>
        <Icon name="close" size={16} />
      </button>
    </div>

    <div class="mq-body">
      <!-- Remount the player per track so its transport never leaks across clips; autoplay the stepped-to
           item. `track.path` keys it exactly like the preview pane keys MediaPlayer per file. -->
      {#key track.path}
        <MediaPlayer
          src={assetUrl(track.path)}
          type={track.type}
          name={track.name}
          autoplay={true}
          openExternal={() => openExternal(track.path)}
        />
      {/key}
    </div>

    <div class="mq-foot">
      <div class="mq-steps">
        <button class="mq-icon" title="Previous (←)" aria-label="Previous" disabled={!multiple} on:click={() => dispatch("prev")}>
          <Icon name="back" size={18} />
        </button>
        <button class="mq-icon" title="Next (→)" aria-label="Next" disabled={!multiple} on:click={() => dispatch("next")}>
          <Icon name="forward" size={18} />
        </button>
      </div>
      <div class="mq-modes">
        <button
          class="mq-chip"
          class:on={repeat !== "off"}
          title="Cycle repeat (off / all / one)"
          aria-label={repeatLabel}
          on:click={() => dispatch("cycleRepeat")}
        >{repeatLabel}</button>
        <button
          class="mq-chip"
          class:on={shuffled}
          title="Toggle shuffle"
          aria-pressed={shuffled}
          aria-label={shuffled ? "Shuffle on" : "Shuffle off"}
          on:click={() => dispatch("toggleShuffle")}
        >Shuffle {shuffled ? "on" : "off"}</button>
      </div>
    </div>
  </div>
</div>

<style>
  .mq-backdrop {
    position: fixed;
    inset: 0;
    z-index: 260;
    background: rgba(0, 0, 0, 0.45);
    display: grid;
    place-items: center;
  }
  .mq-panel {
    display: flex;
    flex-direction: column;
    width: min(92vw, 960px);
    max-height: 88vh;
    background: var(--surface);
    /* Visible border per the dialog standard (not just a shadow). */
    border: 1px solid var(--dialog-border);
    border-radius: 10px;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.4);
    outline: none;
    overflow: hidden;
  }
  .mq-head {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
  }
  .mq-name {
    flex: 1 1 auto;
    min-width: 0;
    font-size: 13px;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .mq-counter {
    flex: 0 0 auto;
    font-size: 12px;
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
  }
  .mq-body {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    /* The player fills the body; a video clip letterboxes inside it. */
    min-height: 320px;
    background: var(--surface-alt);
  }
  .mq-body :global(.mp) {
    padding: 16px;
  }
  .mq-foot {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 10px 12px;
    border-top: 1px solid var(--border);
    background: var(--surface);
  }
  .mq-steps {
    display: flex;
    gap: 6px;
  }
  /* Tick-tacks: the mode row reflows; each chip keeps its text on one line and doesn't shrink. */
  .mq-modes {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .mq-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text);
    cursor: pointer;
  }
  .mq-icon:hover { border-color: var(--border-strong); }
  .mq-icon:disabled { opacity: 0.4; cursor: default; }
  .mq-close { width: 28px; height: 28px; }
  .mq-chip {
    flex: 0 0 auto;
    height: 30px;
    padding: 0 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text);
    font-size: 12px;
    white-space: nowrap;
    cursor: pointer;
  }
  .mq-chip:hover { border-color: var(--border-strong); }
  .mq-chip.on {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
</style>
