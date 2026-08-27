<script lang="ts">
  /**
   * Confirm-before-run for saved macros (CPE-1191, epic CPE-739): dry-runs the macro via
   * `commands.macroPlan` and shows the flattened, unresolved op preview (CPE-938's `PlannedOp` list —
   * the same shape RunCommandConfirm shows for user commands) before an explicit Run click calls
   * `commands.macroRun`. After a successful run it shows the applied step count and offers Undo
   * (`commands.macroUndo`) — this is the ENTIRE safety gate for macro execution: nothing runs without
   * seeing the plan first, and nothing is unrecoverable without a visible Undo.
   *
   * `macro` here is already fully resolved (any `{ask:label}` token substituted client-side by the
   * caller via `macroParams.resolveAskParams` — see App.svelte's `startMacro`), so this component
   * never has to know about prompt-parameters at all.
   *
   * **CPE-1891 — the confirm-and-retry escape hatch.** CPE-1734 made a Rename/Move/Convert step refuse
   * an occupied destination rather than silently clobbering it, but the macro engine is strictly
   * all-or-nothing: one collision used to abort and roll back the WHOLE run, with no way to say "yes,
   * overwrite" and no visibility into which name collided. Alongside `commands.macroPlan`'s dry-run,
   * this now also calls `commands.macroPreflight` — a read-only real-filesystem scan of every planned
   * destination — so the WHOLE collision set is on screen before Run is ever clicked, not discovered
   * one at a time via repeated run/rollback cycles. Each collision is either:
   * - **confirmable** — a plain pre-existing file. The inline "Overwrite" checkbox below (never a
   *   second modal — the repo's inline-instant-control convention) is the only thing standing between
   *   here and Run; checking it re-issues `macroRun` with `confirmedOverwrite` naming exactly those
   *   destinations (never a blanket flag — the backend only bypasses the occupancy guard at a
   *   destination the user was actually shown and ticked).
   * - **not confirmable** — a link (live or dangling). CPE-1734's refusal is absolute here: it is
   *   listed the same way (CPE-1869's reused approach — show the user what they're being told to act
   *   on), but there is no checkbox that unblocks it, and Run stays disabled while any are present. ONE
   *   sentence under the heading explains WHY (two if a run mixes rename/move and convert kinds — they
   *   fail differently: a rename/move destroys the link, a convert writes through it) — a Visual Critic
   *   pass found the earlier per-row placement clipped mid-sentence past a handful of rows and repeated
   *   the same paragraph once per path, so this hoists it out instead: one explanation, paths stay a
   *   plain list below it.
   *
   * **Neither list is fully reversible once run (Blocker 3, PR #1044 review round 2).** A confirmed
   * overwrite replaces the occupant's bytes with nothing preserved anywhere — Undo (and a mid-run
   * rollback) can restore the NAME, never that content. The warning next to the checkbox says so before
   * the run, not after.
   */
  import { createEventDispatcher, onMount } from "svelte";
  import { unwrap } from "../invoke";
  import { commands } from "../bindings.gen"; // typed client (CPE-964)
  import type { ActionMacro, MacroCollision, PlannedOp, ResolvedRun } from "../bindings.gen";
  import { formatPathsForClipboard } from "../format";
  import { MAX_LISTED } from "../revertHoldBack"; // CPE-1869's cap — imported, not redeclared
  import Icon from "./Icon.svelte";

  export let macro: ActionMacro;
  export let inputs: string[] = [];
  /** Scope root the executor's within-root guard checks against ("" ⇒ the backend default). */
  export let root = "";

  const dispatch = createEventDispatcher<{ close: void; ran: ResolvedRun }>();

  let plan: PlannedOp[] | null = null;
  let planError = "";
  let collisions: MacroCollision[] | null = null;
  let preflightError = "";
  /** The inline instant control (CPE-1891) — a checkbox, not a modal, toggled on a dime. */
  let confirmOverwrite = false;
  let copiedBlocked = false;
  let copiedConfirmable = false;
  let copyTimerBlocked: ReturnType<typeof setTimeout> | undefined;
  let copyTimerConfirmable: ReturnType<typeof setTimeout> | undefined;
  let running = false;
  let runError = "";
  let run: ResolvedRun | null = null;
  let undoing = false;
  let undone = false;
  let undoError = "";

  onMount(async () => {
    try {
      plan = unwrap(await commands.macroPlan(macro, inputs));
    } catch (e) {
      planError = e instanceof Error ? e.message : String(e);
    }
    try {
      collisions = unwrap(await commands.macroPreflight(macro, inputs, root));
    } catch (e) {
      preflightError = e instanceof Error ? e.message : String(e);
    }
  });

  $: confirmable = (collisions ?? []).filter((c) => c.confirmable);
  $: blocked = (collisions ?? []).filter((c) => !c.confirmable);
  $: canRun =
    !!plan && plan.length > 0 && blocked.length === 0 && (confirmable.length === 0 || confirmOverwrite);
  $: runLabel = running
    ? "Running…"
    : confirmable.length > 0 && confirmOverwrite && blocked.length === 0
      ? `Overwrite ${confirmable.length} and Run`
      : "Run";

  /** CPE-1869's exact preview shape: `MAX_LISTED` items, except a list exactly one over the cap shows
   *  all of them (an "and 1 more" line is longer than the name it would replace), and `more` is the
   *  remainder beyond that. Reused here rather than re-deriving it, so both lists cap identically to the
   *  revert hold-back list. */
  function preview<T>(items: T[]): { listed: T[]; more: number } {
    const limit = items.length === MAX_LISTED + 1 ? MAX_LISTED + 1 : MAX_LISTED;
    return { listed: items.slice(0, limit), more: items.length > MAX_LISTED + 1 ? items.length - MAX_LISTED : 0 };
  }
  $: blockedPreview = preview(blocked);
  $: confirmablePreview = preview(confirmable);

  /** One representative `reason` sentence per DISTINCT hazard the blocked set contains (CPE-1891
   *  Visual Critic pass): every `rename`/`move` collision shares the same "destroys the link" wording
   *  (both are refused by the same backend guard, `symlink_slot_refusal`) and every `convert` collision
   *  shares the same "writes THROUGH it" wording (`create_slot_link_refusal`) — the two hazards are
   *  genuinely different and both need saying, but a THIRD `move` alongside a rename would just repeat
   *  the first sentence, which is exactly the "N copies of one paragraph" this hoists past. Order
   *  matches first appearance in `blocked` for a stable, non-flickering readout. */
  function representativeReasons(items: MacroCollision[]): string[] {
    const seenKinds = new Set<string>();
    const out: string[] = [];
    for (const c of items) {
      const bucket = c.kind === "convert" ? "convert" : "rename-move";
      if (!seenKinds.has(bucket)) {
        seenKinds.add(bucket);
        out.push(c.reason);
      }
    }
    return out;
  }
  $: blockedReasons = representativeReasons(blocked);

  async function copyCollisionPaths(list: MacroCollision[], which: "blocked" | "confirmable") {
    try {
      // Same "Copy as path" quoting Explorer uses (`formatPathsForClipboard`) — one quoted path per
      // line, ready to paste into a search box, a terminal, or a script (CPE-1869's reused approach).
      await navigator.clipboard.writeText(formatPathsForClipboard(list.map((c) => c.to)));
      if (which === "blocked") {
        copiedBlocked = true;
        clearTimeout(copyTimerBlocked);
        copyTimerBlocked = setTimeout(() => (copiedBlocked = false), 1500);
      } else {
        copiedConfirmable = true;
        clearTimeout(copyTimerConfirmable);
        copyTimerConfirmable = setTimeout(() => (copiedConfirmable = false), 1500);
      }
    } catch {
      /* clipboard unavailable — the paths are still on screen (up to MAX_LISTED of them) to copy by hand */
    }
  }

  async function doRun() {
    running = true;
    runError = "";
    try {
      // Only the destinations actually shown and confirmed — never a blanket flag (CPE-1891, PR #1044
      // review round 2, Blocker 2): the backend only bypasses the occupancy guard at a `to` in this
      // exact list, so a stray extra collision the user never saw still refuses.
      const confirmedDestinations = confirmOverwrite ? confirmable.map((c) => c.to) : [];
      const resolved = unwrap(await commands.macroRun(macro, inputs, root, confirmedDestinations));
      run = resolved;
      dispatch("ran", resolved);
    } catch (e) {
      runError = e instanceof Error ? e.message : String(e);
    } finally {
      running = false;
    }
  }

  async function doUndo() {
    if (!run) return;
    undoing = true;
    undoError = "";
    try {
      unwrap(await commands.macroUndo(run));
      undone = true;
    } catch (e) {
      undoError = e instanceof Error ? e.message : String(e);
    } finally {
      undoing = false;
    }
  }
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && !running && !undoing && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => !running && !undoing && dispatch("close")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" on:click|stopPropagation>
    <header>
      <Icon name="function" size={15} />
      <h2>Run macro “{macro.name}”?</h2>
      <button class="x" title="Close" on:click={() => dispatch("close")} disabled={running || undoing}>
        <Icon name="close" size={14} />
      </button>
    </header>

    {#if !run}
      <p class="warn">
        This applies <b>{macro.steps.length}</b> step{macro.steps.length === 1 ? "" : "s"} to
        <b>{inputs.length}</b> item{inputs.length === 1 ? "" : "s"}. Review the plan before running:
      </p>
      {#if planError}
        <div class="err" data-testid="plan-error">{planError}</div>
      {:else if plan === null}
        <div class="dim" data-testid="plan-loading">Planning…</div>
      {:else}
        <ul class="ops" data-testid="plan-list">
          {#each plan as op, i (i)}
            <li><span class="op-kind">{op.kind}</span> {op.input} <span class="op-arrow">→</span> {op.detail}</li>
          {/each}
          {#if plan.length === 0}<li class="dim">Nothing to run for the current selection.</li>{/if}
        </ul>
      {/if}

      {#if preflightError}
        <div class="err" data-testid="preflight-error">{preflightError}</div>
      {/if}

      {#if blocked.length}
        <!-- CPE-1734's refusal, unconditional: a link is listed so the user can SEE it (CPE-1869's
             reused list-affordance) but there is no checkbox — nothing here can unblock it. The reason
             sentence(s) sit directly under the heading (CPE-1891 Visual Critic pass) rather than once
             per row: the wording is per-KIND, not per-path, so a sentence per row was N copies of one
             paragraph AND clipped mid-sentence past a handful of blocked names. -->
        <div class="collision blocked" data-testid="blocked-collisions">
          <div class="collision-head">
            <Icon name="link-broken" size={13} />
            {blocked.length} destination{blocked.length === 1 ? "" : "s"} can’t be overwritten — a link,
            never confirmable
          </div>
          {#each blockedReasons as reason}
            <div class="collision-reason" data-testid="blocked-reason">{reason}</div>
          {/each}
          <ul class="collision-list">
            {#each blockedPreview.listed as c (c.op_index)}
              <li title={c.to}>{c.to}</li>
            {/each}
            {#if blockedPreview.more}
              <li class="more">and {blockedPreview.more} more</li>
            {/if}
          </ul>
          <!-- This is the list the user must act on BY HAND (rename/remove the link, then re-plan) —
               capped the same way the confirmable list is, so the same copy-to-clipboard need applies
               (CPE-1869's approach, and this panel had been missing it). -->
          <button
            class="mini"
            data-testid="copy-blocked-collisions"
            on:click={() => copyCollisionPaths(blocked, "blocked")}
            title="Copy every blocked name to the clipboard, one per line"
          >
            <Icon name={copiedBlocked ? "check" : "copy"} size={13} />
            {copiedBlocked ? "Copied" : `Copy all ${blocked.length} name${blocked.length === 1 ? "" : "s"}`}
          </button>
        </div>
      {/if}

      {#if confirmable.length}
        <div class="collision" data-testid="confirmable-collisions">
          <div class="collision-head">
            <Icon name="info" size={13} />
            {confirmable.length} destination name{confirmable.length === 1 ? "" : "s"} already exist{confirmable.length === 1 ? "s" : ""}
          </div>
          <ul class="collision-list">
            {#each confirmablePreview.listed as c (c.op_index)}
              <li title={c.to}>{c.to}</li>
            {/each}
            {#if confirmablePreview.more}
              <li class="more">and {confirmablePreview.more} more</li>
            {/if}
          </ul>
          <button
            class="mini"
            data-testid="copy-collisions"
            on:click={() => copyCollisionPaths(confirmable, "confirmable")}
            title="Copy every colliding name to the clipboard, one per line"
          >
            <Icon name={copiedConfirmable ? "check" : "copy"} size={13} />
            {copiedConfirmable ? "Copied" : `Copy all ${confirmable.length} name${confirmable.length === 1 ? "" : "s"}`}
          </button>
          <label class="confirm-check" data-testid="confirm-overwrite-label">
            <input type="checkbox" data-testid="confirm-overwrite" bind:checked={confirmOverwrite} />
            Overwrite {confirmable.length === 1 ? "this file" : "these files"}
          </label>
          <p class="irreversible-note" data-testid="irreversible-note">
            This can’t be undone — Undo (and a rollback if a later step fails) restores the name, not the
            content it replaces.
          </p>
        </div>
      {/if}

      {#if runError}<div class="err" data-testid="run-error">{runError}</div>{/if}
      <div class="actions">
        <button class="btn" on:click={() => dispatch("close")} disabled={running}>Cancel</button>
        <button class="btn primary" data-testid="run-btn" on:click={doRun} disabled={running || !canRun}>
          {runLabel}
        </button>
      </div>
    {:else}
      <div class="results" data-testid="run-results">
        <p class="ok">
          Applied {run.ops.length} step{run.ops.length === 1 ? "" : "s"} to “{macro.name}”.
        </p>
        {#if undone}
          <p class="ok" data-testid="undone-note">Undone — restored to the pre-run state.</p>
        {:else if undoError}
          <div class="err" data-testid="undo-error">{undoError}</div>
        {/if}
      </div>
      <div class="actions">
        {#if !undone}
          <button class="btn" data-testid="undo-btn" on:click={doUndo} disabled={undoing}>
            {undoing ? "Undoing…" : "Undo"}
          </button>
        {/if}
        <button class="btn primary" on:click={() => dispatch("close")} disabled={undoing}>Close</button>
      </div>
    {/if}
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.3); display: grid; place-items: center; z-index: 210; }
  .dialog {
    width: 560px; max-width: 94vw; max-height: 84vh; display: flex; flex-direction: column;
    background: var(--surface); color: var(--text); border: 1px solid var(--dialog-border);
    border-radius: 10px; box-shadow: 0 20px 50px rgba(0, 0, 0, 0.28); padding: 14px 18px 16px;
  }
  header { display: flex; align-items: center; gap: 8px; margin-bottom: 8px; }
  h2 { font-size: 15px; flex: 1; }
  .x { width: 28px; height: 28px; display: grid; place-items: center; color: var(--text-dim); }
  .x:hover { color: var(--text); }
  .warn { font-size: 13px; color: var(--text-dim); line-height: 1.5; margin-bottom: 8px; }
  .ops { list-style: none; margin: 0 0 12px; padding: 0; display: flex; flex-direction: column; gap: 4px; overflow: auto; max-height: 40vh; }
  .ops li {
    font-family: ui-monospace, monospace; font-size: 12px; padding: 6px 9px; border-radius: 6px;
    background: var(--surface-alt); border: 1px solid var(--border); white-space: pre-wrap; word-break: break-all;
  }
  .ops li.dim { color: var(--text-faint); font-family: inherit; }
  .op-kind { color: var(--accent); font-weight: 600; margin-right: 4px; }
  .op-arrow { color: var(--text-faint); }
  .results { margin-bottom: 8px; }
  .ok { font-size: 13px; color: var(--text); }
  .err { color: var(--danger); font-size: 12.5px; margin-bottom: 8px; }
  .dim { color: var(--text-dim); font-size: 12.5px; }
  /* CPE-1891 — the collision panel, matching RevertOutcomePanel's `.ro-held` box (CPE-1869) so the two
     "here's the list you were told to act on" surfaces read the same way. */
  .collision {
    margin-bottom: 10px; padding: 8px 10px; border: 1px solid var(--border-strong);
    border-radius: var(--radius); background: var(--surface-alt); font-size: 12.5px; color: var(--text);
  }
  .collision.blocked { border-color: var(--danger); }
  .collision-head { display: flex; align-items: center; gap: 6px; font-weight: 600; }
  .collision.blocked .collision-head { color: var(--danger); }
  /* One (or two, mixed-kind) explanation sentence(s) under the heading — CPE-1891 Visual Critic pass —
     ordinary prose, `var(--text)` (never `var(--danger)`: the red BORDER above is the "this is refused"
     signal; MENUS.md's no-red-text-for-destructive convention holds here even off a menu). */
  .collision-reason { margin-top: 4px; line-height: 1.4; color: var(--text); overflow-wrap: anywhere; }
  .collision-list { margin: 6px 0 0; padding-left: 18px; color: var(--text-dim); max-height: 120px; overflow: auto; }
  .collision-list li { overflow-wrap: anywhere; font-family: ui-monospace, monospace; font-size: 11.5px; }
  .collision-list li.more { list-style: none; margin-left: -18px; font-family: inherit; }
  /* CPE-1869's "copy the whole list" affordance, same `.mini` treatment as RevertOutcomePanel — now on
     BOTH panels (CPE-1891 Visual Critic pass): the blocked list is the one the user must act on BY
     HAND, so it needs the copy affordance at least as much as the confirmable one does. `min-width`
     keeps the button's footprint stable between "Copy all N…" and "Copied" instead of visibly shrinking. */
  .mini {
    display: inline-flex; align-items: center; gap: 5px; margin-top: 8px; min-width: 128px;
    height: 22px; padding: 0 9px; border-radius: var(--radius);
    border: 1px solid var(--border-strong); background: var(--surface); color: var(--text); font-size: 12px;
    cursor: pointer;
  }
  .mini:hover { background: var(--surface-alt); }
  /* CPE-1891 — the inline instant control: a checkbox the user flips on a dime, never a second modal.
     `min-height: 24px` meets the minimum comfortable target size (a Visual Critic pass measured the
     checkbox itself at 13×13px inside a 17px row). */
  .confirm-check { display: flex; align-items: center; gap: 6px; margin-top: 8px; min-height: 24px; cursor: pointer; }
  .irreversible-note { margin-top: 4px; font-size: 11.5px; line-height: 1.4; color: var(--text-dim); }
  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: auto; }
  .btn { height: 32px; padding: 0 16px; border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-alt); color: var(--text); }
  .btn:disabled { opacity: 0.5; }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
</style>
