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
  import { formatPathsForClipboard } from "../format";
  import Icon from "./Icon.svelte";

  export let outcome: RevertOutcome;
  /** Prefix for this instance's test ids, so each host screen keeps a stable hook. */
  export let testid = "revert-outcome";
  /**
   * The root the revert ran against (`CheckpointDialog`'s `path`, `AgentTimeline`'s `currentPath`,
   * `CopilotDialog`'s `root`). Optional and purely cosmetic: held-back `path`s on the wire are already
   * `/`-joined relative to it (`revert_engine.rs`'s own convention), so the copy-full-list affordance
   * below still works with an empty root — it just hands back relative paths instead of absolute ones.
   */
  export let root = "";
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

  /**
   * **CPE-1869 — "the held-back list tells you to delete files it will not show you".**
   *
   * `summary.listed` is capped at `revertHoldBack.ts`'s `MAX_LISTED` on purpose (CPE-1845
   * measured what an uncapped list costs at 200+ paths: ~185 KB of repeated rows). The gap that left open
   * was retrieval, not preview — the "delete these files yourself" advice named a set the user could see
   * at most 8 names of. `summary.allHeldBackPaths` already carries the untruncated set (it was always on
   * the wire, just never rendered past the cap), so the fix is an affordance to GET it, not a bigger cap.
   *
   * Three options were on the table (copy to clipboard / reveal+select in the file pane / write to a
   * file). Reveal-in-pane is the strongest for a single dialog with one root in view, but this panel is
   * shared across three hosts with three different relationships to "the pane" (`CheckpointDialog` has no
   * pane at all — it is palette-driven; `AgentTimeline` and `CopilotDialog` each have their own), and the
   * held-back set can span many subdirectories under the revert root, which no single directory-pane view
   * shows flatly. Write-to-file adds a save dialog and a file to clean up afterwards for a list the user
   * wants to read once. Copy-to-clipboard needs no navigation, no new window, and works identically from
   * all three hosts — the paths land wherever the user actually wants to work through them (a search box,
   * a terminal, a text editor) rather than only inside this app. That is the tradeoff this ticket accepts.
   *
   * Gated on `advisesManualDelete`, never on `outcome === "held_back_by_checkpoint"` alone: the
   * alias/collision hold-back carries that same discriminant, and its paths are the checkpoint's OWN
   * content under another spelling — offering to copy them "so you can go delete them" would be steering
   * the user to destroy the file the hold-back exists to protect. That is CPE-1869's own acceptance
   * criterion, and it is why the backend field exists rather than this component guessing from `nextStep`.
   */
  $: showCopyAffordance = summary.advisesManualDelete && summary.allHeldBackPaths.length > 0;

  /** `root` joined onto each `/`-relative held-back path, or the bare relative path when `root` is empty. */
  $: absoluteHeldBackPaths = (() => {
    const prefix = root.replace(/[\\/]+$/, "");
    return summary.allHeldBackPaths.map((p) => (prefix ? `${prefix}/${p}` : p));
  })();

  let copied = false;
  let copyTimer: ReturnType<typeof setTimeout> | undefined;
  async function copyHeldBackPaths() {
    try {
      // Same "Copy as path" quoting Explorer uses (`formatPathsForClipboard`) — one quoted path per
      // line, ready to paste into a search box, a terminal, or a script.
      await navigator.clipboard.writeText(formatPathsForClipboard(absoluteHeldBackPaths));
      copied = true;
      clearTimeout(copyTimer);
      copyTimer = setTimeout(() => (copied = false), 1500);
    } catch {
      /* clipboard unavailable — the paths are still on screen (up to MAX_LISTED of them) to copy by hand */
    }
  }
</script>

<div class="ro" data-testid={testid}>
  <div class="ro-headline">{headline}</div>

  {#if summary.reason}
    <div class="ro-held" data-testid="{testid}-held-back">
      <div class="ro-held-reason">{summary.reason}</div>
      <div class="ro-next" data-testid="{testid}-next-step">{summary.nextStep}</div>
      {#if showCopyAffordance}
        <!-- CPE-1869: the "delete these files yourself" advice above points HERE — the full held-back
             set, not just the {MAX_LISTED}-name preview below. Never shown for the alias/collision
             hold-back (see `showCopyAffordance`'s doc): those paths are the checkpoint's own content and
             must not get a delete affordance. -->
        <button
          class="mini ro-copy"
          data-testid="{testid}-copy-held-paths"
          on:click={copyHeldBackPaths}
          title="Copy every held-back path to the clipboard, one per line"
        >
          <Icon name={copied ? "check" : "copy"} size={13} />
          {copied
            ? "Copied"
            : `Copy all ${summary.heldBack} held-back path${summary.heldBack === 1 ? "" : "s"}`}
        </button>
      {/if}
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
  /* CPE-1869 — matches the `.mini` button used for the same "copy X to clipboard" gesture elsewhere
     (e.g. `PropertiesDialog.svelte`'s checksum copy); scoped here since `.mini` isn't a global class. */
  .mini {
    display: inline-flex; align-items: center; gap: 5px;
    height: 22px; padding: 0 9px; border-radius: var(--radius);
    border: 1px solid var(--border-strong); background: var(--surface-alt);
    color: var(--text); font-size: 12px; cursor: pointer;
  }
  .mini:hover { background: var(--surface); }
  .ro-copy { margin-top: 8px; }
  .ro-paths, .ro-failures { margin: 6px 0 0; padding-left: 18px; color: var(--text-dim); }
  .ro-paths li, .ro-failures li { overflow-wrap: anywhere; }
  .ro-more { list-style: none; margin-left: -18px; }
  .ro-failures { color: var(--warn); }
  .ro-fail-path { color: var(--text); }
</style>
