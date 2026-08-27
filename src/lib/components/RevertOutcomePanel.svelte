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

  /**
   * **CPE-1881 round 3 (Visual Critic D1/UAT).** The failures list is deliberately uncapped on screen
   * — UAT's whole finding was that every refused path must stay individually enumerable, unlike the
   * held-back preview above — but an uncapped list scrolls, and scrolling is not the same as "you can
   * get the whole list out of the app". This is the same "copy all" escape hatch CPE-1869 built for the
   * held-back list, mirrored for the write-refusal group: gated on there being a group at all
   * (`writeRefusalCount > 0`), never shown for plain genuine failures, which have no single set worth
   * naming as one thing.
   */
  $: showCopyRefusedAffordance = summary.writeRefusalCount > 0 && summary.allWriteRefusalPaths.length > 0;
  $: absoluteWriteRefusalPaths = (() => {
    const prefix = root.replace(/[\\/]+$/, "");
    return summary.allWriteRefusalPaths.map((p) => (prefix ? `${prefix}/${p}` : p));
  })();
  let copiedRefused = false;
  let copyRefusedTimer: ReturnType<typeof setTimeout> | undefined;
  async function copyWriteRefusalPaths() {
    try {
      await navigator.clipboard.writeText(formatPathsForClipboard(absoluteWriteRefusalPaths));
      copiedRefused = true;
      clearTimeout(copyRefusedTimer);
      copyRefusedTimer = setTimeout(() => (copiedRefused = false), 1500);
    } catch {
      /* clipboard unavailable — every refused path is still on screen, just scrolled to reach */
    }
  }

  /**
   * **CPE-1881 round 3 (D1/D3).** `summary.failures` mixes two kinds of `outcome: "failed"` entries —
   * a grouped write refusal (deliberate, all identical, low information per row) and a genuine per-file
   * failure (a locked file, a permission error — each worth reading on its own). The Visual Critic found
   * the `--warn` amber weight, correctly reserved for the latter, was blanket-applied to the former: 200
   * identical grouped rows painted the whole block amber and buried the one paragraph worth reading.
   * `f.grouped` (from `revertHoldBack.ts`, itself read off the backend's `write_refusal.paths` — never
   * inferred from `error`'s wording) is what a `<li>` below keys its colour on.
   */
  $: failuresHeading =
    summary.writeRefusalCount > 0 ? `Refused (${summary.writeRefusalCount})` : `Failed (${summary.failures.length})`;
</script>

