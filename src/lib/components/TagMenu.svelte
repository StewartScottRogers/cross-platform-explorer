<script lang="ts">
  // Sidebar tag context menu (CPE-653, epic CPE-614): right-click a tag in the Tags section to rename
  // it (across every file) or delete it. A small popover positioned at the click; theme-only colours
  // per docs/design/MENUS.md (no red destructive text).
  import { createEventDispatcher, onMount } from "svelte";
  import { t } from "../i18n";

  export let x = 0;
  export let y = 0;
  export let tag = "";

  const dispatch = createEventDispatcher<{ rename: string; remove: void; close: void }>();

  let value = tag;
  let input: HTMLInputElement | undefined;
  onMount(() => input?.focus());

  function apply() {
    const v = value.trim();
    if (v && v !== tag) dispatch("rename", v);
    else dispatch("close");
  }
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("close")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="menu" role="dialog" aria-label="Tag actions" style="left:{x}px; top:{y}px" on:click|stopPropagation>
    <div class="head">{tag}</div>
    <input
      bind:this={input}
      class="rename"
      bind:value
      on:keydown={(e) => e.key === "Enter" && apply()}
      spellcheck="false"
      autocomplete="off"
      aria-label={$t("ctx.rename")}
    />
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
  .btn { height: 28px; padding: 0 10px; font: inherit; font-size: 12px; border-radius: var(--radius);
    border: 1px solid var(--border-strong); background: var(--surface-alt); color: var(--text); }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
  .btn.ghost { border-color: transparent; background: transparent; color: var(--text-dim); }
  .btn:hover { filter: brightness(1.05); }
</style>
