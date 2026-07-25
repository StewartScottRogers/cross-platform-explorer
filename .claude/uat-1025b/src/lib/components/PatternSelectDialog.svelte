<script lang="ts">
  /** Small input dialog for "Select by pattern" (CPE-360). Emits the glob on submit. */
  import { createEventDispatcher, onMount } from "svelte";

  const dispatch = createEventDispatcher<{ submit: string; cancel: void }>();

  let value = "*.";
  let input: HTMLInputElement;

  onMount(() => {
    input?.focus();
    input?.select();
  });

  function submit() {
    const p = value.trim();
    if (p) dispatch("submit", p);
    else dispatch("cancel");
  }
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("cancel")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("cancel")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Select by pattern" on:click|stopPropagation>
    <h2>Select by pattern</h2>
    <p>Glob against the visible names — <code>*</code> any run, <code>?</code> one character.</p>
    <input
      bind:this={input}
      bind:value
      spellcheck="false"
      aria-label="Pattern"
      placeholder="*.txt"
      on:keydown={(e) => { if (e.key === "Enter") { e.preventDefault(); submit(); } }}
    />
    <div class="actions">
      <button class="btn" on:click={() => dispatch("cancel")}>Cancel</button>
      <button class="btn primary" on:click={submit}>Select</button>
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
  h2 { font-size: 16px; margin-bottom: 8px; }
  p { color: var(--text-dim); font-size: 12.5px; margin-bottom: 12px; line-height: 1.5; }
  code { font-family: ui-monospace, monospace; background: var(--surface-alt); padding: 0 3px; border-radius: 3px; }
  input {
    width: 100%;
    height: 34px;
    padding: 0 10px;
    font: inherit;
    font-family: ui-monospace, monospace;
    color: var(--text);
    background: #fff;
    border: 1px solid var(--accent);
    border-radius: var(--radius);
    outline: none;
  }
  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 16px; }
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
