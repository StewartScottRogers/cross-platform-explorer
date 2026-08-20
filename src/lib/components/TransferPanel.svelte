<script lang="ts">
  // Operations panel (CPE-623, epic CPE-613): a bottom-corner drawer that lists active + just-finished
  // transfers with a progress bar, live counts, and cancel/dismiss. Idle-hidden — renders nothing when
  // no transfer is running, so the plain explorer is unaffected.
  import {
    transfers, percent, cancelTransfer, dismissTransfer, transferReasonsLabel, type TransferState,
  } from "../transfers";
  import Icon from "./Icon.svelte";
  import { displaySafePath } from "../filename";

  /** Which finished rows have their reason list open (CPE-1775). Ids, so a dismissed row forgets. */
  let expanded = new Set<number>();
  function toggleReasons(id: number) {
    expanded.has(id) ? expanded.delete(id) : expanded.add(id);
    expanded = expanded; // Svelte 4: reassign to trigger reactivity on a mutated Set
  }

  // Reason lines are rendered through `displaySafePath` AT THE RENDER SITE below, not via a local
  // helper: every one starts with an **archive-controlled entry name** (`"{name}: {reason}"`), so it is
  // attacker-supplied text whose bidi/format controls would reorder the line — the CPE-1712 spoof, in
  // the one place that reports a hostile archive. `displaySafePath` rather than `displaySafeName`
  // because a reason routinely carries full paths. Inlined because `bidiRenderScan` can only prove a
  // literal `displaySafeName(…)`/`displaySafePath(…)` call safe; a wrapper named anything else reads as
  // a raw render to the guard, which is exactly the class of miss that guard was rebuilt to catch.
  // Before CPE-1775 this text was drawn raw into a hover-only `title=` tooltip.

  /** Past-tense verb for the row's op — "copied"/"moved"/"compressed"/"extracted" (CPE-1184). */
  function verb(t: TransferState): string {
    switch (t.op) {
      case "move": return "moved";
      case "compress": return "compressed";
      case "extract": return "extracted";
      default: return "copied";
    }
  }

  function label(t: TransferState): string {
    if (t.finished) {
      const r = t.report;
      if (!r) return "Done";
      if (r.cancelled) return `Cancelled — ${r.transferred} done`;
      if (r.failed > 0) return `${r.transferred} done, ${r.failed} failed`;
      if (r.skipped > 0) return `${r.transferred} done, ${r.skipped} skipped`;
      return `${r.transferred} item${r.transferred === 1 ? "" : "s"} ${verb(t)}`;
    }
    return (t.current ? displaySafePath(t.current) : "") || "Preparing…";
  }
</script>

{#if $transfers.length > 0}
  <div class="ops" role="region" aria-label="File transfers">
    {#each $transfers as t (t.id)}
      <div class="op" class:done={t.finished}>
        <div class="row">
          <Icon name={t.finished ? "check" : (t.op === "compress" || t.op === "extract" ? "archive" : "copy")} size={14} />
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
          <!-- CPE-1775: the reasons were hover-only, on a `title=` tooltip, in a panel the user has no
               reason to open — so a security refusal was effectively unreadable. A button instead, with
               the wording naming what actually happened (skipped vs failed) and the list one click away. -->
          {#if t.report && t.report.errors.length > 0}
            <button
              class="why"
              aria-expanded={expanded.has(t.id)}
              aria-controls="op-reasons-{t.id}"
              on:click={() => toggleReasons(t.id)}
            >{transferReasonsLabel(t.report)}</button>
          {/if}
        </div>
        {#if t.report && expanded.has(t.id) && t.report.errors.length > 0}
          <ul class="reasons" id="op-reasons-{t.id}">
            {#each t.report.errors as e}<li>{displaySafePath(e)}</li>{/each}
          </ul>
        {/if}
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
  .fill.err { background: var(--danger); }
  .sub { font-size: 11px; color: var(--text-dim); font-variant-numeric: tabular-nums; }
  .dim { color: var(--text-faint); }
  .why {
    font: inherit; color: var(--text-dim); background: none; border: 0; padding: 0 0 0 2px;
    cursor: pointer; text-decoration: underline; text-underline-offset: 2px;
  }
  .why:hover { color: var(--text); }
  .reasons {
    margin: 6px 0 0; padding: 6px 8px; list-style: none;
    background: var(--surface-alt); border-radius: 6px;
    font-size: 11px; color: var(--text); max-height: 132px; overflow-y: auto;
  }
  /* The reason text is a full sentence naming an entry and a path, so it WRAPS rather than ellipsing —
     truncating it would put the user back where CPE-1775 found them. */
  .reasons li { overflow-wrap: anywhere; }
  .reasons li + li { margin-top: 4px; }
</style>
