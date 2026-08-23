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
  import { displaySafeName, displaySafePath } from "../filename";

  export let outcome: RevertOutcome;
  /** Prefix for this instance's test ids, so each host screen keeps a stable hook. */
  export let testid = "revert-outcome";
  /**
   * What the caller calls this operation ("Reverted", "Undo"). The headline reads
   * `"<verb> — applied N changes…"`; empty (the default) keeps the bare `"Applied N changes…"`. Two of
   * the three hosts render this panel detached from the button that produced it — CheckpointDialog's
   * `note` sits at the top of the dialog in a slot shared with "Checkpoint … captured" — so without a
   * verb the line no longer says WHAT was applied.
   */
  export let verb = "";

  $: summary = summarizeRevert(outcome);
  $: headline = verb ? `${verb} — ${lowerFirst(summary.headline)}` : summary.headline;

  /** "Applied 2 changes." → "applied 2 changes." so the verb reads as the start of the sentence. */
  const lowerFirst = (t: string) => (t ? t[0].toLowerCase() + t.slice(1) : t);
</script>

<div class="ro" data-testid={testid}>
  <div class="ro-headline">{headline}</div>

  {#if summary.reason}
    <div class="ro-held" data-testid="{testid}-held-back">
      <div class="ro-held-reason">{summary.reason}</div>
      <div class="ro-next" data-testid="{testid}-next-step">{summary.nextStep}</div>
      {#if summary.listed.length}
        <ul class="ro-paths" data-testid="{testid}-held-paths">
          {#each summary.listed as p (p.path)}
            <!-- `detail` is per-path and usually empty; for the alias/collision hold-back it names the
                 checkpoint entry this path collides with, which is the one fact that differs per path. -->
            <li title={displaySafePath(p.path)}>{displaySafePath(p.path)}{#if p.detail} — {displaySafeName(p.detail)}{/if}</li>
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
        <!-- `f.error` is NOT safe text. `apply_delete`/`apply_write` format it as `"{target}: {os
             error}"`, so a USER-CONTROLLED FILENAME is embedded in it — the same bidi/spoof class
             `displaySafeName` exists for, and it was rendering raw while `f.path` right beside it was
             escaped (CPE-1845 UAT). -->
        <li><span class="ro-fail-path" title={displaySafePath(f.path)}>{displaySafePath(f.path)}</span> — {displaySafeName(f.error)}</li>
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
