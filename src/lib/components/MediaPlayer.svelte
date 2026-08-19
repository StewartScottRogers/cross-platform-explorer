<script lang="ts">
  /**
   * Audio/video playback with a custom themed transport (CPE-1429, epic CPE-720).
   *
   * Renders a native `<audio>`/`<video>` element (bytes streamed over Tauri's asset protocol via the
   * `src` prop — never a data URL, so a large video streams) WITHOUT the raw browser `controls`, and
   * drives it from the pure `mediaTransport` state machine so all transport logic stays testable and
   * reusable (CPE-1430 reuses the same controller). On an `error` event (unsupported codec/container)
   * it degrades to a clear message plus an "Open externally" action.
   */
  import * as mt from "../mediaTransport";
  import type { MediaType } from "../mediaTransport";
  import { displaySafeName } from "../filename";

  /** Asset-protocol URL for the file (from `convertFileSrc` in the app; injectable in tests). */
  export let src: string;
  /** Whether to render an `<audio>` or `<video>` element. */
  export let type: MediaType = "audio";
  /** File name, for the element's accessible label. */
  export let name = "";
  /** Open the file in the OS default handler — wired to the open-externally command by the parent. */
  export let openExternal: () => void = () => {};
  /** Start playing as soon as the source loads. Used by the full-screen quick-look (CPE-1430) so a
   *  stepped-to clip autoplays; stays off in the preview pane so selecting a file never blasts audio. */
  export let autoplay = false;

  let mediaEl: HTMLMediaElement | undefined;
  let state = mt.initialMediaState();
  let errored = false;

  // Reset transport + error state whenever the source changes (a new file selected).
  let lastSrc: string | null = null;
  $: if (src !== lastSrc) {
    lastSrc = src;
    errored = false;
    // Autoplay (CPE-1430) is expressed through the pure controller (`mt.play`) so the transport UI still
    // reflects intent immediately; `syncPlay` then drives the element (guarded for jsdom).
    state = autoplay ? mt.play(mt.initialMediaState()) : mt.initialMediaState();
  }

  // ---- state → element (imperative sync). Guarded for jsdom, where play()/pause() are unimplemented
  //      and some setters throw; the state machine remains the source of truth the UI renders off. ----
  $: syncPlay(mediaEl, state.playing);
  $: if (mediaEl) trySet(() => (mediaEl!.volume = mt.effectiveVolume(state)));
  $: if (mediaEl) trySet(() => (mediaEl!.muted = state.muted));
  $: if (mediaEl) trySet(() => (mediaEl!.playbackRate = state.rate));
  $: if (mediaEl) trySet(() => (mediaEl!.loop = state.loop));

  function trySet(fn: () => void) {
    try {
      fn();
    } catch {
      /* jsdom / unsupported setter — ignore */
    }
  }

  function syncPlay(el: HTMLMediaElement | undefined, playing: boolean) {
    if (!el) return;
    try {
      if (playing && el.paused) {
        const p = el.play();
        if (p && typeof p.catch === "function") p.catch(() => { /* autoplay/interrupt — ignore */ });
      } else if (!playing && !el.paused) {
        el.pause();
      }
    } catch {
      /* jsdom: HTMLMediaElement.play/pause not implemented */
    }
  }

  // ---- element → state (event readback) ----
  function onTimeUpdate() {
    if (mediaEl) state = mt.setCurrentTime(state, mediaEl.currentTime);
  }
  function onDurationChange() {
    if (mediaEl) state = mt.setDuration(state, mediaEl.duration);
  }
  function onPlay() {
    state = mt.play(state);
  }
  function onPause() {
    state = mt.pause(state);
  }
  function onEnded() {
    if (!state.loop) state = mt.pause(state);
  }
  function onError() {
    errored = true;
    state = mt.pause(state);
  }

  // ---- transport control handlers ----
  function togglePlay() {
    state = mt.togglePlay(state);
  }
  function onScrub(e: Event) {
    const t = Number((e.currentTarget as HTMLInputElement).value);
    state = mt.seek(state, t);
    if (mediaEl) trySet(() => (mediaEl!.currentTime = state.currentTime));
  }
  function onVolume(e: Event) {
    state = mt.setVolume(state, Number((e.currentTarget as HTMLInputElement).value));
  }
  function toggleMute() {
    state = mt.toggleMute(state);
  }
  function cycleRate() {
    state = mt.cycleRate(state);
  }
  function toggleLoop() {
    state = mt.toggleLoop(state);
  }

  $: scrubMax = state.duration > 0 ? state.duration : 0;
  $: canSeek = state.duration > 0;
  $: volumeSlider = state.muted ? 0 : state.volume;
</script>

