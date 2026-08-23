<script lang="ts">
  /**
   * CPE-1845 — the one place a revert result is rendered.
   *
   * Three screens showed `applied N, skipped M` and nothing else (CheckpointDialog, AgentTimeline,
   * CopilotDialog), which reported a deliberate, correct hold-back as M problems and dropped every
   * reason the backend sent. All three now render this, so the wording cannot drift apart between them
   * and the reasons `src/docs/16-checkpoints.md` promises are actually on screen.
   *
   * The hold-back is deliberately ONE statement plus a count with a capped path list — not one row per
   * path carrying a copy of the same paragraph.
   */
  import type { RevertOutcome } from "../bindings.gen";
  import { summarizeRevert } from "../revertHoldBack";
  import { displaySafePath } from "../filename";

  export let outcome: RevertOutcome;
  /** Prefix for this instance's test ids, so each host screen keeps a stable hook. */
  export let testid = "revert-outcome";

  $: summary = summarizeRevert(outcome);
</script>

<div class="ro" data-testid={testid}>
  <div class="ro-headline">{summary.headline}</div>

  {#if summary.reason}
    <div class="ro-held" data-testid="{testid}-held-back">
      <div class="ro-held-reason">{summary.reason}</div>
      <div class="ro-next" data-testid="{testid}-next-step">{summary.nextStep}</div>
      {#if summary.listed.length}
        <ul class="ro-paths" data-testid="{testid}-held-paths">
          {#each summary.listed as p (p)}
            <li title={displaySafePath(p)}>{displaySafePath(p)}</li>
          {/each}
          {#if summary.more}
            <li class="ro-more">and {summary.more} more</li>
          {/if}
        </ul>
      {/if}
    </div>
  {/if}

  {#if summary.failures.length}
    <ul class="ro-failures" data-testid="{testid}-failures">
      {#each summary.failures as f (f.path)}
        <li><span class="ro-fail-path" title={displaySafePath(f.path)}>{displaySafePath(f.path)}</span> — {f.error}</li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .ro { font-size: 12.5px; color: var(--text); }
  .ro-headline { font-weight: 600; }
  .ro-held {
    margin-top: 6px;
    padding: 8px 10px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--surface-alt);
  }
  .ro-held-reason { color: var(--text); }
  .ro-next { margin-top: 6px; color: var(--text-dim); }
  .ro-paths, .ro-failures { margin: 6px 0 0; padding-left: 18px; color: var(--text-dim); }
  .ro-paths li, .ro-failures li { overflow-wrap: anywhere; }
  .ro-more { list-style: none; margin-left: -18px; }
  .ro-failures { color: var(--warn); }
  .ro-fail-path { color: var(--text); }
</style>
