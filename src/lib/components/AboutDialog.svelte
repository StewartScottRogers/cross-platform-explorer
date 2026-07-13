<script lang="ts">
  /** About dialog (CPE-229): app name, running version, and a docs link. The
      version is passed in (read at runtime by App via getVersion), never
      hard-coded. Link clicks are delegated to App via the `openurl` event so
      URL-opening lives in one place. */
  import { createEventDispatcher } from "svelte";

  export let version = "";
  export let repoUrl = "";

  const dispatch = createEventDispatcher<{ close: void; openurl: string }>();
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("close")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label="About" on:click|stopPropagation>
    <h2>Cross-Platform Explorer</h2>
    <p class="ver">Version {version || "—"}</p>
    <p class="desc">A fast, cross-platform file explorer with one-click install and auto-updates.</p>

    <div class="links">
      <button class="link" on:click={() => dispatch("openurl", repoUrl)}>Documentation</button>
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
    width: 400px;
    max-width: 90vw;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: 10px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25);
    padding: 20px;
  }
  h2 { font-size: 16px; margin-bottom: 4px; }
  .ver { color: var(--text-dim); font-size: 13px; margin-bottom: 12px; }
  .desc { color: var(--text-dim); line-height: 1.5; margin-bottom: 14px; }
  .links { margin-bottom: 18px; }
  .link {
    padding: 0;
    color: var(--accent);
    text-decoration: underline;
    background: transparent;
  }
  .link:hover { color: var(--accent-hover); background: transparent; }
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
</style>
