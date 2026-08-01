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
   */
  import { createEventDispatcher, onMount } from "svelte";
  import { unwrap } from "../invoke";
  import { commands } from "../bindings.gen"; // typed client (CPE-964)
  import type { ActionMacro, PlannedOp, ResolvedRun } from "../bindings.gen";
  import Icon from "./Icon.svelte";

  export let macro: ActionMacro;
  export let inputs: string[] = [];
  /** Scope root the executor's within-root guard checks against ("" ⇒ the backend default). */
  export let root = "";

  const dispatch = createEventDispatcher<{ close: void; ran: ResolvedRun }>();

  let plan: PlannedOp[] | null = null;
  let planError = "";
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
  });

  async function doRun() {
    running = true;
    runError = "";
    try {
      const resolved = unwrap(await commands.macroRun(macro, inputs, root));
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
      {#if runError}<div class="err" data-testid="run-error">{runError}</div>{/if}
      <div class="actions">
        <button class="btn" on:click={() => dispatch("close")} disabled={running}>Cancel</button>
        <button
          class="btn primary"
          data-testid="run-btn"
          on:click={doRun}
          disabled={running || !plan || plan.length === 0}
        >
          {running ? "Running…" : "Run"}
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
  .err { color: #d05656; font-size: 12.5px; margin-bottom: 8px; }
  .dim { color: var(--text-dim); font-size: 12.5px; }
  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: auto; }
  .btn { height: 32px; padding: 0 16px; border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-alt); color: var(--text); }
  .btn:disabled { opacity: 0.5; }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
</style>