<div class="ro" data-testid={testid}>
  <div class="ro-headline">{headline}</div>

  {#if summary.reason}
    <div class="ro-held" data-testid="{testid}-held-back">
      <!-- CPE-1881 round 2 (UAT): this block sits right next to the write-refusal one below with the
           same neutral surface, and neither used to say what it was — legible on a careful read but easy
           to conflate. A one-line label each is enough to tell them apart at a glance. -->
      <div class="ro-held-label">Held back</div>
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

  {#if summary.writeRefusalReason}
    <!-- CPE-1881: the shared explanation for a whole group of write refusals (currently only the
         hard-link rule), stated once — the write-side counterpart to `.ro-held` above. The refused paths
         themselves are still listed individually below in `.ro-failures`, each with its own short
         per-path fact; this block is only the paragraph that used to be repeated on every one of them. -->
    <div class="ro-held" data-testid="{testid}-write-refusal">
      <!-- Wrapped defensively (the CPE-1757 bidi-escape guard's own suggested fix for a new raw render):
           unlike `f.error` above, this text never embeds a path today — `revert_engine.rs` builds it from
           a count and static wording only — but there is no structural guarantee that stays true, so this
           costs nothing and closes the gap before it can open. -->
      <div class="ro-held-label">Refused</div>
      <div class="ro-held-reason">{displaySafeName(summary.writeRefusalReason)}</div>
    </div>
  {/if}

  {#if summary.failures.length}
    <!-- CPE-1881 round 3 (D1). Was a bare, unbounded `<ul>` — the Visual Critic found a 200-row list ran
         flush against the host dialog's own border with zero bottom padding and no scroll cue: it read as
         guillotined, not "scroll for more". Bounded here instead, to about 10 rows, with the browser's own
         scrollbar as the explicit cue. The DATA stays uncapped — every row is still in the DOM and still
         individually reachable (UAT's own finding was that hiding any of them would be this ticket's bug
         wearing a different hat); only the on-screen HEIGHT is capped, same principle as the held-back
         block's capped preview + copy-all button above, applied the other way round (list first, count
         stated explicitly, nothing dropped). -->
    <div class="ro-failures-box" data-testid="{testid}-failures">
      <div class="ro-failures-head">
        <!-- CPE-1881 round 3: `writeRefusalCount` existed since round 2 and nothing ever rendered it —
             the only count on screen was whatever happened to be quoted inside the paragraph's own prose
             above. Stated here explicitly so it survives a reword of that sentence. -->
        <!-- Both wrapped defensively (same reasoning as `writeRefusalReason` above and the bidi-escape
             guard's own suggested fix): purely a count + static wording today, never a path, but the
             wrap costs nothing and the guard requires it for any new raw render either way. -->
        <span class="ro-held-label">{displaySafeName(failuresHeading)}</span>
        {#if showCopyRefusedAffordance}
          <button
            class="mini ro-copy"
            data-testid="{testid}-copy-refused-paths"
            on:click={copyWriteRefusalPaths}
            title="Copy every refused path to the clipboard, one per line"
          >
            <Icon name={copiedRefused ? "check" : "copy"} size={13} />
            {displaySafeName(
              copiedRefused
                ? "Copied"
                : `Copy all ${summary.writeRefusalCount} refused path${summary.writeRefusalCount === 1 ? "" : "s"}`,
            )}
          </button>
        {/if}
      </div>
      <ul class="ro-failures">
        {#each summary.failures as f (f.path)}
          <!-- `f.error` is NOT safe text. `apply_delete`/`apply_write` format it as `"{target}: {os
               error}"`, so a USER-CONTROLLED FILENAME is embedded in it — the same bidi/spoof class
               `displaySafeName` exists for, and it was rendering raw while `f.path` right beside it was
               escaped (CPE-1845 UAT). -->
          <li class:ro-fail-grouped={f.grouped}>
            <span class="ro-fail-path" title={displaySafePath(f.path)}>{displaySafePath(f.path)}</span> — {displaySafeName(f.error)}
          </li>
        {/each}
      </ul>
    </div>
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
  /* CPE-1881 round 2 — distinguishes the held-back block from the write-refusal block beside it. */
  .ro-held-label { font-weight: 600; font-size: 11px; text-transform: uppercase; letter-spacing: 0.03em; color: var(--text-dim); }
  .ro-held-reason { color: var(--text); margin-top: 2px; }
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
  .ro-paths { margin: 6px 0 0; padding-left: 18px; color: var(--text-dim); }
  .ro-paths li, .ro-failures li { overflow-wrap: anywhere; }
  .ro-more { list-style: none; margin-left: -18px; }
  /* CPE-1881 round 3 (D1) — the bordered box the failures list now sits in, matching `.ro-held`'s
     visual language (same border/radius/background) instead of floating unbounded against the host
     dialog's own border.
     CPE-1881 round 3 (Visual Critic measurement, recorded not acted on) — in BOTH themes `--surface-alt`
     measured 1–2% off the page background, so this box's separation from its surroundings is carried
     almost entirely by this 1px `--border-strong` border with no second visual cue. Do not soften or
     remove the border without adding one (a shadow, a stronger fill) to replace it. */
  .ro-failures-box {
    margin-top: 6px;
    padding: 8px 10px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--surface-alt);
  }
  .ro-failures-head { display: flex; align-items: center; justify-content: space-between; gap: 8px; flex-wrap: wrap; }
  /* CPE-1881 round 3 (D1) — bounded to ~10 rows so a 200-entry list stops growing the dialog and reads
     as "scroll for more" (a real scrollbar track) instead of running flush against the host dialog's
     border with nothing to signal there is more below. The DATA is never capped — every row from
     `summary.failures` is still rendered into the DOM here, just scrolled rather than all visible at
     once; see the markup comment above this list for why that split is deliberate. */
  .ro-failures {
    margin: 6px 0 0; padding: 0 4px 0 18px; color: var(--warn);
    max-height: 200px; overflow-y: auto;
  }
  /* CPE-1881 round 3 (D3) — a grouped write refusal is 200 identical, low-information rows (the ticket's
     own repro): the same `--warn` weight a genuine per-file failure earns (a locked file, a permission
     error — each worth reading on its own) was drowning the one paragraph worth reading in amber mass.
     Grouped rows read at the held-back list's `--text-dim` weight instead; an ungrouped `<li>` (a real
     failure) keeps `--warn` from the rule above. */
  .ro-failures li.ro-fail-grouped { color: var(--text-dim); }
  .ro-fail-path { color: var(--text); }
</style>
