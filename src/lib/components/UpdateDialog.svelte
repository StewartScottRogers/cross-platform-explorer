<script lang="ts">
  /** Software-update prompt (CPE-230). A dumb, state-driven modal: App owns the
      updater lifecycle and drives `state`/`progress`; this just renders and emits
      `install` / `close`. Nothing installs until the user asks. It refuses to be
      dismissed mid-download so the install is never yanked away. */
  import { createEventDispatcher } from "svelte";

  export let version = "";
  export let currentVersion = "";
  export let notes = "";
  export let state: "prompt" | "downloading" | "error" = "prompt";
  export let progress = 0; // 0..100, used when not indeterminate
  export let indeterminate = false;
  export let error = "";

  const dispatch = createEventDispatcher<{ install: void; close: void }>();

  function tryClose() {
    if (state !== "downloading") dispatch("close");
  }
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && tryClose()} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={tryClose}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Software update" on:click|stopPropagation>
    <h2>Update available</h2>
    <p class="sub">
      Version {version} is ready to install{currentVersion ? ` — you have ${currentVersion}` : ""}.
    </p>

    {#if notes}
      <div class="notes" aria-label="Release notes">{notes}</div>
    {/if}

    {#if state === "downloading"}
      <div class="progress" class:indeterminate role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={indeterminate ? undefined : progress}>
        <div class="bar" style={indeterminate ? "" : `width:${progress}%`}></div>
      </div>
      <p class="status">{indeterminate ? "Downloading…" : `Downloading… ${progress}%`}</p>
    {:else if state === "error"}
      <p class="err">{error}</p>
    {/if}

    <div class="actions">
      {#if state === "prompt"}
        <button class="btn" on:click={() => dispatch("close")}>Later</button>
        <button class="btn primary" on:click={() => dispatch("install")}>Install &amp; Restart</button>
      {:else if state === "error"}
        <button class="btn" on:click={() => dispatch("close")}>Close</button>
        <button class="btn primary" on:click={() => dispatch("install")}>Try Again</button>
      {:else}
        <button class="btn" disabled>Installing…</button>
      {/if}
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.25);
    display: grid;
    place-items: center;
    z-index: 200;
  }
  .dialog {
    width: 440px;
    max-width: 90vw;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: 10px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25);
    padding: 20px;
  }
  h2 { font-size: 16px; margin-bottom: 4px; }
  .sub { color: var(--text-dim); font-size: 13px; margin-bottom: 12px; }
  .notes {
    max-height: 160px;
    overflow-y: auto;
    padding: 10px 12px;
    margin-bottom: 14px;
    background: var(--surface-alt);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    color: var(--text);
    font-size: 13px;
    line-height: 1.5;
    white-space: pre-wrap;
  }
  .progress {
    height: 8px;
    border-radius: 999px;
    background: var(--active);
    overflow: hidden;
    margin-bottom: 8px;
  }
  .progress .bar {
    height: 100%;
    background: var(--accent);
    border-radius: 999px;
    transition: width 0.15s ease;
  }
  .progress.indeterminate .bar {
    width: 40%;
    animation: slide 1.1s ease-in-out infinite;
  }
  @keyframes slide {
    0% { margin-left: -40%; }
    100% { margin-left: 100%; }
  }
  .status { color: var(--text-dim); font-size: 12px; margin-bottom: 14px; }
  .err { color: #c42b1c; font-size: 13px; margin-bottom: 14px; }
  .actions { display: flex; justify-content: flex-end; gap: 8px; }
  .btn {
    height: 32px;
    padding: 0 16px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--surface-alt);
  }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
  .btn.primary:hover { background: var(--accent-hover); }
  .btn:disabled { opacity: 0.6; }
</style>
