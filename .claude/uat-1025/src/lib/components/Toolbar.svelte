<script lang="ts">
  /**
   * A thin toolbar strip whose first button is a Settings gear (CPE-226). The
   * gear toggles a popover scoped to this surface; its contents are the default
   * slot. Additional toolbar buttons can be passed via the "actions" slot (none
   * in v1). Used at the app level and at the top of each pane.
   */
  import { t } from "../i18n";

  export let label: string;

  let open = false;
  function toggle() {
    open = !open;
  }
  function close() {
    open = false;
  }
</script>

<svelte:window on:click={close} on:keydown={(e) => e.key === "Escape" && close()} />

<div class="toolbar">
  <button
    class="tb-gear"
    class:active={open}
    type="button"
    title={$t("tb.settings", { label })}
    aria-label={$t("tb.settings", { label })}
    aria-haspopup="dialog"
    aria-expanded={open}
    on:click|stopPropagation={toggle}
  >
    <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true" focusable="false">
      <path
        fill="currentColor"
        d="M9.405 1.05c-.413-1.4-2.397-1.4-2.81 0l-.1.34a1.464 1.464 0 0 1-2.105.872l-.31-.17c-1.283-.698-2.686.705-1.987 1.987l.169.311c.446.82.023 1.841-.872 2.105l-.34.1c-1.4.413-1.4 2.397 0 2.81l.34.1a1.464 1.464 0 0 1 .872 2.105l-.17.31c-.698 1.283.705 2.686 1.987 1.987l.311-.169a1.464 1.464 0 0 1 2.105.872l.1.34c.413 1.4 2.397 1.4 2.81 0l.1-.34a1.464 1.464 0 0 1 2.105-.872l.31.17c1.283.698 2.686-.705 1.987-1.987l-.169-.311a1.464 1.464 0 0 1 .872-2.105l.34-.1c1.4-.413 1.4-2.397 0-2.81l-.34-.1a1.464 1.464 0 0 1-.872-2.105l.17-.31c.698-1.283-.705-2.686-1.987-1.987l-.311.169a1.464 1.464 0 0 1-2.105-.872l-.1-.34zM8 10.93a2.929 2.929 0 1 1 0-5.86 2.929 2.929 0 0 1 0 5.858z"
      />
    </svg>
  </button>

  <slot name="actions" />

  {#if open}
    <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-noninteractive-element-interactions -->
    <div class="tb-popover" role="dialog" aria-label={$t("tb.settings", { label })} on:click|stopPropagation>
      <div class="tb-popover-head">{label}</div>
      <div class="tb-popover-body"><slot /></div>
    </div>
  {/if}
</div>

<style>
  .toolbar {
    position: relative;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 6px;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
    flex: none;
    min-height: 32px;
  }
  .tb-gear {
    display: grid;
    place-items: center;
    width: 26px;
    height: 24px;
    border-radius: var(--radius);
    color: var(--text-dim);
  }
  .tb-gear:hover { background: var(--surface-alt); color: var(--text); }
  .tb-gear.active { background: var(--selection); color: var(--text); }
  .tb-popover {
    position: absolute;
    top: calc(100% + 4px);
    left: 4px;
    z-index: 50;
    min-width: 220px;
    max-width: 320px;
    padding: 8px;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-lg);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.18);
  }
  .tb-popover-head {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-dim);
    padding: 2px 4px 6px;
    border-bottom: 1px solid var(--border);
    margin-bottom: 6px;
  }
</style>
