<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { planFindReplace, planAffix, planNumber, planCase, type RenameItem, type CaseMode } from "../batchRename";

  /** The names of the selected items to rename. */
  export let names: string[] = [];

  let mode: "replace" | "affix" | "number" | "case" = "replace";
  let find = "";
  let replace = "";
  let caseSensitive = false;
  let prefix = "";
  let suffix = "";
  let pattern = "";
  let start = 1;
  let caseMode: CaseMode = "lower";

  const dispatch = createEventDispatcher<{ apply: RenameItem[]; cancel: void }>();

  $: items =
    mode === "replace"
      ? planFindReplace(names, find, replace, caseSensitive)
      : mode === "affix"
        ? planAffix(names, prefix, suffix)
        : mode === "number"
          ? planNumber(names, pattern, Number.isFinite(start) ? start : 1)
          : planCase(names, caseMode);
  $: changed = items.filter((i) => i.changed);
  $: hasConflict = items.some((i) => i.conflict);
  $: canApply = changed.length > 0 && !hasConflict;

  function apply() {
    if (!canApply) return;
    dispatch("apply", changed);
  }
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("cancel")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("cancel")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" on:click|stopPropagation>
    <h2>Rename {names.length} items</h2>

    <div class="modes">
      <button class="mode" class:active={mode === "replace"} on:click={() => (mode = "replace")}>Find &amp; replace</button>
      <button class="mode" class:active={mode === "affix"} on:click={() => (mode = "affix")}>Add text</button>
      <button class="mode" class:active={mode === "number"} on:click={() => (mode = "number")}>Number</button>
      <button class="mode" class:active={mode === "case"} on:click={() => (mode = "case")}>Change case</button>
    </div>

    {#if mode === "replace"}
      <div class="fields">
        <label>
          <span>Find</span>
          <!-- svelte-ignore a11y-autofocus -->
          <input bind:value={find} autofocus placeholder="text to find" />
        </label>
        <label>
          <span>Replace with</span>
          <input bind:value={replace} placeholder="replacement" />
        </label>
        <label class="check">
          <input type="checkbox" bind:checked={caseSensitive} />
          Case-sensitive
        </label>
      </div>
    {:else if mode === "affix"}
      <div class="fields">
        <label>
          <span>Prefix</span>
          <input bind:value={prefix} placeholder="text before the name" />
        </label>
        <label>
          <span>Suffix</span>
          <input bind:value={suffix} placeholder="text before the extension" />
        </label>
      </div>
    {:else if mode === "number"}
      <div class="fields">
        <label>
          <span>Name pattern</span>
          <input bind:value={pattern} placeholder="photo-### (the # run becomes the number)" />
        </label>
        <label>
          <span>Start at</span>
          <input type="number" bind:value={start} min="0" style="width:90px" />
        </label>
      </div>
    {:else}
      <div class="fields">
        <label>
          <span>Case</span>
          <select bind:value={caseMode}>
            <option value="lower">lowercase</option>
            <option value="upper">UPPERCASE</option>
            <option value="title">Title Case</option>
          </select>
        </label>
      </div>
    {/if}

    <div class="preview">
      {#each items as it (it.from)}
        <div class="rowp" class:dim={!it.changed} class:conflict={it.conflict}>
          <span class="from">{it.from}</span>
          <span class="arrow">→</span>
          <span class="to">{it.to}</span>
        </div>
      {/each}
    </div>

    <div class="status">
      {#if hasConflict}
        <span class="warn">Some names would collide — adjust the inputs.</span>
      {:else if changed.length === 0}
        <span>No names change yet.</span>
      {:else}
        <span>{changed.length} of {names.length} will be renamed.</span>
      {/if}
    </div>

    <div class="actions">
      <button class="btn" on:click={() => dispatch("cancel")}>Cancel</button>
      <button class="btn primary" disabled={!canApply} on:click={apply}>Rename</button>
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
    width: 520px;
    max-width: 92vw;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: 10px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25);
    padding: 20px;
  }
  .modes { display: flex; gap: 6px; margin: 4px 0 12px; }
  .mode {
    height: 28px; padding: 0 12px; border-radius: var(--radius);
    border: 1px solid var(--border-strong); background: var(--surface-alt); font-size: 12px;
  }
  .mode.active { background: var(--accent); border-color: var(--accent); color: #fff; }
  h2 {
    font-size: 16px;
    margin-bottom: 14px;
  }
  .fields {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-bottom: 14px;
  }
  label {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 13px;
    color: var(--text-dim);
  }
  label > span {
    width: 92px;
    flex: none;
  }
  label input:not([type]) {
    flex: 1;
  }
  input:not([type="checkbox"]) {
    height: 30px;
    padding: 0 10px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--surface-alt);
  }
  label.check {
    gap: 6px;
  }
  .preview {
    max-height: 220px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 6px;
    margin-bottom: 12px;
    font-size: 12px;
    font-family: ui-monospace, "Cascadia Code", "Consolas", monospace;
  }
  .rowp {
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    gap: 8px;
    align-items: center;
    padding: 2px 4px;
  }
  .rowp .from,
  .rowp .to {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rowp .to {
    color: var(--accent);
  }
  .rowp.dim {
    opacity: 0.45;
  }
  .rowp.dim .to {
    color: inherit;
  }
  .rowp.conflict .to {
    color: #c42b1c;
    font-weight: 600;
  }
  .arrow {
    color: var(--text-faint);
  }
  .status {
    font-size: 12px;
    color: var(--text-dim);
    margin-bottom: 14px;
    min-height: 16px;
  }
  .status .warn {
    color: #c42b1c;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
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
  .btn.primary:hover:not(:disabled) {
    background: var(--accent-hover);
  }
  .btn:disabled {
    opacity: 0.5;
  }
</style>
