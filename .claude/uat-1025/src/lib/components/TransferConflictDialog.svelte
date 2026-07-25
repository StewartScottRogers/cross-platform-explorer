<script lang="ts">
  // Copy conflict chooser (CPE-624, epic CPE-613): when a paste would overwrite existing names, ask
  // once how to resolve the whole batch — Replace / Skip / Keep both — instead of silently
  // auto-renaming. Dispatches the chosen ConflictPolicy (or cancel).
  import { createEventDispatcher } from "svelte";
  import type { ConflictPolicy } from "../transfers";
  import { t } from "../i18n";

  export let count = 0;

  const dispatch = createEventDispatcher<{ choose: ConflictPolicy; cancel: void }>();
  const choose = (p: ConflictPolicy) => dispatch("choose", p);
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("cancel")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("cancel")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" on:click|stopPropagation>
    <h2>{$t("xfer.conflictTitle")}</h2>
    <p class="body">
      {count === 1 ? $t("xfer.conflictOne") : $t("xfer.conflictMany").replace("{count}", String(count))}
    </p>
    <div class="actions">
      <button class="btn primary" on:click={() => choose("overwrite")}>{$t("xfer.replace")}</button>
      <button class="btn" on:click={() => choose("keepboth")}>{$t("xfer.keepBoth")}</button>
      <button class="btn" on:click={() => choose("skip")}>{$t("xfer.skip")}</button>
      <button class="btn ghost" on:click={() => dispatch("cancel")}>{$t("common.cancel")}</button>
    </div>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.25); display: grid; place-items: center; z-index: 210; }
  .dialog {
    width: 420px; max-width: 92vw; background: var(--surface); color: var(--text);
    border: 1px solid var(--border-strong); border-radius: 10px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25); padding: 16px 18px 18px;
  }
  h2 { font-size: 16px; margin-bottom: 8px; }
  .body { font-size: 13px; color: var(--text-dim); margin-bottom: 16px; }
  .actions { display: flex; flex-wrap: wrap; gap: 8px; justify-content: flex-end; }
  .btn { height: 32px; padding: 0 14px; border-radius: var(--radius); border: 1px solid var(--border-strong); background: var(--surface-alt); color: var(--text); font: inherit; }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
  .btn.ghost { border-color: transparent; background: transparent; color: var(--text-dim); }
  .btn:hover { filter: brightness(1.05); }
</style>
