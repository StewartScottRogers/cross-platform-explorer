<script lang="ts">
  /**
   * Full side-by-side (before | after) diff view for Agent Watch "Edit Diff Peek" (CPE-746, epic
   * CPE-727). Opened from a timeline write entry when the compact inline peek (CPE-745) isn't enough:
   * shows the whole change with removed/added lines aligned across two columns. Reads the before/after
   * from the CPE-744 diff store via `sideBySideRows`. Modal + themed, matching the app's dialog
   * convention (backdrop, visible border, Escape/backdrop to close).
   */
  import { createEventDispatcher } from "svelte";
  import { sideBySideRows, type SideRow } from "../agentDiffs";

  export let path = "";
  export let before = "";
  export let after = "";

  const dispatch = createEventDispatcher<{ close: void }>();
  const baseOf = (p: string) => p.replace(/\\/g, "/").replace(/\/+$/, "").split("/").pop() || p;

  // Full change (large context) — the deep view, unlike the compact peek.
  $: rows = sideBySideRows(before, after, 100) as SideRow[];
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("close")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Diff for {baseOf(path)}" on:click|stopPropagation>
    <header class="sbs-head">
      <span class="sbs-title" title={path}>{baseOf(path)}</span>
      <span class="sbs-cols"><span>before</span><span>after</span></span>
      <button class="sbs-close" title="Close" on:click={() => dispatch("close")}>✕</button>
    </header>
    <div class="sbs-body">
      {#each rows as r}
        <div class="sbs-row">
          <div class="cell left" class:changed={r.changed && r.left !== null} class:blank={r.left === null}>{r.left ?? ""}</div>
          <div class="cell right" class:changed={r.changed && r.right !== null} class:blank={r.right === null}>{r.right ?? ""}</div>
        </div>
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
    z-index: 210;
  }
  .dialog {
    width: 900px;
    max-width: 94vw;
    max-height: 86vh;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: 10px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25);
    overflow: hidden;
  }
  .sbs-head {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--border, #3a3a3a);
    font-size: 13px;
    font-weight: 600;
  }
  .sbs-title {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sbs-cols {
    display: flex;
    gap: 0;
    flex: 0 0 auto;
    font-size: 11px;
    font-weight: 500;
    opacity: 0.6;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .sbs-cols span {
    width: 160px;
    text-align: center;
  }
  .sbs-close {
    flex: 0 0 auto;
    border: 0;
    background: none;
    color: inherit;
    cursor: pointer;
    font-size: 13px;
    padding: 2px 6px;
    border-radius: 4px;
  }
  .sbs-close:hover {
    background: rgba(128, 128, 128, 0.18);
  }
  .sbs-body {
    overflow: auto;
    font-family: var(--mono, ui-monospace, SFMono-Regular, Menlo, Consolas, monospace);
    font-size: 12px;
    line-height: 1.5;
  }
  .sbs-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
  }
  .cell {
    padding: 0 10px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    border-right: 1px solid var(--border, #3a3a3a);
  }
  .cell.right {
    border-right: 0;
  }
  .cell.changed.left {
    background: color-mix(in srgb, #b5433a 20%, transparent);
  }
  .cell.changed.right {
    background: color-mix(in srgb, #3a9d4a 22%, transparent);
  }
  .cell.blank {
    background: color-mix(in srgb, var(--text, #888) 6%, transparent);
  }
</style>