{#if errored}
  <div class="mp-fallback" role="alert" data-testid="mp-fallback">
    <p class="mp-fallback-msg">Can't play this media file — its codec or container isn't supported here.</p>
    <button class="mp-open-ext" type="button" on:click={openExternal}>Open externally</button>
  </div>
{:else}
  <div class="mp" class:mp-audio={type === "audio"} class:mp-video={type === "video"}>
    {#if type === "video"}
      <!-- svelte-ignore a11y-media-has-caption -->
      <video
        class="mp-media"
        bind:this={mediaEl}
        {src}
        aria-label={displaySafeName(name)}
        on:timeupdate={onTimeUpdate}
        on:durationchange={onDurationChange}
        on:loadedmetadata={onDurationChange}
        on:play={onPlay}
        on:pause={onPause}
        on:ended={onEnded}
        on:error={onError}
      ></video>
    {:else}
      <!-- svelte-ignore a11y-media-has-caption -->
      <audio
        class="mp-media"
        bind:this={mediaEl}
        {src}
        aria-label={displaySafeName(name)}
        on:timeupdate={onTimeUpdate}
        on:durationchange={onDurationChange}
        on:loadedmetadata={onDurationChange}
        on:play={onPlay}
        on:pause={onPause}
        on:ended={onEnded}
        on:error={onError}
      ></audio>
    {/if}

    <div class="mp-transport" data-testid="mp-transport">
      <button
        class="mp-btn mp-play"
        type="button"
        data-testid="mp-playpause"
        aria-label={state.playing ? "Pause" : "Play"}
        aria-pressed={state.playing}
        title={state.playing ? "Pause" : "Play"}
        on:click={togglePlay}
      >
        {#if state.playing}
          <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"
            ><rect x="6" y="5" width="4" height="14" rx="1" fill="currentColor" /><rect x="14" y="5" width="4" height="14" rx="1" fill="currentColor" /></svg>
        {:else}
          <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"
            ><path d="M8 5.5v13l11-6.5z" fill="currentColor" /></svg>
        {/if}
      </button>

      <span class="mp-time" data-testid="mp-current">{mt.formatTime(state.currentTime)}</span>

      <input
        class="mp-scrub"
        type="range"
        min="0"
        max={scrubMax}
        step="0.1"
        value={state.currentTime}
        disabled={!canSeek}
        aria-label="Seek"
        data-testid="mp-scrub"
        on:input={onScrub}
      />

      <span class="mp-time mp-dur" data-testid="mp-duration">{mt.formatTime(state.duration)}</span>

      <button
        class="mp-btn"
        type="button"
        data-testid="mp-mute"
        aria-label={state.muted ? "Unmute" : "Mute"}
        aria-pressed={state.muted}
        title={state.muted ? "Unmute" : "Mute"}
        on:click={toggleMute}
      >
        {#if state.muted}
          <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"
            ><path d="M4 9v6h4l5 4V5L8 9z" fill="currentColor" /><path d="M16 9l5 5M21 9l-5 5" stroke="currentColor" stroke-width="1.8" fill="none" stroke-linecap="round" /></svg>
        {:else}
          <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"
            ><path d="M4 9v6h4l5 4V5L8 9z" fill="currentColor" /><path d="M16 8.5a4 4 0 0 1 0 7" stroke="currentColor" stroke-width="1.8" fill="none" stroke-linecap="round" /></svg>
        {/if}
      </button>

      <input
        class="mp-vol"
        type="range"
        min="0"
        max="1"
        step="0.05"
        value={volumeSlider}
        aria-label="Volume"
        data-testid="mp-vol"
        on:input={onVolume}
      />

      <button
        class="mp-btn mp-rate"
        type="button"
        data-testid="mp-rate"
        aria-label="Playback speed"
        title="Playback speed"
        on:click={cycleRate}
      >{state.rate}&times;</button>

      <button
        class="mp-btn mp-loop"
        class:on={state.loop}
        type="button"
        data-testid="mp-loop"
        aria-label="Loop"
        aria-pressed={state.loop}
        title="Loop"
        on:click={toggleLoop}
      >
        <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true"
          ><path d="M4 12a5 5 0 0 1 5-5h9m0 0l-3-3m3 3l-3 3M20 12a5 5 0 0 1-5 5H6m0 0l3 3m-3-3l3-3" stroke="currentColor" stroke-width="1.7" fill="none" stroke-linecap="round" stroke-linejoin="round" /></svg>
      </button>
    </div>
  </div>
{/if}

<style>
  .mp {
    display: flex;
    flex-direction: column;
    gap: 10px;
    width: 100%;
    height: 100%;
    padding: 12px;
    box-sizing: border-box;
    justify-content: center;
    align-items: center;
  }
  .mp-video .mp-media {
    flex: 1 1 auto;
    min-height: 0;
    max-width: 100%;
    max-height: 100%;
    background: #000;
    border-radius: var(--radius);
  }
  .mp-audio {
    /* An audio clip has no picture — centre the transport bar in the pane. */
    justify-content: center;
  }
  .mp-audio .mp-media {
    display: none; /* the custom transport replaces the raw element's controls */
  }

  /* ---- custom transport bar ---- */
  .mp-transport {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    width: 100%;
    max-width: 720px;
    padding: 8px 10px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    box-sizing: border-box;
  }
  .mp-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: 0 0 auto;
    height: 30px;
    min-width: 30px;
    padding: 0 6px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text);
    font-size: 12px;
    line-height: 1;
    cursor: pointer;
  }
  .mp-btn:hover {
    border-color: var(--border-strong);
  }
  .mp-play {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
  .mp-rate {
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .mp-loop.on {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
  .mp-time {
    flex: 0 0 auto;
    font-size: 11px;
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    min-width: 34px;
    text-align: center;
  }
  .mp-scrub {
    flex: 1 1 140px;
    min-width: 120px;
    accent-color: var(--accent);
    cursor: pointer;
  }
  .mp-scrub:disabled {
    cursor: default;
    opacity: 0.6;
  }
  .mp-vol {
    flex: 0 0 auto;
    width: 80px;
    accent-color: var(--accent);
    cursor: pointer;
  }

  /* ---- graceful fallback (unsupported codec/container) ---- */
  .mp-fallback {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    height: 100%;
    padding: 24px;
    text-align: center;
  }
  .mp-fallback-msg {
    margin: 0;
    color: var(--text-dim);
    font-size: 13px;
    max-width: 360px;
  }
  .mp-open-ext {
    padding: 6px 14px;
    border: 1px solid var(--accent);
    border-radius: var(--radius);
    background: var(--accent);
    color: #fff;
    font-size: 13px;
    cursor: pointer;
  }
  .mp-open-ext:hover {
    filter: brightness(0.95);
  }
</style>
