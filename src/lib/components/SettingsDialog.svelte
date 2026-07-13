<script lang="ts">
  /** App-wide Settings dialog (CPE-229) reachable from the Application menu. It
      mirrors the app-level Toolbar gear content but as a modal. It is a dumb
      view: current values come in as props, and every change is dispatched for
      App to apply + persist, so there is a single source of truth. */
  import { createEventDispatcher } from "svelte";

  export let showHidden = false;
  export let showDetails = true;

  const dispatch = createEventDispatcher<{
    close: void;
    setHidden: boolean;
    setDetails: boolean;
    reset: void;
  }>();
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("close")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Settings" on:click|stopPropagation>
    <h2>Settings</h2>

    <div class="settings-row">
      <span>Show details/preview pane</span>
      <input
        type="checkbox"
        checked={showDetails}
        on:change={(e) => dispatch("setDetails", e.currentTarget.checked)}
      />
    </div>
    <div class="settings-row">
      <span>Show hidden files</span>
      <input
        type="checkbox"
        checked={showHidden}
        on:change={(e) => dispatch("setHidden", e.currentTarget.checked)}
      />
    </div>
    <div class="settings-row">
      <button class="settings-btn" on:click={() => dispatch("reset")}>
        Reset all settings to defaults
      </button>
    </div>

    <div class="actions">
      <button class="btn primary" on:click={() => dispatch("close")}>Close</button>
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
    width: 420px;
    max-width: 90vw;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: 10px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25);
    padding: 20px;
  }
  h2 { font-size: 16px; margin-bottom: 12px; }
  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 18px; }
  .btn {
    height: 32px;
    padding: 0 16px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--surface-alt);
  }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
  .btn.primary:hover { background: var(--accent-hover); }
</style>
