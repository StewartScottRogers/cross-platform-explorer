<script lang="ts">
  /**
   * Integrity report view (CPE-792, epic CPE-737). Baseline a folder's checksums (backend
   * `checksum_folder`), then verify a fresh scan against it (`verifyManifest`, CPE-790) — surfacing
   * silent corruption (bitrot) and missing files prominently, edits/new files secondarily. Rebaseline
   * accepts the current state as the new baseline. A thin render over the tested integrity model; the
   * per-folder baselines persist via settings (App owns the store).
   */
  import { createEventDispatcher } from "svelte";
  import { invoke } from "../invoke";
  import { hasIssues, type ChecksumEntry, type IntegrityReport } from "../integrity";

  /** Folder to check (pre-filled with the current folder by App). */
  export let initialPath = "";
  /** All stored baselines keyed by folder path. App supplies + persists; the entry for the current
      `path` is used, so switching folders in the field updates what Verify compares against. */
  export let baselines: Record<string, ChecksumEntry[]> = {};

  const dispatch = createEventDispatcher<{ baseline: { path: string; entries: ChecksumEntry[] }; cancel: void }>();

  let path = initialPath;
  let report: IntegrityReport | null = null;
  let loading = false;
  let error = "";
  let note = "";

  $: baseline = baselines[path.trim()] ?? [];
  $: hasBaseline = baseline.length > 0;

  async function scan(): Promise<ChecksumEntry[]> {
    return invoke<ChecksumEntry[]>("checksum_folder", { path: path.trim() });
  }

  async function doBaseline() {
    if (!path.trim()) return;
    loading = true; error = ""; note = "";
    try {
      const entries = await scan();
      dispatch("baseline", { path: path.trim(), entries });
      report = null;
      note = `Baselined ${entries.length} file${entries.length === 1 ? "" : "s"}.`;
    } catch (e) { error = String(e); } finally { loading = false; }
  }

  async function doVerify() {
    if (!path.trim()) return;
    loading = true; error = ""; note = "";
    try {
      // Diff in the backend (CPE-870): it re-scans + classifies and returns only the compact report, so a
      // large folder never ships its whole manifest across the IPC boundary just to be diffed here.
      report = await invoke<IntegrityReport>("verify_folder", { path: path.trim(), baseline });
      if (!hasBaseline) note = "No baseline yet — everything shows as new. Baseline first.";
    } catch (e) { error = String(e); } finally { loading = false; }
  }

  async function rebaseline() {
    // Accept the current state: re-scan and store as the new baseline, clearing the report.
    await doBaseline();
  }

  const base = (p: string) => p.split(/[\\/]/).pop() || p;
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("cancel")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("cancel")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Integrity report" on:click|stopPropagation>
    <h2>Integrity check</h2>
    <p>Baseline a folder's checksums, then verify later. A hash change with an <em>unchanged</em> timestamp is
       flagged as silent corruption; a normal edit moves both.</p>

    <div class="paths">
      <input class="path" placeholder="Folder to check…" bind:value={path} aria-label="Folder path" />
      <button class="btn" data-testid="baseline-btn" disabled={loading || !path.trim()} on:click={doBaseline}>Baseline</button>
      <button class="btn primary" data-testid="verify-btn" disabled={loading || !path.trim()} on:click={doVerify}>Verify</button>
    </div>
    <div class="status-line">
      <span data-testid="baseline-state">{hasBaseline ? `Baseline: ${baseline.length} files` : "No baseline stored"}</span>
      {#if note}<span class="note" data-testid="note">{note}</span>{/if}
    </div>

    <div class="report" data-testid="report">
      {#if error}
        <div class="err">{error}</div>
      {:else if loading}
        <div class="empty">Scanning…</div>
      {:else if !report}
        <div class="empty">Baseline the folder, then Verify to see changes.</div>
      {:else}
        <div class="counts" data-testid="counts" class:alarm={hasIssues(report)}>
          <span class="c-corrupted">corrupted {report.corrupted.length}</span>
          <span class="c-missing">missing {report.missing.length}</span>
          <span class="c-edited">edited {report.edited.length}</span>
          <span class="c-new">new {report.new.length}</span>
          <span class="c-intact">intact {report.intact.length}</span>
        </div>
        <div class="groups">
          {#each [["corrupted", report.corrupted], ["missing", report.missing], ["edited", report.edited], ["new", report.new]] as [label, list]}
            {#if list.length}
              <div class="group group-{label}" data-testid="group-{label}">
                <div class="group-head">{label} ({list.length})</div>
                {#each list as p (p)}
                  <div class="item" title={p}>{base(p)}</div>
                {/each}
              </div>
            {/if}
          {/each}
          {#if !hasIssues(report) && report.edited.length === 0 && report.new.length === 0}
            <div class="all-ok" data-testid="all-ok">✓ All {report.intact.length} files intact.</div>
          {/if}
        </div>
      {/if}
    </div>

    <div class="actions">
      <button class="btn" data-testid="rebaseline-btn" disabled={loading || !path.trim()} on:click={rebaseline}>Rebaseline (accept current)</button>
      <button class="btn primary" on:click={() => dispatch("cancel")}>Close</button>
    </div>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.25); display: grid; place-items: center; z-index: 200; }
  .dialog { width: 640px; max-width: 95vw; background: var(--surface); border: 1px solid var(--border-strong); border-radius: 10px; box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25); padding: 20px; }
  h2 { font-size: 16px; margin-bottom: 8px; }
  p { color: var(--text-dim); font-size: 12.5px; margin-bottom: 12px; line-height: 1.5; }
  .paths { display: flex; gap: 8px; }
  .path { flex: 1 1 auto; height: 30px; padding: 0 8px; font: inherit; color: var(--text); background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); min-width: 0; }
  .status-line { display: flex; gap: 12px; margin: 8px 2px; font-size: 12px; color: var(--text-dim); }
  .note { color: var(--accent); }
  .report { height: 44vh; overflow: auto; border: 1px solid var(--border); border-radius: var(--radius); margin-top: 6px; }
  .counts { display: flex; flex-wrap: wrap; gap: 12px; padding: 8px 10px; font-size: 12px; border-bottom: 1px solid var(--border); position: sticky; top: 0; background: var(--surface); }
  .counts.alarm { background: color-mix(in srgb, #c0392b 12%, var(--surface)); }
  .c-corrupted { color: #c0392b; font-weight: 600; }
  .c-missing { color: #c0392b; }
  .c-edited { color: #b8860b; }
  .c-new { color: #2e9e4f; }
  .c-intact { color: var(--text-dim); }
  .group { padding: 4px 0; }
  .group-head { padding: 4px 10px; font-size: 11px; text-transform: uppercase; letter-spacing: 0.03em; color: var(--text-dim); }
  .group-corrupted .group-head, .group-missing .group-head { color: #c0392b; }
  .item { padding: 2px 10px 2px 20px; font-size: 12.5px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .all-ok { padding: 14px; color: #2e9e4f; font-size: 13px; }
  .empty, .err { padding: 12px; color: var(--text-dim); font-size: 12.5px; }
  .err { color: #c0392b; }
  .actions { display: flex; justify-content: space-between; align-items: center; margin-top: 14px; }
  .btn { height: 30px; padding: 0 14px; border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-alt); color: var(--text); }
  .btn:disabled { opacity: 0.4; }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
</style>
