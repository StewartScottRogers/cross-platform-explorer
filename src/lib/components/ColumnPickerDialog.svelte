<script lang="ts">
  /**
   * Column picker (CPE-1146, epic CPE-707): add/remove/reorder the metadata columns (Dimensions,
   * Duration, Track/Year, PDF pages, …) the details view shows for the current folder, from CPE-1145's
   * `metadata_columns_available()` catalog. Every change (add, remove, reorder) is dispatched
   * immediately as the new full `active` array — the caller persists it per-folder (`settings.ts`
   * `metaColumnsByFolder`) and there's nothing to "cancel": the dialog just closes.
   */
  import { createEventDispatcher } from "svelte";
  import { t } from "../i18n";
  import Icon from "./Icon.svelte";
  import { addMetaColumn, removeMetaColumn, moveMetaColumn, type ActiveMetaColumn } from "../columns";
  import type { AvailableColumn } from "../bindings.gen";

  /** The full pickable catalog (CPE-1145), typically `$metaColumnCatalog`. */
  export let available: AvailableColumn[] = [];
  /** This folder's current active set, in display order. */
  export let active: ActiveMetaColumn[] = [];

  const dispatch = createEventDispatcher<{ change: ActiveMetaColumn[]; close: void }>();

  $: activeIds = new Set(active.map((c) => c.id));
  // Pair each active entry with its catalog row for the label; an id the catalog no longer offers
  // (stale persisted state) is dropped from THIS list rather than rendering a blank row — it still
  // round-trips in `active` untouched until the user next changes something.
  $: activeResolved = active
    .map((ac) => ({ ac, col: available.find((a) => a.id === ac.id) }))
    .filter((x): x is { ac: ActiveMetaColumn; col: AvailableColumn } => !!x.col);

  function add(id: string) {
    dispatch("change", addMetaColumn(active, id));
  }
  function remove(id: string) {
    dispatch("change", removeMetaColumn(active, id));
  }
  function move(id: string, dir: -1 | 1) {
    dispatch("change", moveMetaColumn(active, id, dir));
  }
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("close")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label={$t("cols.title")} on:click|stopPropagation>
    <div class="head-row">
      <h2>{$t("cols.title")}</h2>
      <button class="close" data-testid="close-x" aria-label={$t("common.close")} on:click={() => dispatch("close")}>
        <Icon name="close" size={14} />
      </button>
    </div>

    <div class="section">
      <div class="section-head">{$t("cols.activeHeading")}</div>
      {#if activeResolved.length === 0}
        <div class="empty" data-testid="active-empty">{$t("cols.noneActive")}</div>
      {:else}
        <div class="list" data-testid="active-list">
          {#each activeResolved as { col }, i (col.id)}
            <div class="row" data-testid="active-{col.id}">
              <span class="label">{col.label}</span>
              <span class="row-actions">
                <button
                  data-testid="up-{col.id}"
                  disabled={i === 0}
                  title={$t("cols.moveUp")}
                  aria-label={$t("cols.moveUp")}
                  on:click={() => move(col.id, -1)}
                ><Icon name="chev-up" size={12} /></button>
                <button
                  data-testid="down-{col.id}"
                  disabled={i === activeResolved.length - 1}
                  title={$t("cols.moveDown")}
                  aria-label={$t("cols.moveDown")}
                  on:click={() => move(col.id, 1)}
                ><Icon name="chev-down" size={12} /></button>
                <button
                  data-testid="remove-{col.id}"
                  class="remove"
                  title={$t("cols.removeBtn")}
                  aria-label={$t("cols.removeBtn")}
                  on:click={() => remove(col.id)}
                ><Icon name="close" size={12} /></button>
              </span>
            </div>
          {/each}
        </div>
      {/if}
    </div>

    <div class="section">
      <div class="section-head">{$t("cols.availableHeading")}</div>
      <div class="list" data-testid="available-list">
        {#each available as col (col.id)}
          <div class="row">
            <span class="label">{col.label}</span>
            <button
              data-testid="add-{col.id}"
              class="add"
              disabled={activeIds.has(col.id)}
              title={$t("cols.addBtn")}
              aria-label={$t("cols.addBtn")}
              on:click={() => add(col.id)}
            ><Icon name="plus" size={12} /></button>
          </div>
        {/each}
      </div>
    </div>

    <div class="actions">
      <button class="btn primary" data-testid="done-btn" on:click={() => dispatch("close")}>{$t("common.close")}</button>
    </div>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.25); display: grid; place-items: center; z-index: 200; }
  .dialog { width: 460px; max-width: 95vw; max-height: 85vh; overflow: auto; background: var(--surface); border: 1px solid var(--dialog-border); border-radius: 10px; box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25); padding: 20px; }
  .head-row { display: flex; align-items: center; justify-content: space-between; gap: 8px; margin-bottom: 10px; }
  h2 { font-size: 16px; }
  .close { display: grid; place-items: center; height: 26px; width: 26px; padding: 0; border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-alt); color: var(--text); }
  .section { margin-bottom: 14px; }
  .section-head { font-size: 11.5px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.03em; color: var(--text-dim); margin-bottom: 6px; }
  .empty { font-size: 12.5px; color: var(--text-dim); padding: 8px 2px; }
  .list { display: flex; flex-direction: column; gap: 4px; border: 1px solid var(--border); border-radius: var(--radius); padding: 6px; max-height: 220px; overflow: auto; }
  .row { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding: 4px 6px; border-radius: 6px; }
  .row:hover { background: var(--surface-alt); }
  .label { font-size: 12.5px; color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .row-actions { display: flex; gap: 4px; flex: 0 0 auto; }
  .row-actions button, .add { display: grid; place-items: center; height: 22px; width: 22px; padding: 0; border: 1px solid var(--border-strong); border-radius: 5px; background: var(--surface); color: var(--text); flex: 0 0 auto; }
  .row-actions button:disabled, .add:disabled { opacity: 0.4; }
  .row-actions .remove:hover { background: var(--surface-alt); }
  .actions { display: flex; justify-content: flex-end; }
  .btn { height: 30px; padding: 0 14px; border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-alt); color: var(--text); }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
</style>
