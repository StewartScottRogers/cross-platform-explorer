<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import Icon from "./Icon.svelte";

  export let title = "Are you sure?";
  export let message = "";
  export let confirmLabel = "OK";
  export let danger = false;

  const dispatch = createEventDispatcher<{ confirm: void; cancel: void }>();
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("cancel")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("cancel")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" on:click|stopPropagation>
    <h2>
      {#if danger}<span class="warn"><Icon name="delete" size={18} /></span>{/if}
      {title}
    </h2>
    <p>{message}</p>
    <div class="actions">
      <button class="btn" on:click={() => dispatch("cancel")}>Cancel</button>
      <button class="btn primary" class:danger on:click={() => dispatch("confirm")}>
        {confirmLabel}
      </button>
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
  h2 {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 16px;
    margin-bottom: 10px;
  }
  .warn { color: #c42b1c; display: grid; place-items: center; }
  p { color: var(--text-dim); margin-bottom: 18px; line-height: 1.5; }
  .actions { display: flex; justify-content: flex-end; gap: 8px; }
  .btn {
    height: 32px;
    padding: 0 16px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--surface-alt);
  }
  .btn.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
  .btn.primary:hover { background: var(--accent-hover); }
  .btn.primary.danger { background: #c42b1c; border-color: #c42b1c; }
  .btn.primary.danger:hover { background: #a82419; }
</style>
