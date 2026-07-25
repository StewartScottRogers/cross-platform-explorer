<script lang="ts">
  // Operations panel (CPE-623, epic CPE-613): a bottom-corner drawer that lists active + just-finished
  // transfers with a progress bar, live counts, and cancel/dismiss. Idle-hidden — renders nothing when
  // no transfer is running, so the plain explorer is unaffected.
  import { transfers, percent, cancelTransfer, dismissTransfer, type TransferState } from "../transfers";
  import Icon from "./Icon.svelte";

  function label(t: TransferState): string {
    if (t.finished) {
      const r = t.report;
      if (!r) return "Done";
      if (r.cancelled) return `Cancelled — ${r.transferred} done`;
      if (r.failed > 0) return `${r.transferred} done, ${r.failed} failed`;
      if (r.skipped > 0) return `${r.transferred} done, ${r.skipped} skipped`;
      return `${r.transferred} item${r.transferred === 1 ? "" : "s"} copied`;
    }
    return t.current || "Preparing…";
  }
</script>

{#if $transfers.length > 0}
  <div class="ops" role="region" aria-label="File transfers">
    {#each $transfers as t (t.id)}
      <div class="op" class:done={t.finished}>
        <div class="row">
          <Icon name={t.finished ? "check" : "copy"} size={14} />
          <span class="name" title={label(t)}>{label(t)}</span>
          {#if t.finished}
            <button class="x" title="Dismiss" aria-label="Dismiss" on:click={() => dismissTransfer(t.id)}>
              <Icon name="close" size={12} />
            </button>
          {:else}
            <button class="x" title="Cancel" aria-label="Cancel" on:click={() => cancelTransfer(t.id)}>
              <Icon name="close" size={12} />
            </button>
          {/if}
        </div>
        <div class="bar"><div class="fill" class:err={(t.report?.failed ?? 0) > 0} style="width:{percent(t)}%"></div></div>
        <div class="sub">
          {percent(t)}%
          {#if t.total_items > 0}<span class="dim"> · {t.done_items}/{t.total_items} files</span>{/if}
          {#if t.report && t.report.errors.length > 0}<span class="dim" title={t.report.errors.join("\n")}> · {t.report.errors.length} error{t.report.errors.length === 1 ? "" : "s"}</span>{/if}
        </div>
      </div>
    {/each}
  </div>
{/if}

<style>
  .ops {
    position: fixed; right: 14px; bottom: 14px; z-index: 150;
    width: min(340px, 90vw); display: flex; flex-direction: column; gap: 8px;
  }
  .op {
    background: var(--surface); color: var(--text);
    border: 1px solid var(--border-strong); border-radius: 10px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.22); padding: 10px 12px;
  }
  .op.done { opacity: 0.9; }
  .row { display: flex; align-items: center; gap: 8px; }
  .name { flex: 1; min-width: 0; font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .x { width: 22px; height: 22px; display: grid; place-items: center; border-radius: 4px; flex: 0 0 auto; }
  .x:hover { background: var(--surface-alt); }
  .bar { height: 6px; margin: 8px 0 4px; background: var(--surface-alt); border-radius: 3px; overflow: hidden; }
  .fill { height: 100%; background: var(--accent); border-radius: 3px; transition: width 0.15s linear; }
  .fill.err { background: #c42b1c; }
  .sub { font-size: 11px; color: var(--text-dim); font-variant-numeric: tabular-nums; }
  .dim { color: var(--text-faint); }
</style>
