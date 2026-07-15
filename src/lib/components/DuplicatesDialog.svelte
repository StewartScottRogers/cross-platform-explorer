<script lang="ts">
  /**
   * Find duplicate files (CPE-421) — the UI over the `find_duplicates` engine (CPE-420). Scans the
   * current folder, lists byte-identical groups (largest reclaimable space first) with the total
   * space that could be reclaimed, and jumps to a file's folder on click. Read-only + safe: it never
   * deletes anything — the user decides what to remove.
   */
  import { createEventDispatcher } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Icon from "./Icon.svelte";
  import { formatSize } from "../format";
  import { baseName, parentDir } from "../contentSearch";
  import { redundantPaths, keepsOnePerGroup, pruneGroups } from "../duplicates";

  export let root = "";

  const dispatch = createEventDispatcher<{ close: void; navigate: string }>();

  interface DupGroup { size: number; hash: string; paths: string[] }
  interface DupResult { groups: DupGroup[]; files_scanned: number; truncated: boolean }

  let loading = false;
  let error = "";
  let started = false;
  let result: DupResult = { groups: [], files_scanned: 0, truncated: false };

  // Paths the user has marked for removal (CPE-428). A Set, reassigned to trigger reactivity.
  let selected = new Set<string>();
  let deleting = false;
  $: canClean = selected.size > 0 && keepsOnePerGroup(result.groups, selected);

  function toggle(path: string) {
    if (selected.has(path)) selected.delete(path);
    else selected.add(path);
    selected = new Set(selected);
  }
  function selectRedundant() {
    selected = new Set(redundantPaths(result.groups));
  }
  async function cleanUp() {
    if (!canClean) return;
    const paths = [...selected];
    deleting = true;
    try {
      await invoke("delete_to_trash", { paths });
      result = { ...result, groups: pruneGroups(result.groups, selected) };
      selected = new Set();
    } catch (e) {
      error = String(e);
    } finally {
      deleting = false;
    }
  }

  // Reclaimable = for each group, the redundant copies × size (keep one copy per group).
  $: reclaimable = result.groups.reduce((n, g) => n + g.size * (g.paths.length - 1), 0);

  async function run() {
    loading = true;
    error = "";
    started = true;
    try {
      result = await invoke<DupResult>("find_duplicates", { root });
    } catch (e) {
      error = String(e);
      result = { groups: [], files_scanned: 0, truncated: false };
    } finally {
      loading = false;
    }
  }

  function goToFile(path: string) {
    // Dispatch the FILE path — the host reveals it (navigates to its folder AND selects it, CPE-423).
    dispatch("navigate", path);
    dispatch("close");
  }
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
<div class="backdrop" on:click={() => dispatch("close")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" on:click|stopPropagation>
    <header>
      <h2>Find duplicate files</h2>
      <span class="root" title={root}>{baseName(root) || root}</span>
      <button class="x" title="Close" on:click={() => dispatch("close")}><Icon name="close" size={14} /></button>
    </header>

    {#if !started}
      <div class="intro">
        <p>Scan this folder (and subfolders) for byte-identical files. You choose which copies to remove — they go to the Recycle Bin, and at least one copy of each set is always kept.</p>
        <button class="btn primary" on:click={run}>Scan for duplicates</button>
      </div>
    {:else if loading}
      <p class="dim">Scanning…</p>
    {:else if error}
      <p class="err">{error}</p>
    {:else if result.groups.length === 0}
      <p class="dim">No duplicate files found ({result.files_scanned.toLocaleString()} files scanned).</p>
    {:else}
      <div class="summary">
        <span>
          {result.groups.length} duplicate set{result.groups.length === 1 ? "" : "s"} ·
          {formatSize(reclaimable) || "0 B"} reclaimable
          {#if result.truncated}<span class="dim"> (scan capped)</span>{/if}
        </span>
        <span class="cleanup">
          <button class="mini" on:click={selectRedundant} title="Tick every copy except the first in each set">Select redundant</button>
          <button class="mini danger" disabled={!canClean || deleting} on:click={cleanUp}>
            {deleting ? "Removing…" : `Move ${selected.size} to Recycle Bin`}
          </button>
        </span>
      </div>
      <div class="results">
        {#each result.groups as g (g.hash)}
          <div class="group">
            <div class="ghead">
              <Icon name="copy" size={13} />
              {g.paths.length} copies · {formatSize(g.size) || "0 B"} each
              <span class="waste">{formatSize(g.size * (g.paths.length - 1)) || "0 B"} extra</span>
            </div>
            {#each g.paths as p (p)}
              <div class="row">
                <label class="pick" title="Mark this copy for the Recycle Bin">
                  <input type="checkbox" checked={selected.has(p)} on:change={() => toggle(p)} />
                </label>
                <button class="hit" title={p} on:click={() => goToFile(p)}>
                  <Icon name="file" size={12} /> <span class="name">{baseName(p)}</span>
                  <span class="loc">{parentDir(p)}</span>
                </button>
              </div>
            {/each}
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.25); display: grid; place-items: center; z-index: 200; }
  .dialog {
    width: 660px; max-width: 94vw; max-height: 82vh; display: flex; flex-direction: column;
    background: var(--surface); border: 1px solid var(--border-strong); border-radius: 10px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25); padding: 14px 16px 16px;
  }
  header { display: flex; align-items: center; gap: 8px; margin-bottom: 10px; }
  h2 { font-size: 16px; }
  .root { color: var(--text-dim); font-size: 12px; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .x { width: 28px; height: 28px; display: grid; place-items: center; }
  .intro { padding: 8px 0; display: grid; gap: 12px; }
  .intro p { color: var(--text-dim); font-size: 13px; }
  .btn { height: 32px; padding: 0 16px; border-radius: var(--radius); border: 1px solid var(--border-strong); background: var(--surface-alt); justify-self: start; }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
  .summary { font-size: 12px; color: var(--text-dim); margin-bottom: 6px; display: flex; align-items: center; gap: 10px; }
  .cleanup { margin-left: auto; display: flex; gap: 6px; flex: 0 0 auto; }
  .mini { height: 24px; padding: 0 10px; border-radius: var(--radius); border: 1px solid var(--border-strong); background: var(--surface-alt); font-size: 12px; }
  .mini:hover { background: var(--surface); }
  .mini.danger:not(:disabled) { border-color: #c42b1c; color: #c42b1c; }
  .mini:disabled { opacity: 0.5; }
  .row { display: flex; align-items: center; gap: 4px; padding-left: 16px; }
  .pick { display: inline-flex; align-items: center; }
  .row .hit { flex: 1; }
  .results { overflow: auto; }
  .group { margin-bottom: 10px; }
  .ghead { display: flex; align-items: center; gap: 6px; font-size: 12px; font-weight: 600; padding: 3px 6px; }
  .waste { margin-left: auto; color: var(--text-faint); font-weight: 400; }
  .hit { display: flex; align-items: center; gap: 6px; width: 100%; text-align: left; padding: 2px 6px 2px 22px; font-size: 12px; }
  .hit:hover { background: var(--surface-alt); }
  .name { flex: 0 0 auto; }
  .loc { color: var(--text-faint); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dim { color: var(--text-faint); }
  .err { color: #c42b1c; }
</style>
