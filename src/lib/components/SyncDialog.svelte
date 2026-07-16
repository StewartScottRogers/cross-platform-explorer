<script lang="ts">
  // Two-way mirror sync dialog (CPE-495). A dry-run PREVIEW of the safe SyncPlan (from the CPE-438
  // planner via forge_repo_status) with a per-repo on-diverge policy, run on confirm. Self-contained:
  // it re-plans whenever the policy changes and runs each planned step via forge_sync. Never
  // force-pushes — a divergence under the "manual" policy surfaces for the user instead of reconciling.
  import { createEventDispatcher, onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { type OnDiverge, loadSyncPolicy, saveSyncPolicy, syncActionLabel } from "../syncPolicy";
  import { type AutoMirror, loadAutoMirror, saveAutoMirror, INTERVAL_CHOICES } from "../autoMirror";

  export let path: string;

  type Status = {
    is_repo?: boolean;
    branch?: string;
    upstream?: string;
    ahead?: number;
    behind?: number;
    dirty?: boolean;
    actions?: string[];
    up_to_date?: boolean;
    conflicts_possible?: boolean;
    blocked?: string | null;
    warnings?: string[];
  };

  const dispatch = createEventDispatcher<{ close: void; done: void }>();

  let policy: OnDiverge = loadSyncPolicy(path);
  let auto: AutoMirror = loadAutoMirror(path);

  function persistAuto() {
    saveAutoMirror(path, auto);
    auto = auto; // trigger reactivity
  }
  let status: Status | null = null;
  let planning = true;
  let running = false;
  let log: string[] = [];
  let failed = false;

  /** (Re)plan a dry run under the current policy — the preview reflects what "Run sync" would do. */
  async function replan() {
    planning = true;
    try {
      status = await invoke<Status>("forge_repo_status", { path, onDiverge: policy });
    } catch (e) {
      status = null;
      log = ["Could not read repo status: " + (e instanceof Error ? e.message : String(e))];
      failed = true;
    } finally {
      planning = false;
    }
  }

  onMount(replan);

  async function changePolicy(next: OnDiverge) {
    policy = next;
    saveSyncPolicy(path, next);
    await replan();
  }

  function onPolicyChange(e: Event) {
    changePolicy((e.currentTarget as HTMLSelectElement).value as OnDiverge);
  }

  $: steps = status?.actions ?? [];
  $: canRun = !planning && !running && steps.length > 0 && !status?.blocked;

  /** Run each planned step in order; stop + surface on the first failure (e.g. a merge conflict). */
  async function run() {
    if (!canRun) return;
    running = true;
    failed = false;
    log = [];
    for (const action of steps) {
      try {
        const out = await invoke<string>("forge_sync", { path, action });
        log = [...log, `✓ ${syncActionLabel(action)} — ${out}`];
      } catch (e) {
        log = [...log, `✗ ${syncActionLabel(action)} — ${e instanceof Error ? e.message : String(e)}`];
        failed = true;
        break; // a conflict/error halts the plan; the user resolves it before continuing
      }
    }
    running = false;
    dispatch("done"); // let the parent refresh its status bar + listing
    await replan(); // reflect the new post-sync state in the preview
  }
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && !running && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => !running && dispatch("close")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" on:click|stopPropagation>
    <h2>Sync {status?.branch ? `“${status.branch}”` : "repository"}</h2>

    <p class="track">
      {#if status?.upstream}Tracking <code>{status.upstream}</code>{:else}<span class="warn-text">No upstream branch — nothing to sync against.</span>{/if}
    </p>

    {#if status}
      <div class="state">
        {#if status.behind}<span class="chip">↓ {status.behind} behind</span>{/if}
        {#if status.ahead}<span class="chip">↑ {status.ahead} ahead</span>{/if}
        {#if status.dirty}<span class="chip dirty">● Uncommitted changes</span>{/if}
        {#if !status.behind && !status.ahead && !status.dirty}<span class="chip ok">✓ In sync</span>{/if}
      </div>
    {/if}

    <label class="policy">
      <span>On divergence</span>
      <select value={policy} on:change={onPolicyChange} disabled={running}>
        <option value="merge">Merge (default)</option>
        <option value="rebase">Rebase</option>
        <option value="manual">Manual — never auto-reconcile</option>
      </select>
    </label>

    <div class="preview">
      <div class="preview-head">Planned steps</div>
      {#if planning}
        <p class="muted">Planning…</p>
      {:else if status?.up_to_date}
        <p class="muted">Already up to date — nothing to do.</p>
      {:else if status?.blocked}
        <p class="warn-text">{status.blocked}</p>
      {:else if steps.length === 0}
        <p class="muted">No sync steps available.</p>
      {:else}
        <div class="steps">
          {#each steps as action}
            <span class="step">{syncActionLabel(action)}</span>
          {/each}
        </div>
      {/if}

      {#if status?.conflicts_possible}
        <p class="warn-text small">⚠ Histories have diverged — this may produce merge conflicts to resolve.</p>
      {/if}
      {#each status?.warnings ?? [] as w}
        <p class="warn-text small">⚠ {w}</p>
      {/each}
    </div>

    <div class="auto">
      <label class="auto-toggle">
        <input type="checkbox" bind:checked={auto.enabled} on:change={persistAuto} disabled={running} />
        <span>Auto-sync in the background</span>
      </label>
      {#if auto.enabled}
        <label class="auto-interval">
          every
          <select bind:value={auto.intervalMinutes} on:change={persistAuto} disabled={running}>
            {#each INTERVAL_CHOICES as m}
              <option value={m}>{m < 60 ? `${m} min` : `${m / 60} h`}</option>
            {/each}
          </select>
        </label>
      {/if}
    </div>
    {#if auto.enabled}
      <p class="auto-note">Runs a fast-forward pull + push on a timer and on window focus. A divergence
        or possible conflict <b>pauses</b> and surfaces here — never auto-reconciled, never force-pushed.</p>
    {/if}

    {#if log.length}
      <div class="log" class:failed>
        {#each log as line}<div class="log-line">{line}</div>{/each}
      </div>
    {/if}

    <div class="actions">
      <button class="btn" on:click={() => dispatch("close")} disabled={running}>Close</button>
      <button class="btn primary" on:click={run} disabled={!canRun}>
        {running ? "Syncing…" : "Run sync"}
      </button>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.25);
    display: grid;
    place-items: center;
    z-index: 200;
  }
  .dialog {
    width: 460px;
    max-width: 92vw;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: 10px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25);
    padding: 20px;
  }
  h2 { font-size: 16px; margin-bottom: 10px; }
  .track { color: var(--text-dim); font-size: 12px; margin-bottom: 12px; }
  .track code { font-size: 11px; }
  /* Status + step containers reflow: they wrap and grow the panel height rather than
     overflowing their pills (tick-tack rule). */
  .state { display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 14px; }
  .chip {
    white-space: nowrap; flex: 0 0 auto;
    font-size: 11px; padding: 2px 8px; border-radius: 999px;
    border: 1px solid var(--border-strong); background: var(--surface-alt);
    font-variant-numeric: tabular-nums;
  }
  .chip.dirty { color: #b5872b; }
  .chip.ok { color: var(--text-dim); }
  .policy { display: flex; align-items: center; justify-content: space-between; gap: 10px; margin-bottom: 14px; }
  .policy span { font-size: 12px; color: var(--text-dim); }
  .policy select {
    flex: 1 1 auto; max-width: 60%; height: 30px; padding: 0 8px;
    border: 1px solid var(--border-strong); border-radius: var(--radius);
    background: var(--surface-alt); color: inherit;
  }
  .preview { border-top: 1px solid var(--border); padding-top: 12px; margin-bottom: 12px; }
  .preview-head { font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-faint); margin-bottom: 8px; }
  .muted { color: var(--text-dim); font-size: 12px; }
  .steps { display: flex; flex-wrap: wrap; gap: 6px; }
  .step {
    white-space: nowrap; flex: 0 0 auto;
    font-size: 11px; padding: 3px 10px; border-radius: 6px;
    background: var(--surface-alt); border: 1px solid var(--border-strong);
  }
  .auto { display: flex; align-items: center; gap: 12px; flex-wrap: wrap;
    border-top: 1px solid var(--border); padding-top: 12px; margin-bottom: 8px; }
  .auto-toggle { display: flex; align-items: center; gap: 7px; font-size: 12px; }
  .auto-interval { display: flex; align-items: center; gap: 6px; font-size: 12px; color: var(--text-dim); }
  .auto-interval select { height: 28px; padding: 0 6px; border: 1px solid var(--border-strong);
    border-radius: var(--radius); background: var(--surface-alt); color: inherit; }
  .auto-note { font-size: 11px; color: var(--text-dim); line-height: 1.45; margin-bottom: 12px; }
  .warn-text { color: #b5872b; font-size: 12px; }
  .warn-text.small { font-size: 11px; margin-top: 6px; }
  .log {
    margin-bottom: 14px; max-height: 140px; overflow: auto;
    background: var(--surface-alt); border: 1px solid var(--border); border-radius: 6px;
    padding: 8px; font-size: 11px; font-family: var(--mono, monospace);
  }
  .log.failed { border-color: #b5872b; }
  .log-line { white-space: pre-wrap; word-break: break-word; }
  .actions { display: flex; justify-content: flex-end; gap: 8px; }
  .btn {
    height: 32px; padding: 0 16px;
    border: 1px solid var(--border-strong); border-radius: var(--radius);
    background: var(--surface-alt);
  }
  .btn:disabled { opacity: 0.5; cursor: default; }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
  .btn.primary:not(:disabled):hover { background: var(--accent-hover); }
</style>
