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
   * (`groupedFailures.length > 0`), never shown for plain genuine failures, which have no single set
   * worth naming as one thing.
   */
  // CPE-1881 round 5 (item 1) — gated on `groupedFailures`, the SAME set the box below actually
  // renders, never on `summary.writeRefusalCount`/`summary.allWriteRefusalPaths`. Those backend
  // scalars/arrays and `groupedFailures` (derived from `write_refusal.paths` Set membership) agree in
  // the common case, but a duplicate path, a count/paths mismatch, or a refused path with no matching
  // `failed` entry makes them diverge — the exact "heading undercounts its own list" defect round 4
  // fixed once already, reopened here through a second field. See `groupedFailures.length`/
  // `absoluteWriteRefusalPaths` below, which are the single source of truth for both the heading and
  // this affordance/copy button.
  $: showCopyRefusedAffordance = groupedFailures.length > 0;
  $: absoluteWriteRefusalPaths = (() => {
    const prefix = root.replace(/[\\/]+$/, "");
    return groupedFailures.map((f) => (prefix ? `${prefix}/${f.path}` : f.path));
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
   * **CPE-1881 round 4 (Critic finding 1) — split, not coloured, apart.** `summary.failures` mixes two
   * kinds of `outcome: "failed"` entries — a grouped write refusal (deliberate, all identical, low
   * information per row) and a genuine per-file failure (a locked file, a permission error — each worth
   * reading on its own). Round 3 tried to tell them apart with colour alone inside ONE list, and that
   * shipped two real defects the Critic caught: the list's own `Refused (N)` heading undercounted
   * whenever a genuine failure was mixed in (the heading only ever counted the grouped subset, the `<ul>`
   * rendered both), and the colour-only split was hue-only at matched lightness in light theme (fails a
   * colour-vision check) and inverted in dark theme (the quiet colour was the one row worth reading).
   * Splitting into two separate, separately-headed lists fixes both at once: each box's own count is now
   * exactly what that box contains, and telling them apart no longer depends on colour perception at all.
   * `f.grouped` is still read off `revertHoldBack.ts`'s `write_refusal.paths` membership — never inferred
   * from `error`'s wording — it now decides which BOX a row goes in rather than which colour a shared row
   * gets.
   */
  $: genuineFailures = summary.failures.filter((f) => !f.grouped);
  $: groupedFailures = summary.failures.filter((f) => f.grouped);

  /**
   * **CPE-1881 round 5 (item 4).** HELD BACK and REFUSED both got a "copy all" escape hatch
   * (CPE-1869, round 3); FAILED never did, despite being the SAME shape — an uncapped list in the SAME
   * ~10-row scroll region — and the ticket's own hypothetical (a batch of 200 locked files) lands here,
   * not in REFUSED. Mirrors `copyWriteRefusalPaths` exactly, gated on the SAME rendered list
   * (`genuineFailures`) rather than any derived count, for the same reason item 1 moved the other two
   * boxes off `summary.*` scalars.
   */
  $: showCopyFailedAffordance = genuineFailures.length > 0;
  $: absoluteFailedPaths = (() => {
    const prefix = root.replace(/[\\/]+$/, "");
    return genuineFailures.map((f) => (prefix ? `${prefix}/${f.path}` : f.path));
  })();
  let copiedFailed = false;
  let copyFailedTimer: ReturnType<typeof setTimeout> | undefined;
  async function copyFailedPaths() {
    try {
      await navigator.clipboard.writeText(formatPathsForClipboard(absoluteFailedPaths));
      copiedFailed = true;
      clearTimeout(copyFailedTimer);
      copyFailedTimer = setTimeout(() => (copiedFailed = false), 1500);
    } catch {
      /* clipboard unavailable — every failed path is still on screen, just scrolled to reach */
    }
  }
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

  {#if genuineFailures.length}
    <!-- CPE-1881 round 4 (Critic finding 1) — genuine failures now get their own box, so its own count
         is exactly what it contains. Bounded to the same ~10-row scroll region as the refused box below
         (D1's fix, extended here for consistency and against the same hypothetical: a batch of 200
         locked files would have hit the identical guillotined-list defect the ticket started with).
         CPE-1881 round 5 (item 4) — HELD BACK and REFUSED both had a copy-all button; this box, capped
         at the same ~10 rows against the exact "batch of 200 locked files" case, did not. -->
    <div class="ro-failures-box" data-testid="{testid}-failed">
      <div class="ro-failures-head">
        <span class="ro-held-label">{displaySafeName(`Failed (${genuineFailures.length})`)}</span>
        {#if showCopyFailedAffordance}
          <button
            class="mini ro-copy"
            data-testid="{testid}-copy-failed-paths"
            on:click={copyFailedPaths}
            title="Copy every failed path to the clipboard, one per line"
          >
            <Icon name={copiedFailed ? "check" : "copy"} size={13} />
            {displaySafeName(
              copiedFailed
                ? "Copied"
                : `Copy all ${genuineFailures.length} failed path${genuineFailures.length === 1 ? "" : "s"}`,
            )}
          </button>
        {/if}
      </div>
      <ul class="ro-failures ro-failures-warn">
        {#each genuineFailures as f (f.path)}
          <!-- `f.error` is NOT safe text. `apply_delete`/`apply_write` format it as `"{target}: {os
               error}"`, so a USER-CONTROLLED FILENAME is embedded in it — the same bidi/spoof class
               `displaySafeName` exists for, and it was rendering raw while `f.path` right beside it was
               escaped (CPE-1845 UAT). -->
          <li><span class="ro-fail-path" title={displaySafePath(f.path)}>{displaySafePath(f.path)}</span> — {displaySafeName(f.error)}</li>
        {/each}
      </ul>
    </div>
  {/if}

  {#if groupedFailures.length}
    <!-- CPE-1881 round 3 (D1), round 4 (Critic findings 1+2), round 5 (item 1 + item 2).
         - item 1: heading and copy button now derive from `groupedFailures.length`/
           `absoluteWriteRefusalPaths` (built from `groupedFailures` itself), never from the backend
           scalars `summary.writeRefusalCount`/`summary.allWriteRefusalPaths` — those can diverge from
           what this box actually renders (a duplicate path, a count/paths mismatch, a refused path with
           no matching `failed` entry), which is the exact "heading undercounts its own list" defect
           round 4 already fixed once, reopened here through a second field.
         - item 2: the WHY paragraph is now nested INSIDE this box, above the list it explains, instead
           of sitting beside it as an identically-styled peer box (`.ro-held`) with nothing binding it to
           what it explains. Every grey box in this panel is now structurally uniform (one outer surface
           per topic), and three stacked surfaces collapse to two. -->
    <div class="ro-failures-box" data-testid="{testid}-refused">
      {#if summary.writeRefusalReason}
        <!-- CPE-1881 round 4 (Critic finding 3) — labelled "WHY", not "Refused": the list heading right
             below is ALSO "Refused (N)", and having both say the same word would reintroduce the exact
             held-back/refusal ambiguity the round-2 labels existed to close, one clause over. Wrapped
             defensively (the CPE-1757 bidi-escape guard's own suggested fix for a new raw render):
             unlike `f.error` below, this text never embeds a path today — `revert_engine.rs` builds it
             from a count and static wording only — but there is no structural guarantee that stays
             true, so this costs nothing and closes the gap before it can open. -->
        <div class="ro-refusal-why" data-testid="{testid}-write-refusal">
          <div class="ro-held-label">Why</div>
          <div class="ro-held-reason">{displaySafeName(summary.writeRefusalReason)}</div>
        </div>
      {/if}
      <div class="ro-failures-head">
        <!-- Wrapped defensively (same reasoning as `writeRefusalReason` above): purely a count + static
             wording today, never a path, but the wrap costs nothing and the guard requires it for any
             new raw render either way. -->
        <span class="ro-held-label">{displaySafeName(`Refused (${groupedFailures.length})`)}</span>
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
                : `Copy all ${groupedFailures.length} refused path${groupedFailures.length === 1 ? "" : "s"}`,
            )}
          </button>
        {/if}
      </div>
      <ul class="ro-failures ro-failures-dim">
        {#each groupedFailures as f (f.path)}
          <!-- `f.error` is NOT safe text — see the identical comment on the Failed box above. It is
               also now the SHORT per-file link count alone (CPE-1881 round 5, item 5) — the WHY
               paragraph right above already says what a hard link is and why it was refused, so
               repeating that on every one of up to 200 rows restated both the box heading and WHY. -->
          <li><span class="ro-fail-path" title={displaySafePath(f.path)}>{displaySafePath(f.path)}</span> — {displaySafeName(f.error)}</li>
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
  /* CPE-1881 round 5 (item 2) — the WHY paragraph nested inside `.ro-failures-box`, above the
     `.ro-failures-head`/list it explains. Deliberately UNBORDERED and un-surfaced (unlike the old
     `.ro-held` peer box it replaces): the border and background are already carried by the parent
     `.ro-failures-box`, so this is spacing only — a second nested surface here would just be the same
     "peer section, not part of what it's inside" problem one layer down. */
  .ro-refusal-why { margin-bottom: 8px; }
  /* CPE-1881 round 3 (D1) — bounded to ~10 rows (~200px) so a 200-entry list stops growing the dialog
     and reads as "scroll for more" (a real scrollbar track) instead of running flush against the host
     dialog's border with nothing to signal there is more below. The DATA is never capped — every row is
     still rendered into the DOM here, just scrolled rather than all visible at once. 10 rows is a
     deliberate, considered choice (not the default) — kept over 6 (too little of the explanation stays
     primary — sorry, too little of the LIST stays visible) or 16 (list starts competing with the
     explanation above it for attention) — see the round-4 Work Log entry.
     CPE-1881 round 4 (Critic finding 2) — the browser's DEFAULT scrollbar thumb measured 2.24:1 on
     `--surface-alt` in light theme (below the 3:1 WCAG UI-component minimum; 9.62:1 in dark, so this was
     a light-theme-only failure), leaving only the half-clipped last row as a scroll cue there. Styled
     explicitly off `--border-strong` below, which clears 3:1 in both themes already (used for this same
     box's own border). `scrollbar-color` covers Firefox; the `::-webkit-scrollbar-*` pseudo-elements
     cover Chromium/WebView2, which is what this app itself ships on.
     CPE-1881 round 5 — re-measured, NOT acted on (recorded so a future `--border-strong` edit doesn't
     take this under the floor without anyone noticing): this scrollbar thumb now measures 3.33:1 in
     dark theme (down from round 4's 9.62:1 — `--border-strong` has since moved, and both themes now
     derive this thumb from it) and 3.71:1 in light theme. Both still clear the 3:1 WCAG UI-component
     floor, but neither is comfortable margin any more — dark went from "obviously fine" to "just barely
     fine" in one unrelated token change. Do not nudge `--border-strong` darker in either theme without
     re-checking this thumb; it is the tightest consumer of that token on this panel. */
  .ro-failures {
    margin: 6px 0 0; padding: 0 4px 0 18px;
    max-height: 200px; overflow-y: auto;
    scrollbar-width: thin;
    scrollbar-color: var(--border-strong) transparent;
  }
  .ro-failures::-webkit-scrollbar { width: 8px; }
  .ro-failures::-webkit-scrollbar-track { background: transparent; }
  .ro-failures::-webkit-scrollbar-thumb { background: var(--border-strong); border-radius: 4px; }
  /* CPE-1881 round 4 (Critic finding 5, closed by finding 1's split rather than by a colour swap) —
     round 3 told a genuine failure apart from a grouped refusal by colour ALONE inside one shared list:
     measured hue-only at matched lightness in light theme (a colour-vision check away from invisible)
     and inverted in dark theme (the row worth reading was the quietest one). Splitting into two
     separately-headed boxes (see the markup) makes the distinction structural — which box a row is in —
     so colour is now decoration, not the only signal. `--warn` stays reserved for the genuinely broken
     box; `--text-dim` matches the held-back list's weight for this same class of secondary, low-urgency
     detail. */
  .ro-failures-warn { color: var(--warn); }
  .ro-failures-dim { color: var(--text-dim); }
  .ro-fail-path { color: var(--text); }
</style>
