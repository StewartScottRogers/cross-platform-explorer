<script lang="ts">
  /**
   * Navigation Mode cheatsheet (CPE-1555, epic CPE-1487): a read-only overlay listing the vim-style
   * bindings `reduceNavKey` (CPE-1552's `navMode.ts`) understands. Mirrors `ShortcutsDialog.svelte`'s
   * backdrop / visible-border / Escape-to-close pattern, but lives in its own file — `ShortcutsDialog`
   * is a shared file this batch deliberately avoids touching concurrently.
   *
   * INERT: nothing mounts this yet. Visibility is fully caller-controlled via the `open` prop — no
   * internal state, no keybinding wired to open it (CPE-1556 does that once the modal layer is live).
   */
  import { createEventDispatcher } from "svelte";
  import Icon from "./Icon.svelte";

  /** Whether the cheatsheet is shown. The caller owns this — toggling it is out of scope here. */
  export let open = false;

  const dispatch = createEventDispatcher<{ close: void }>();

  /** The seven Navigation Mode binding groups (mirrors the ticket's scope list + the docs page). */
  const BINDINGS: { keys: string; description: string }[] = [
    { keys: "h j k l", description: "Move left / down / up / right" },
    { keys: "gg / G", description: "Jump to the top / bottom" },
    { keys: "v", description: "Enter or exit visual mode (range select)" },
    { keys: "d / y / p", description: "Cut / copy / paste the selection" },
    { keys: "/", description: "Start filtering" },
    { keys: ":", description: "Start a command" },
    { keys: "Esc", description: "Exit visual mode" },
  ];
</script>

<svelte:window on:keydown={(e) => open && e.key === "Escape" && dispatch("close")} />

{#if open}
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
  <div class="backdrop" data-testid="nav-cheatsheet-backdrop" on:click={() => dispatch("close")}>
    <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
    <div
      class="dialog"
      role="dialog"
      aria-modal="true"
      aria-label="Navigation mode cheatsheet"
      on:click|stopPropagation
    >
      <h2>
        <span class="ic"><Icon name="keyboard" size={18} /></span>
        Navigation mode
        <button
          class="x"
          title="Close (Esc)"
          aria-label="Close"
          data-testid="nav-cheatsheet-close"
          on:click={() => dispatch("close")}
        >
          <Icon name="close" size={16} />
        </button>
      </h2>

      <div class="rows" data-testid="nav-cheatsheet-rows">
        {#each BINDINGS as b (b.keys)}
          <div class="row">
            <span class="desc">{b.description}</span>
            <kbd>{b.keys}</kbd>
          </div>
        {/each}
      </div>
    </div>
  </div>
{/if}

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
    max-width: 92vw;
    max-height: 86vh;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--dialog-border);
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
  .rows {
    overflow-y: auto;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 30px;
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
