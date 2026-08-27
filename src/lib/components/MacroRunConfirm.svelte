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
   *   here and Run; checking it re-issues `macroRun` with `confirmedOverwrite: true`.
   * - **not confirmable** — a link (live or dangling). CPE-1734's refusal is absolute here: it is
   *   listed the same way (CPE-1869's reused approach — show the user what they're being told to act
   *   on) but there is no checkbox that unblocks it, and Run stays disabled while any are present.
   */
  import { createEventDispatcher, onMount } from "svelte";
  import { unwrap } from "../invoke";
  import { commands } from "../bindings.gen"; // typed client (CPE-964)
  import type { ActionMacro, MacroCollision, PlannedOp, ResolvedRun } from "../bindings.gen";
  import { formatPathsForClipboard } from "../format";
  import Icon from "./Icon.svelte";

  export let macro: ActionMacro;
  export let inputs: string[] = [];
  /** Scope root the executor's within-root guard checks against ("" ⇒ the backend default). */
  export let root = "";

  const dispatch = createEventDispatcher<{ close: void; ran: ResolvedRun }>();

  /** Matches `revertHoldBack.ts`'s `MAX_LISTED` (CPE-1869) — the same on-screen preview cap. */
  const MAX_LISTED = 8;

  let plan: PlannedOp[] | null = null;
  let planError = "";
  let collisions: MacroCollision[] | null = null;
  let preflightError = "";
  /** The inline instant control (CPE-1891) — a checkbox, not a modal, toggled on a dime. */
  let confirmOverwrite = false;
  let copied = false;
  let copyTimer: ReturnType<typeof setTimeout> | undefined;
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
    : confirmable.length > 0 && confirmOverwrite
      ? `Overwrite ${confirmable.length} and Run`
      : "Run";

  async function copyCollisionPaths(list: MacroCollision[]) {
    try {
      // Same "Copy as path" quoting Explorer uses (`formatPathsForClipboard`) — one quoted path per
      // line, ready to paste into a search box, a terminal, or a script (CPE-1869's reused approach).
      await navigator.clipboard.writeText(formatPathsForClipboard(list.map((c) => c.to)));
      copied = true;
      clearTimeout(copyTimer);
      copyTimer = setTimeout(() => (copied = false), 1500);
    } catch {
      /* clipboard unavailable — the paths are still on screen (up to MAX_LISTED of them) to copy by hand */
    }
  }

  async function doRun() {
    running = true;
    runError = "";
    try {
      const resolved = unwrap(await commands.macroRun(macro, inputs, root, confirmOverwrite));
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
             reused list-affordance) but there is no checkbox — nothing here can unblock it. -->
        <div class="collision blocked" data-testid="blocked-collisions">
          <div class="collision-head">
            <Icon name="link-broken" size={13} />
            {blocked.length} destination{blocked.length === 1 ? "" : "s"} can’t be overwritten — a link,
            never confirmable
          </div>
          <ul class="collision-list">
            {#each blocked.slice(0, MAX_LISTED) as c (c.op_index)}
              <li title={c.to}>{c.to}</li>
            {/each}
            {#if blocked.length > MAX_LISTED}
              <li class="more">and {blocked.length - MAX_LISTED} more</li>
            {/if}
          </ul>
        </div>
      {/if}

      {#if confirmable.length}
        <div class="collision" data-testid="confirmable-collisions">
          <div class="collision-head">
            <Icon name="info" size={13} />
            {confirmable.length} destination name{confirmable.length === 1 ? "" : "s"} already exist{confirmable.length === 1 ? "s" : ""}
          </div>
          <ul class="collision-list">
            {#each confirmable.slice(0, MAX_LISTED) as c (c.op_index)}
              <li title={c.to}>{c.to}</li>
            {/each}
            {#if confirmable.length > MAX_LISTED}
              <li class="more">and {confirmable.length - MAX_LISTED} more</li>
            {/if}
          </ul>
          <button
            class="mini"
            data-testid="copy-collisions"
            on:click={() => copyCollisionPaths(confirmable)}
            title="Copy every colliding name to the clipboard, one per line"
          >
            <Icon name={copied ? "check" : "copy"} size={13} />
            {copied ? "Copied" : `Copy all ${confirmable.length} name${confirmable.length === 1 ? "" : "s"}`}
          </button>
          <label class="confirm-check" data-testid="confirm-overwrite-label">
            <input type="checkbox" data-testid="confirm-overwrite" bind:checked={confirmOverwrite} />
            Overwrite {confirmable.length === 1 ? "this file" : "these files"}
          </label>
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
  .collision-list { margin: 6px 0 0; padding-left: 18px; color: var(--text-dim); max-height: 120px; overflow: auto; }
  .collision-list li { overflow-wrap: anywhere; font-family: ui-monospace, monospace; font-size: 11.5px; }
  .collision-list li.more { list-style: none; margin-left: -18px; font-family: inherit; }
  /* CPE-1869's "copy the whole list" affordance, same `.mini` treatment as RevertOutcomePanel. */
  .mini {
    display: inline-flex; align-items: center; gap: 5px; margin-top: 8px;
    height: 22px; padding: 0 9px; border-radius: var(--radius);
    border: 1px solid var(--border-strong); background: var(--surface); color: var(--text); font-size: 12px;
    cursor: pointer;
  }
  .mini:hover { background: var(--surface-alt); }
  /* CPE-1891 — the inline instant control: a checkbox the user flips on a dime, never a second modal. */
  .confirm-check { display: flex; align-items: center; gap: 6px; margin-top: 8px; cursor: pointer; }
  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: auto; }
  .btn { height: 32px; padding: 0 16px; border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-alt); color: var(--text); }
  .btn:disabled { opacity: 0.5; }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
</style>
