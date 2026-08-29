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
  import { displaySafePath } from "../filename";

  interface OpResult { path: string; ok: boolean; error: string; }
  /** `firstError` (CPE-1879 review finding 3): the first refused entry's path + reason, so a refusal —
   *  including the link-guard refusal CPE-1879 added — reaches the screen instead of collapsing into a
   *  bare failure count. Optional: absent when `failed === 0`, or for history recorded before this
   *  field existed. */
  interface RunStatus { when: number; ok: number; failed: number; label: string; firstError?: { path: string; error: string }; }

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
      // CPE-1925: directory entries are counted like any other entry. A run whose whole job is to
      // recreate five empty folders used to show `0 / 0` and finish instantly, which reads as "there
      // was nothing to do" rather than "the folders were never in the plan".
      total = p.copy.length + p.update.length + p.delete.length + p.createDirs.length;
      // Stream per-file results so the row shows live progress instead of one blocking round-trip.
      const results: OpResult[] = [];
      const channel = createChannel<OpResult[]>();
      channel.onmessage = (batch) => {
        for (const r of batch) results.push(r);
        progress = results.length;
      };
      await rawInvoke("apply_backup_plan_stream", {
        sourceRoot: srcRoot, destRoot: dstRoot,
        copy: p.copy, update: p.update, deletePaths: p.delete, createDirs: p.createDirs, verify: true,
        // CPE-1664: the backend refuses the plan outright without this. `apply` is only reachable from
        // the Run / Restore buttons below, so the flag rides on a real click — a mirror plan deletes
        // files under the destination root with no Recycle Bin copy and no undo.
        confirmed: true,
        onResult: channel,
      });
      const failedResults = results.filter((r) => !r.ok);
      const failed = failedResults.length;
      // CPE-1879 review finding 3: surface the FIRST refusal's path + reason, not just the count — a
      // hard-link/symlink refusal (or any other per-file error) used to be reported only as an OpResult
      // that nothing ever rendered.
      const status: RunStatus = {
        when: Date.now(), ok: results.length - failed, failed, label: reverse ? "restore" : "backup",
        firstError: failedResults[0] ? { path: failedResults[0].path, error: failedResults[0].error } : undefined,
      };
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
            <span class="paths">{displaySafePath(job.source)} → {displaySafePath(job.dest)}</span>
            <label class="chk autorun" title="Run automatically when the destination drive connects">
              <input type="checkbox" data-testid="autorun-toggle" checked={!!job.autoRun}
                     on:change={(e) => toggleAutoRun(job.id, e.currentTarget.checked)} />
              auto-run on connect
            </label>
            {#if busyId === job.id}
              <span class="status running" data-testid="job-progress">running… {progress}{total ? ` / ${total}` : ""}</span>
            {:else if lastRun[job.id]}
              {@const st = lastRun[job.id]}
              <span class="status" data-testid="job-status" class:bad={st.failed > 0}>
                {st.label}: {st.ok} ok{st.failed ? `, ${st.failed} failed` : ""} · {fmtTime(st.when)}
              </span>
              <!-- CPE-1879 review finding 3: the first refusal's path + reason, not just the count — a
                   backup destination pointed at a dedup store, or one hit by a planted link, used to
                   report "N failed" with no way to see why or which file. -->
              {#if st.firstError}
                {@const fe = st.firstError}
                <span class="status-detail" data-testid="job-status-detail" title={displaySafePath(fe.error)}>
                  {displaySafePath(fe.path)}: {displaySafePath(fe.error)}
                </span>
              {/if}
            {/if}
            {#if (history[job.id]?.length ?? 0) > 0}
              <button class="hist-toggle" data-testid="history-toggle" on:click={() => (showHistory = showHistory === job.id ? "" : job.id)}>
                {history[job.id].length} run{history[job.id].length === 1 ? "" : "s"} {showHistory === job.id ? "▾" : "▸"}
              </button>
            {/if}
            {#if showHistory === job.id}
              <div class="history" data-testid="job-history">
                {#each history[job.id] as run (run.when)}
                  <div class="hist-row" class:bad={run.failed > 0}>
                    {run.label}: {run.ok} ok{run.failed ? `, ${run.failed} failed` : ""} · {fmtTime(run.when)}
                    {#if run.firstError}<span class="hist-detail" title={displaySafePath(run.firstError.error)}> — {displaySafePath(run.firstError.path)}: {displaySafePath(run.firstError.error)}</span>{/if}
                  </div>
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
        Dry-run: <b>{plan.copy.length}</b> copy · <b>{plan.update.length}</b> update · <b>{plan.delete.length}</b> delete · <b>{plan.createDirs.length}</b> new folders · {plan.unchanged} unchanged
      </div>
      <!-- CPE-1925: the plan's own disclosure of what it will NOT carry. A folder the scan could not
           read, or one the depth cap stopped at, is not created in the destination (its emptiness was
           never established) and nothing under it is mirror-deleted — and the user is told which, in
           the preview, before the run rather than after it.
           Deliberately literal, with the reason rendered as data rather than mapped to prose in the
           markup: every added render site here is one more entry in `bidiEscape.guard.test.ts`'s
           REGISTRY ratchet, so the wording that could have lived in three ternaries lives in the
           surrounding static text instead. -->
      {#if plan.skippedDirs.length > 0}
        <div class="plan skipped" data-testid="plan-skipped">
          Folders not carried — this backup cannot see inside them: <b>{plan.skippedDirs.length}</b>
          {#each plan.skippedDirs as sd (sd.path)}
            <span class="skipped-dir">{displaySafePath(sd.path)} ({displaySafePath(sd.reason)})</span>
          {/each}
        </div>
      {/if}
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
  .dialog { width: 720px; max-width: 96vw; background: var(--surface); border: 1px solid var(--dialog-border); border-radius: 10px; box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25); padding: 20px; }
  h2 { font-size: 16px; margin-bottom: 12px; }
  /* CPE-1983 — one stable height (CPE-1968's decision, reused). Job rows carry their own per-row
     controls (the auto-run checkbox, Run, Delete) and each row's status/plan text grows as a job
     runs; under a centred dialog every one of those growths moved the rows above. At the 700px
     harness window `40vh` is 280px and binds; the floor engages below 400px, the cap above 800px. */
  .jobs { height: clamp(160px, 40vh, 320px); overflow-y: auto; display: flex; flex-direction: column; gap: 6px; }
  /* CPE-1983 — centred, and only because `.jobs` is now a fixed height (the same second half of the
     fix `OrganizeDialog`'s `.empty` carries): a one-line placeholder pinned to the corner of a 280px
     box reads as a failed render rather than an empty one. Safe because `.empty` renders only under
     `{#if list.length === 0}` and is therefore never mounted alongside a `.job`, so this cannot
     centre a list; `height: 100%` resolves against `.jobs`'s definite height. */
  .empty { color: var(--text-dim); font-size: 12.5px; padding: 8px 2px; display: grid; place-items: center; height: 100%; }
  .job { display: flex; align-items: center; gap: 10px; padding: 8px; border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface-alt); }
  .jinfo { flex: 1 1 auto; min-width: 0; display: flex; flex-wrap: wrap; align-items: baseline; gap: 6px 10px; }
  .jname { font-weight: 600; }
  .mirror { font-size: 10px; text-transform: uppercase; letter-spacing: 0.03em; padding: 0 6px; border-radius: 999px; background: var(--accent); color: #fff; }
  .paths { font-size: 11.5px; color: var(--text-dim); font-family: ui-monospace, monospace; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .status { font-size: 11.5px; color: #2e9e4f; }
  .status.bad { color: var(--danger); }
  .status.running { color: var(--accent-text); }
  /* CPE-1925 skipped-folder disclosure. A row of pills, so it reflows onto more rows and grows its
     height while each pill keeps its path on one line (the tick-tack rule). */
  .plan.skipped { display: flex; flex-wrap: wrap; align-items: baseline; gap: 4px 8px; color: var(--text-dim); }
  .skipped-dir { flex: 0 0 auto; white-space: nowrap; max-width: 100%; overflow: hidden; text-overflow: ellipsis; font-family: ui-monospace, monospace; font-size: 11px; padding: 0 6px; border-radius: 999px; background: var(--surface-alt); border: 1px solid var(--border); }
  /* CPE-1879 review finding 3: the first refusal's path + reason, own line so it never crowds the
     status pill; truncated with the full text in the native tooltip (`title`) rather than wrapped. */
  .status-detail { flex-basis: 100%; font-size: 11px; color: var(--danger); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .hist-toggle { font-size: 11px; padding: 0 6px; border: 1px solid var(--border); border-radius: 999px; background: var(--surface); color: var(--text-dim); }
  .history { flex-basis: 100%; margin-top: 4px; padding-left: 4px; }
  .hist-row { font-size: 11px; color: var(--text-dim); font-variant-numeric: tabular-nums; padding: 1px 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .hist-row.bad { color: var(--danger); }
  .hist-detail { color: var(--danger); }
  .jbtns { flex: 0 0 auto; display: flex; gap: 6px; }
  .plan { margin-top: 10px; padding: 8px 10px; border: 1px solid var(--border); border-radius: var(--radius); font-size: 12.5px; background: var(--surface-alt); }
  .err { margin-top: 10px; padding: 8px 10px; color: var(--danger); font-size: 12.5px; }
  .builder { display: flex; align-items: center; gap: 8px; margin-top: 14px; flex-wrap: wrap; }
  .builder .grow { flex: 1 1 130px; }
  input:not([type=checkbox]) { height: 30px; padding: 0 8px; font: inherit; color: var(--text); background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); min-width: 0; }
  .chk { font-size: 12px; color: var(--text-dim); }
  /* CPE-1632 darkened this fill to clear WCAG's 3:1 white-on-fill UI floor (3.70:1) — see
     src/app.css.solid-fill-contrast.test.ts. CPE-1821 then made --accent-2 (which CPE-1632 already
     paired this with) a real theme token instead of an undefined var()'s hex fallback — see
     src/app.css's --pal-accent2-fill comment for why one value serves every theme. */
  .mirror.auto { background: var(--accent-2); }
  .autorun { display: inline-flex; align-items: center; gap: 4px; }
  .mini { width: 24px; height: 24px; border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); color: var(--text); }
  .actions { display: flex; justify-content: flex-end; margin-top: 16px; }
  .btn { height: 28px; padding: 0 12px; border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-alt); color: var(--text); font-size: 12px; }
  .btn:disabled { opacity: 0.4; }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
</style>
