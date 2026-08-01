<script lang="ts">
  // Sidebar smart-folder context menu (CPE-667, epic CPE-614): right-click a saved smart folder to
  // rename or delete it. Mirrors TagMenu; theme-only colours per docs/design/MENUS.md (no red text).
  // CPE-1231: also reused for saved (structured) searches — the move up/down buttons reorder within
  // whichever list the caller wired up, disabled at the ends via `canMoveUp`/`canMoveDown` (the caller
  // computes those from its own store's index, since this popover only knows the single item's `id`).
  import { createEventDispatcher, onMount } from "svelte";
  import { t } from "../i18n";
  import Icon from "./Icon.svelte";

  export let x = 0;
  export let y = 0;
  export let name = "";
  /** Whether the item can move earlier/later in its list (CPE-1231). Both default true so callers that
      don't pass them (none currently) still render enabled buttons rather than silently breaking. */
  export let canMoveUp = true;
  export let canMoveDown = true;

  const dispatch = createEventDispatcher<{
    rename: string;
    remove: void;
    close: void;
    moveUp: void;
    moveDown: void;
  }>();

  let value = name;
  let input: HTMLInputElement | undefined;
  onMount(() => input?.focus());

  function apply() {
    const v = value.trim();
    if (v && v !== name) dispatch("rename", v);
    else dispatch("close");
  }
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("close")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="menu" role="dialog" aria-label="Smart folder actions" style="left:{x}px; top:{y}px" on:click|stopPropagation>
    <div class="head">{name}</div>
    <input
      bind:this={input}
      class="rename"
      bind:value
      on:keydown={(e) => e.key === "Enter" && apply()}
      spellcheck="false"
      autocomplete="off"
      aria-label={$t("ctx.rename")}
    />
    <div class="row move-row">
      <button
        class="btn icon"
        title={$t("smart.moveUp")}
        aria-label={$t("smart.moveUp")}
        disabled={!canMoveUp}
        on:click={() => dispatch("moveUp")}
      ><Icon name="chev-up" size={12} /></button>
      <button
        class="btn icon"
        title={$t("smart.moveDown")}
        aria-label={$t("smart.moveDown")}
        disabled={!canMoveDown}
        on:click={() => dispatch("moveDown")}
      ><Icon name="chev-down" size={12} /></button>
    </div>
    <div class="row">
      <button class="btn primary" on:click={apply}>{$t("common.apply")}</button>
      <button class="btn" on:click={() => dispatch("remove")}>{$t("menu.delete")}</button>
      <button class="btn ghost" on:click={() => dispatch("close")}>{$t("common.cancel")}</button>
    </div>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; z-index: 220; }
  .menu {
    position: fixed; width: 240px;
    background: var(--surface); color: var(--text);
    border: 1px solid var(--border-strong); border-radius: 8px;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.25); padding: 10px;
  }
  .head { font-size: 12px; color: var(--text-dim); margin-bottom: 6px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .rename { width: 100%; height: 30px; padding: 0 8px; font: inherit; font-size: 13px;
    border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-alt); color: var(--text); }
  .rename:focus { outline: none; border-color: var(--accent); }
  .row { display: flex; gap: 6px; margin-top: 8px; justify-content: flex-end; }
  .move-row { justify-content: flex-start; margin-top: 6px; }
  .btn { height: 28px; padding: 0 10px; font: inherit; font-size: 12px; border-radius: var(--radius);
    border: 1px solid var(--border-strong); background: var(--surface-alt); color: var(--text); }
  .btn.icon { display: flex; align-items: center; justify-content: center; width: 28px; padding: 0; }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
  .btn.ghost { border-color: transparent; background: transparent; color: var(--text-dim); }
  .btn:hover:not(:disabled) { filter: brightness(1.05); }
  .btn:disabled { opacity: 0.5; cursor: default; }
</style>
