<script lang="ts">
  /**
   * AI file copilot (CPE-1276, epic CPE-977, slice 2). The human-in-the-loop surface over the safe
   * backend shipped in CPE-1275: type a natural-language instruction for the CURRENT folder, review the
   * whitelisted plan it produces (per-kind counts + the ordered op list), then explicitly Confirm before
   * anything runs. The backend is the safety net (whitelisted op set, root-confined incl. symlink-safe,
   * re-validated on execute, a checkpoint taken before any op runs, deletes routed to the OS trash) — this
   * dialog's job is to make the preview unmistakable and to NEVER call execute without an explicit click
   * on the shown plan's Confirm button. One flow, four phases:
   *
   *   input → (Plan) → preview [violations ⇒ no Confirm] → (Confirm) → executed → (Undo) → reverted
   *
   * `copilotExecute` is called from exactly one place (`doConfirm`, itself called from exactly one
   * button's `on:click`) — there is no other path to it in this component (no auto-run on mount, no
   * watcher, no retry-on-error that skips the click). Mirrors ContentIndexSearchDialog/CheckpointDialog
   * for dialog chrome + `unwrap`/`commands` typed-client conventions.
   */
  import { createEventDispatcher } from "svelte";
  import { commands } from "../bindings.gen";
  import type { CopilotPlanResult, CopilotExecuteResult, RevertOutcome, FileOp } from "../bindings.gen";
  import { unwrap } from "../invoke";
  import * as settings from "../settings";
  import Icon from "./Icon.svelte";
  import RevertOutcomePanel from "./RevertOutcomePanel.svelte"; // CPE-1845 — shared, reason-carrying
  import { displaySafePath } from "../filename";

  /** The folder the instruction applies to — shown prominently so the scope is never ambiguous. */
  export let root = "";

  const dispatch = createEventDispatcher<{
    close: void;
    help: void;
    /** Files under `root` changed (a plan executed, or an undo reverted them) — the host should refresh. */
    applied: void;
    reverted: void;
    /** The user wants to configure the copilot — the host should open Settings. */
    openSettings: void;
  }>();

  // Config is read once on open (mirrors ContentIndexSearchDialog's config load) — this dialog doesn't
  // watch Settings live; closing and reopening after configuring picks up the change.
  const cfg = settings.loadCopilotConfig();
  const needsConfig = !cfg.enabled || !cfg.base_url.trim() || !cfg.model.trim();

  type Phase = "input" | "planning" | "preview" | "executing" | "executed";
  let phase: Phase = "input";

  let instruction = "";

  let planResult: CopilotPlanResult | null = null;
  let planError = "";

  let execResult: CopilotExecuteResult | null = null;
  let execError = "";

  let undoing = false;
  let undoOutcome: RevertOutcome | null = null;
  let undoError = "";

  $: canPlan = !needsConfig && instruction.trim().length > 0 && phase !== "planning" && phase !== "executing";
  $: hasViolations = (planResult?.violations.length ?? 0) > 0;

  async function doPlan() {
    if (!instruction.trim() || !root || needsConfig) return;
    phase = "planning";
    planError = "";
    planResult = null;
    execResult = null;
    execError = "";
    undoOutcome = null;
    undoError = "";
    try {
      planResult = unwrap(await commands.copilotPlan(root, instruction.trim(), cfg));
      phase = "preview";
    } catch (e) {
      planError = e instanceof Error ? e.message : String(e);
      phase = "input";
    }
  }

  /** The ONE place `copilotExecute` is called — only reachable from the Confirm button's `on:click`,
   *  never automatically. `planResult` (the exact plan just previewed) is what's sent; nothing else. */
  async function doConfirm() {
    if (!planResult || planResult.violations.length > 0 || phase !== "preview") return;
    phase = "executing";
    execError = "";
    try {
      execResult = unwrap(await commands.copilotExecute(root, planResult.plan));
      phase = "executed";
      if (execResult.checkpoint) dispatch("applied");
    } catch (e) {
      execError = e instanceof Error ? e.message : String(e);
      phase = "preview";
    }
  }

  async function doUndo() {
    if (!execResult?.checkpoint) return;
    undoing = true;
    undoError = "";
    try {
      undoOutcome = unwrap(
        await commands.checkpointRevert(root, execResult.checkpoint.checkpoint.manifest_id),
      );
      dispatch("reverted");
    } catch (e) {
      undoError = e instanceof Error ? e.message : String(e);
    } finally {
      undoing = false;
    }
  }

  function startOver() {
    phase = "input";
    instruction = "";
    planResult = null;
    planError = "";
    execResult = null;
    execError = "";
    undoOutcome = null;
    undoError = "";
  }

  function backToInput() {
    phase = "input";
    planResult = null;
    planError = "";
  }

  // ---- FileOp rendering (a closed 5-variant union: move/rename/delete/mkdir/copy) -------------------
  function opKind(op: FileOp): "move" | "rename" | "delete" | "mkdir" | "copy" {
    if ("move" in op) return "move";
    if ("rename" in op) return "rename";
    if ("delete" in op) return "delete";
    if ("mkdir" in op) return "mkdir";
    return "copy";
  }
  function opFrom(op: FileOp): string {
    if ("move" in op) return op.move.src;
    if ("rename" in op) return op.rename.path;
    if ("delete" in op) return op.delete.path;
    if ("mkdir" in op) return op.mkdir.path;
    return op.copy.src;
  }
  function opTo(op: FileOp): string | null {
    if ("move" in op) return op.move.dst;
    if ("rename" in op) return op.rename.new_name;
    if ("copy" in op) return op.copy.dst;
    return null;
  }
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && phase !== "planning" && phase !== "executing" && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => phase !== "planning" && phase !== "executing" && dispatch("close")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label="AI file copilot" on:click|stopPropagation>
    <div class="head-row">
      <h2>AI file copilot</h2>
      <button class="docs" title="Open documentation" aria-label="Open documentation" data-testid="help-btn"
        on:click={() => dispatch("help")}><Icon name="book" size={15} /></button>
      <button class="x" title="Close" aria-label="Close" data-testid="close-btn"
        on:click={() => dispatch("close")}><Icon name="close" size={14} /></button>
    </div>

    <div class="scope" data-testid="scope" title={displaySafePath(root)}>
      Scope: <strong>{displaySafePath(root)}</strong>
    </div>

    {#if needsConfig}
      <div class="setup" data-testid="needs-config">
        <p class="setup-title">The AI file copilot isn't set up yet.</p>
        <p class="dim">
          It needs a model endpoint — a local server (LM Studio, no key) or a hosted OpenAI-compatible one
          (with a key) — before it can turn an instruction into a plan.
        </p>
        <button class="btn primary" data-testid="open-settings-btn" on:click={() => dispatch("openSettings")}>
          Set it up in Settings
        </button>
      </div>
    {:else}
      <p class="warn" data-testid="warning">
        This will propose changes to files under the folder above — moves, renames, deletes, new folders,
        copies. Nothing runs until you review the plan and click <strong>Confirm</strong>.
      </p>

      {#if phase === "input" || phase === "planning"}
        <textarea
          class="instruction"
          placeholder="e.g. Move all the screenshots into a new 'Screenshots' folder"
          bind:value={instruction}
          disabled={phase === "planning"}
          data-testid="instruction-input"
          rows="3"
        />
        {#if planError}<div class="err" data-testid="plan-error">{planError}</div>{/if}
        <div class="actions">
          <button class="btn primary" data-testid="plan-btn" disabled={!canPlan} on:click={doPlan}>
            {phase === "planning" ? "Planning…" : "Plan"}
          </button>
        </div>
      {/if}

      {#if planResult && (phase === "preview" || phase === "executing" || phase === "executed")}
        <div class="preview" data-testid="preview-panel">
          <div class="preview-lead">Instruction: <em>{instruction}</em></div>

          {#if planResult.violations.length > 0}
            <div class="violations" data-testid="violations">
              <p class="violations-title">This plan was refused — it isn't safe to run:</p>
              <ul>
                {#each planResult.violations as v}
                  <li>{v}</li>
                {/each}
              </ul>
            </div>
          {:else}
            <div class="summary" data-testid="summary">
              <span>{planResult.summary.moves} move{planResult.summary.moves === 1 ? "" : "s"}</span>
              <span>{planResult.summary.renames} rename{planResult.summary.renames === 1 ? "" : "s"}</span>
              <span>{planResult.summary.deletes} delete{planResult.summary.deletes === 1 ? "" : "s"}</span>
              <span>{planResult.summary.mkdirs} new folder{planResult.summary.mkdirs === 1 ? "" : "s"}</span>
              <span>{planResult.summary.copies} copy/copies</span>
            </div>
            {#if planResult.summary.deletes > 0}
              <p class="note">Deletes go to the OS trash/Recycle Bin — recoverable, not permanent.</p>
            {/if}

            <div class="op-list" data-testid="op-list">
              {#each planResult.plan.ops as op, i (i)}
                <div class="op-row" data-testid="op-row-{i}">
                  <span class="op-kind kind-{opKind(op)}">{opKind(op)}</span>
                  <span class="op-from" title={displaySafePath(opFrom(op))}>{displaySafePath(opFrom(op))}</span>
                  {#if opTo(op)}
                    <span class="op-arrow">→</span>
                    <span class="op-to" title={displaySafePath(opTo(op) ?? "")}>{displaySafePath(opTo(op) ?? "")}</span>
                  {/if}
                </div>
              {/each}
            </div>
          {/if}

          {#if execError}<div class="err" data-testid="exec-error">{execError}</div>{/if}

          {#if phase === "preview"}
            <div class="actions">
              <button class="btn" data-testid="back-btn" on:click={backToInput}>Edit instruction</button>
              {#if planResult.violations.length === 0}
                <button class="btn primary confirm" data-testid="confirm-btn" on:click={doConfirm}>
                  Confirm — run this plan
                </button>
              {/if}
            </div>
          {:else if phase === "executing"}
            <div class="actions"><span class="dim">Running…</span></div>
          {/if}
        </div>
      {/if}

      {#if execResult && phase === "executed"}
        <div class="results" data-testid="exec-results">
          {#if execResult.violations.length > 0}
            <div class="violations" data-testid="exec-violations">
              <p class="violations-title">Nothing ran — the plan no longer validated:</p>
              <ul>
                {#each execResult.violations as v}
                  <li>{v}</li>
                {/each}
              </ul>
            </div>
          {:else}
            <p class="results-lead">
              {execResult.results.filter((r) => r.ok).length} of {execResult.results.length} ops succeeded.
            </p>
            <div class="op-results" data-testid="op-results-list">
              {#each execResult.results as r, i (i)}
                <div class="op-result" class:failed={!r.ok} data-testid="op-result-{i}">
                  <Icon name={r.ok ? "check" : "close"} size={12} />
                  <span class="op-result-path" title={displaySafePath(r.path)}>{displaySafePath(r.path)}</span>
                  {#if !r.ok}<span class="op-result-err">{r.error}</span>{/if}
                </div>
              {/each}
            </div>

            {#if execResult.checkpoint}
              <div class="undo-row">
                <span class="checkpoint-note" title={execResult.checkpoint.checkpoint.manifest_id}>
                  Checkpoint captured before these changes.
                </span>
                <button class="btn danger" data-testid="undo-btn" disabled={undoing || !!undoOutcome}
                  on:click={doUndo}>
                  {undoing ? "Undoing…" : "Undo"}
                </button>
              </div>
              {#if undoError}<div class="err" data-testid="undo-error">{undoError}</div>{/if}
              {#if undoOutcome}
                <div class="note" data-testid="undo-outcome">
                  <RevertOutcomePanel outcome={undoOutcome} testid="undo-outcome-panel" verb="Undo" {root} />
                </div>
              {/if}
            {/if}
          {/if}

          <div class="actions">
            <button class="btn" data-testid="start-over-btn" on:click={startOver}>New instruction</button>
          </div>
        </div>
      {/if}
    {/if}
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.25); display: grid; place-items: center; z-index: 200; }
  .dialog {
    width: 680px; max-width: 94vw; max-height: 86vh; overflow: auto;
    background: var(--surface); border: 1px solid var(--dialog-border); border-radius: 10px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25); padding: 16px 18px 18px;
  }
  .head-row { display: flex; align-items: center; gap: 8px; margin-bottom: 4px; }
  h2 { font-size: 16px; flex: 1; }
  .docs, .x { display: grid; place-items: center; height: 26px; width: 26px; padding: 0; border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-alt); color: var(--text); flex: 0 0 auto; }
  .scope { font-size: 12px; color: var(--text-dim); margin-bottom: 10px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .scope strong { color: var(--text); }
  .warn { font-size: 12.5px; color: var(--text-dim); line-height: 1.5; margin-bottom: 10px; }
  .setup { border: 1px solid var(--border); border-radius: var(--radius); padding: 14px; display: flex; flex-direction: column; gap: 6px; align-items: flex-start; }
  .setup-title { font-weight: 600; font-size: 14px; color: var(--text); }
  .dim { color: var(--text-faint); font-size: 12.5px; }
  .instruction {
    width: 100%; padding: 8px 10px; font: inherit; resize: vertical;
    border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-alt); color: var(--text);
    margin-bottom: 8px;
  }
  .instruction:disabled { opacity: 0.6; }
  .err { color: var(--danger); font-size: 12.5px; margin-bottom: 8px; }
  .note { color: var(--text-dim); font-size: 12px; margin: 4px 0 8px; }
  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 10px; }
  .btn { height: 30px; padding: 0 14px; border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-alt); color: var(--text); }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
  .btn.danger { border-color: var(--danger); color: var(--danger); }
  .btn:disabled { opacity: 0.5; }

  .preview { border: 1px solid var(--border); border-radius: var(--radius); padding: 12px; margin-top: 4px; }
  .preview-lead { font-size: 12.5px; color: var(--text-dim); margin-bottom: 8px; }
  .summary { display: flex; flex-wrap: wrap; gap: 12px; font-size: 12px; color: var(--text-dim); margin-bottom: 4px; }
  .summary span { white-space: nowrap; flex: 0 0 auto; }
  .violations { border: 1px solid var(--danger); border-radius: var(--radius); padding: 10px; background: color-mix(in srgb, var(--danger) 8%, var(--surface)); }
  .violations-title { font-weight: 600; font-size: 12.5px; color: var(--text); margin-bottom: 4px; }
  .violations ul { margin: 0 0 0 18px; padding: 0; font-size: 12.5px; color: var(--text); }

  /* CPE-1983 — one stable height (CPE-1968's decision, reused). CORRECTED IN ROUND 2, and the
     correction is the point of CPE-1933: round 1's comment here described "per-op checkboxes inside
     it" and an "Apply / Cancel row". Neither exists — there are no checkboxes in this component at
     all, and the buttons are `Edit instruction` / `Confirm — run this plan`. The FIX was right and
     the stated REASON was invented, which is exactly the failure mode a comment asserting facts
     about nearby code has.
     What it actually buys: `.op-list` lives inside `{#if planResult && …}`, so like the seven boxes
     this ticket allowlisted its ARRIVAL is a reflow that no height can remove. What a fixed height
     DOES remove is the arrival's dependence on the plan's SIZE — a 2-op plan and a 40-op plan now
     displace the instruction textarea above it by the same amount, so the distance the prompt box
     travels stops being a function of what the model happened to return. That is less than the ten
     other fixes buy and it is stated as less. At the 700px harness window `32vh` is 224px and binds;
     the floor engages below 500px, the cap above 812px.
     NO CENTRED PLACEHOLDER (unlike six of the sibling fixes): this box renders `{#each ops}` and
     nothing else, and it only exists when a plan does. */
  .op-list { height: clamp(160px, 32vh, 260px); overflow: auto; margin-top: 8px; border: 1px solid var(--border); border-radius: var(--radius); }
  .op-row { display: flex; align-items: center; gap: 8px; padding: 6px 8px; border-bottom: 1px solid var(--border); font-size: 12px; }
  .op-row:last-child { border-bottom: none; }
  .op-kind { flex: 0 0 auto; padding: 1px 8px; border-radius: 999px; font-size: 11px; font-weight: 600; white-space: nowrap; background: var(--surface-alt); color: var(--text); border: 1px solid var(--border-strong); }
  .op-from, .op-to { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-dim); }
  .op-from { flex: 1 1 auto; }
  .op-to { flex: 1 1 auto; color: var(--text); }
  .op-arrow { flex: 0 0 auto; color: var(--text-faint); }

  .results { border: 1px solid var(--border); border-radius: var(--radius); padding: 12px; margin-top: 10px; }
  .results-lead { font-size: 12.5px; color: var(--text); margin-bottom: 8px; }
  .op-results { max-height: 26vh; overflow: auto; }
  .op-result { display: flex; align-items: center; gap: 6px; padding: 4px 2px; font-size: 12px; color: var(--text-dim); }
  .op-result.failed { color: var(--danger); }
  .op-result-path { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1 1 auto; }
  .op-result-err { flex: 0 0 auto; color: var(--danger); font-size: 11px; }
  .undo-row { display: flex; align-items: center; justify-content: space-between; gap: 10px; margin-top: 10px; padding-top: 10px; border-top: 1px solid var(--border); }
  .checkpoint-note { font-size: 12px; color: var(--text-dim); }
</style>
