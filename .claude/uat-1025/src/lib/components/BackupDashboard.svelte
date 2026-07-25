<script lang="ts">
  /**
   * Backup jobs dashboard (CPE-798, epic CPE-736). Define source→dest jobs, **dry-run** a plan
   * (`planBackup`, CPE-796, over two `scan_tree` scans), **run** it (`apply_backup_plan`, CPE-797, with
   * checksum verify) showing per-run status, and **one-click restore** (the reverse copy). A thin render
   * over the tested planner + the copy-engine backend; jobs persist via settings (App owns the store).
   */
  import { createEventDispatcher } from "svelte";
  import { rawInvoke, createChannel, unwrap } from "../invoke";
  import { commands } from "../bindings.gen"; // typed client (CPE-964)
  import { addJob, removeJob, updateJob, planBackup, type BackupJob, type BackupPlan } from "../backup";
  import type { CompareNode } from "../treeDiff";

  interface OpResult { path: string; ok: boolean; error: string; }
  interface RunStatus { when: number; ok: number; failed: number; label: string; }

  export let jobs: BackupJob[] = [];
  /** Per-job run history (CPE-798), newest first — App owns + persists it. */
  export let history: Record<string, RunStatus[]> = {};

  const dispatch = createEventDispatcher<{
    change: BackupJob[];
    run: { jobId: string; status: RunStatus };
    cancel: void;
  }>();

  let showHistory = "";

  let list: BackupJob[] = jobs.map((j) => ({ ...j }));
  let name = "";
  let source = "";
  let dest = "";
  let mirror = false;

  let busyId = "";
  let plan: (BackupPlan & { jobId: string }) | null = null;
  let error = "";
  // Live-progress counters for the running job (CPE-798): files completed / total planned.
  let progress = 0;
  let total = 0;
  const lastRun: Record<string, RunStatus> = {};

  function persist() {
    dispatch("change", list);
  }

  function add() {
    if (!name.trim() || !source.trim() || !dest.trim()) return;
    list = addJob(list, name.trim(), source.trim(), dest.trim(), mirror);
    name = source = dest = ""; mirror = false;
    persist();
  }
  function del(id: string) {
    list = removeJob(list, id);
    if (plan?.jobId === id) plan = null;
    persist();
  }
  function toggleAutoRun(id: string, on: boolean) {
    list = updateJob(list, id, { autoRun: on });
    persist();
  }

  async function scan(path: string): Promise<CompareNode[]> {
    return commands.scanTree(path, 32).then(unwrap) as Promise<CompareNode[]>;
  }

  async function computePlan(job: BackupJob, reverse = false): Promise<BackupPlan> {
    const src = reverse ? job.dest : job.source;
    const dst = reverse ? job.source : job.dest;
    const [s, d] = await Promise.all([scan(src), scan(dst)]);
    return planBackup(s, d, job.mirror);
  }

  async function dryRun(job: BackupJob) {
    busyId = job.id; error = ""; plan = null;
    try {
      plan = { ...(await computePlan(job)), jobId: job.id };
    } catch (e) { error = String(e); } finally { busyId = ""; }
  }

  async function apply(job: BackupJob, reverse: boolean) {
    busyId = job.id; error = ""; plan = null; progress = 0; total = 0;
    const srcRoot = reverse ? job.dest : job.source;
    const dstRoot = reverse ? job.source : job.dest;
    try {
      const p = await computePlan(job, reverse);
      total = p.copy.length + p.update.length + p.delete.length;
      // Stream per-file results so the row shows live progress instead of one blocking round-trip.
      const results: OpResult[] = [];
      const channel = createChannel<OpResult[]>();
      channel.onmessage = (batch) => {
        for (const r of batch) results.push(r);
        progress = results.length;
      };
      await rawInvoke("apply_backup_plan_stream", {
        sourceRoot: srcRoot, destRoot: dstRoot,
        copy: p.copy, update: p.update, deletePaths: p.delete, verify: true,
        onResult: channel,
      });
      const failed = results.filter((r) => !r.ok).length;
      const status = { when: Date.now(), ok: results.length - failed, failed, label: reverse ? "restore" : "backup" };
      lastRun[job.id] = status;
      dispatch("run", { jobId: job.id, status });
    } catch (e) { error = String(e); } finally { busyId = ""; progress = 0; total = 0; }
  }

  const fmtTime = (ms: number) => new Date(ms).toLocaleTimeString();
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("cancel")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("cancel")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Backup jobs" on:click|stopPropagation>
    <h2>Backup jobs</h2>

    <div class="jobs" data-testid="job-list">
      {#if list.length === 0}<div class="empty">No backup jobs yet.</div>{/if}
      {#each list as job (job.id)}
        <div class="job" data-testid="job-row">
          <div class="jinfo">
            <span class="jname">{job.name}</span>
            {#if job.mirror}<span class="mirror">mirror</span>{/if}
            {#if job.autoRun}<span class="mirror auto" data-testid="auto-pill">auto</span>{/if}
            <span class="paths">{job.source} → {job.dest}</span>
            <label class="chk autorun" title="Run automatically when the destination drive connects">
              <input type="checkbox" data-testid="autorun-toggle" checked={!!job.autoRun}
                     on:change={(e) => toggleAutoRun(job.id, e.currentTarget.checked)} />
              auto-run on connect
            </label>
            {#if busyId === job.id}
              <span class="status running" data-testid="job-progress">running… {progress}{total ? ` / ${total}` : ""}</span>
            {:else if lastRun[job.id]}
              <span class="status" data-testid="job-status" class:bad={lastRun[job.id].failed > 0}>
                {lastRun[job.id].label}: {lastRun[job.id].ok} ok{lastRun[job.id].failed ? `, ${lastRun[job.id].failed} failed` : ""} · {fmtTime(lastRun[job.id].when)}
              </span>
            {/if}
            {#if (history[job.id]?.length ?? 0) > 0}
              <button class="hist-toggle" data-testid="history-toggle" on:click={() => (showHistory = showHistory === job.id ? "" : job.id)}>
                {history[job.id].length} run{history[job.id].length === 1 ? "" : "s"} {showHistory === job.id ? "▾" : "▸"}
              </button>
            {/if}
            {#if showHistory === job.id}
              <div class="history" data-testid="job-history">
                {#each history[job.id] as run (run.when)}
                  <div class="hist-row" class:bad={run.failed > 0}>{run.label}: {run.ok} ok{run.failed ? `, ${run.failed} failed` : ""} · {fmtTime(run.when)}</div>
                {/each}
              </div>
            {/if}
          </div>
          <div class="jbtns">
            <button class="btn" data-testid="dryrun-btn" disabled={busyId === job.id} on:click={() => dryRun(job)}>Dry-run</button>
            <button class="btn primary" data-testid="run-btn" disabled={busyId === job.id} on:click={() => apply(job, false)}>Run</button>
            <button class="btn" data-testid="restore-btn" disabled={busyId === job.id} on:click={() => apply(job, true)}>Restore</button>
            <button class="mini danger" aria-label="Delete job" on:click={() => del(job.id)}>✕</button>
          </div>
        </div>
      {/each}
    </div>

    {#if error}
      <div class="err" data-testid="backup-error">{error}</div>
    {/if}
    {#if plan}
      <div class="plan" data-testid="plan-summary">
        Dry-run: <b>{plan.copy.length}</b> copy · <b>{plan.update.length}</b> update · <b>{plan.delete.length}</b> delete · {plan.unchanged} unchanged
      </div>
    {/if}

    <div class="builder" data-testid="add-job">
      <input class="grow" placeholder="Job name" bind:value={name} aria-label="Job name" />
      <input class="grow" placeholder="Source folder" bind:value={source} aria-label="Source folder" />
      <input class="grow" placeholder="Dest folder" bind:value={dest} aria-label="Dest folder" />
      <label class="chk"><input type="checkbox" bind:checked={mirror} /> mirror</label>
      <button class="btn primary" data-testid="add-job-btn" disabled={!name.trim() || !source.trim() || !dest.trim()} on:click={add}>Add</button>
    </div>

    <div class="actions">
      <button class="btn primary" on:click={() => dispatch('cancel')}>Close</button>
    </div>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.25); display: grid; place-items: center; z-index: 200; }
  .dialog { width: 720px; max-width: 96vw; background: var(--surface); border: 1px solid var(--border-strong); border-radius: 10px; box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25); padding: 20px; }
  h2 { font-size: 16px; margin-bottom: 12px; }
  .jobs { max-height: 40vh; overflow-y: auto; display: flex; flex-direction: column; gap: 6px; }
  .empty { color: var(--text-dim); font-size: 12.5px; padding: 8px 2px; }
  .job { display: flex; align-items: center; gap: 10px; padding: 8px; border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface-alt); }
  .jinfo { flex: 1 1 auto; min-width: 0; display: flex; flex-wrap: wrap; align-items: baseline; gap: 6px 10px; }
  .jname { font-weight: 600; }
  .mirror { font-size: 10px; text-transform: uppercase; letter-spacing: 0.03em; padding: 0 6px; border-radius: 999px; background: var(--accent); color: #fff; }
  .paths { font-size: 11.5px; color: var(--text-dim); font-family: ui-monospace, monospace; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .status { font-size: 11.5px; color: #2e9e4f; }
  .status.bad { color: #c0392b; }
  .status.running { color: var(--accent); }
  .hist-toggle { font-size: 11px; padding: 0 6px; border: 1px solid var(--border); border-radius: 999px; background: var(--surface); color: var(--text-dim); }
  .history { flex-basis: 100%; margin-top: 4px; padding-left: 4px; }
  .hist-row { font-size: 11px; color: var(--text-dim); font-variant-numeric: tabular-nums; padding: 1px 0; }
  .hist-row.bad { color: #c0392b; }
  .jbtns { flex: 0 0 auto; display: flex; gap: 6px; }
  .plan { margin-top: 10px; padding: 8px 10px; border: 1px solid var(--border); border-radius: var(--radius); font-size: 12.5px; background: var(--surface-alt); }
  .err { margin-top: 10px; padding: 8px 10px; color: #c0392b; font-size: 12.5px; }
  .builder { display: flex; align-items: center; gap: 8px; margin-top: 14px; flex-wrap: wrap; }
  .builder .grow { flex: 1 1 130px; }
  input:not([type=checkbox]) { height: 30px; padding: 0 8px; font: inherit; color: var(--text); background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); min-width: 0; }
  .chk { font-size: 12px; color: var(--text-dim); }
  .mirror.auto { background: var(--accent-2, #2a7); }
  .autorun { display: inline-flex; align-items: center; gap: 4px; }
  .mini { width: 24px; height: 24px; border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); color: var(--text); }
  .actions { display: flex; justify-content: flex-end; margin-top: 16px; }
  .btn { height: 28px; padding: 0 12px; border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-alt); color: var(--text); font-size: 12px; }
  .btn:disabled { opacity: 0.4; }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
</style>
