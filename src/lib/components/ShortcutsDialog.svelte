<script lang="ts">
  /**
   * Keyboard-shortcut cheat sheet (CPE-339). Read-only modal; renders the pure
   * SHORTCUT_GROUPS table. Same backdrop / Escape / click-away pattern as the
   * other dialogs.
   */
  import { createEventDispatcher } from "svelte";
  import Icon from "./Icon.svelte";
  import { SHORTCUT_GROUPS } from "../shortcuts";

  const dispatch = createEventDispatcher<{ close: void }>();
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("close")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Keyboard shortcuts" on:click|stopPropagation>
    <h2>
      <span class="ic"><Icon name="keyboard" size={18} /></span>
      Keyboard shortcuts
      <button class="x" title="Close (Esc)" aria-label="Close" on:click={() => dispatch("close")}>
        <Icon name="close" size={16} />
      </button>
    </h2>

    <div class="groups">
      {#each SHORTCUT_GROUPS as group (group.title)}
        <section>
          <h3>{group.title}</h3>
          {#each group.items as s (s.keys + s.description)}
            <div class="row">
              <span class="desc">{s.description}</span>
              <kbd>{s.keys}</kbd>
            </div>
          {/each}
        </section>
      {/each}
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
    width: 720px;
    max-width: 92vw;
    max-height: 86vh;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: 10px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25);
    padding: 20px;
  }
  h2 {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 16px;
    margin-bottom: 14px;
  }
  .ic { display: grid; place-items: center; color: var(--accent); }
  .x { margin-left: auto; padding: 4px; border-radius: var(--radius); color: var(--text-dim); }
  .x:hover { background: var(--active); color: var(--text); }
  .groups {
    overflow-y: auto;
    columns: 2;
    column-gap: 28px;
  }
  section {
    break-inside: avoid;
    margin-bottom: 16px;
  }
  h3 {
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-dim);
    margin-bottom: 6px;
    padding-bottom: 4px;
    border-bottom: 1px solid var(--border);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 28px;
  }
  .desc { color: var(--text); font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  kbd {
    margin-left: auto;
    flex: none;
    font-family: ui-monospace, monospace;
    font-size: 11.5px;
    color: var(--text);
    background: var(--surface-alt);
    border: 1px solid var(--border-strong);
    border-bottom-width: 2px;
    border-radius: 5px;
    padding: 2px 7px;
    white-space: nowrap;
  }
</style>
