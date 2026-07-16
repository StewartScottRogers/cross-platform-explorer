<script lang="ts">
  // Integrated workbench (CPE-526, epic CPE-505) — read an agent's changes as a diff without leaving
  // the window. Loads `git diff` (working tree vs HEAD) for the current folder via the workbench_diff
  // command and renders it per-file with add/del/context styling. The editor + embedded browser panes
  // are wave 2 (CPE-527).
  import { createEventDispatcher, onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Icon from "./Icon.svelte";
  import { parseDiff, diffStats, fileLabel, type DiffFile } from "../diff";

  /** Repo root to diff. */
  export let root: string;

  const dispatch = createEventDispatcher<{ close: void }>();

  let files: DiffFile[] = [];
  let loading = true;
  let error = "";

  $: stats = diffStats(files);

  async function load() {
    loading = true;
    error = "";
    try {
      const text = await invoke<string>("workbench_diff", { root });
      files = parseDiff(text);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      files = [];
    } finally {
      loading = false;
    }
  }
  onMount(load);
</script>

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="wb-overlay" on:click|self={() => dispatch("close")}>
  <div class="wb-panel">
    <div class="wb-titlebar">
      <span class="wb-title"><Icon name="code" size={15} /> Workbench — Diff</span>
      <div class="wb-tools">
        {#if !loading && !error}
          <span class="wb-stat"><span class="add">+{stats.added}</span> <span class="del">−{stats.removed}</span> · {stats.files} file{stats.files === 1 ? "" : "s"}</span>
        {/if}
        <button class="wb-btn" on:click={load} title="Re-run git diff">Refresh</button>
        <button class="wb-x" title="Close" aria-label="Close" on:click={() => dispatch("close")}>×</button>
      </div>
    </div>

    {#if error}<div class="wb-status error">{error}</div>{/if}

    <div class="wb-body">
      {#if loading}
        <div class="wb-empty">Running git diff…</div>
      {:else if files.length === 0}
        <div class="wb-empty">No changes — the working tree matches HEAD.</div>
      {:else}
        {#each files as f (f.oldPath + "→" + f.newPath)}
          <div class="file">
            <div class="file-head">{fileLabel(f)}{#if f.binary} <span class="binary">binary</span>{/if}</div>
            {#if !f.binary}
              {#each f.hunks as h}
                <div class="hunk-head">{h.header}</div>
                {#each h.lines as l}
                  <div class="line {l.kind}"><span class="gutter">{l.kind === "add" ? "+" : l.kind === "del" ? "−" : " "}</span><span class="code">{l.text}</span></div>
                {/each}
              {/each}
            {/if}
          </div>
        {/each}
      {/if}
    </div>
  </div>
</div>

<style>
  .wb-overlay { position: fixed; inset: 0; background: rgba(0,0,0,0.45); display: flex;
    align-items: center; justify-content: center; z-index: 60; }
  .wb-panel { width: min(1000px, 96vw); height: min(760px, 92vh); display: flex; flex-direction: column;
    background: var(--surface); color: var(--text); border: 1px solid var(--border-strong);
    border-radius: 8px; box-shadow: 0 16px 48px rgba(0,0,0,0.4); overflow: hidden; }

  .wb-titlebar { display: flex; align-items: center; justify-content: space-between;
    padding: 10px 14px; border-bottom: 1px solid var(--border); }
  .wb-title { display: flex; align-items: center; gap: 8px; font-weight: 600; }
  .wb-tools { display: flex; align-items: center; gap: 10px; }
  .wb-stat { font-size: 12px; font-variant-numeric: tabular-nums; opacity: .85; }
  .wb-stat .add { color: #3a9d4a; } .wb-stat .del { color: #c94f4f; }
  .wb-btn { font: inherit; font-size: 12px; height: 28px; padding: 0 12px; border-radius: 6px; cursor: pointer;
    border: 1px solid var(--border-strong); background: var(--surface); color: var(--text); }
  .wb-btn:hover { background: rgba(128,128,128,0.14); }
  .wb-x { border: 0; background: transparent; color: var(--text-dim); font-size: 20px; cursor: pointer;
    line-height: 1; padding: 0 4px; border-radius: 4px; }
  .wb-x:hover { background: rgba(128,128,128,0.18); color: var(--text); }

  .wb-status { padding: 6px 14px; font-size: 12px; border-bottom: 1px solid var(--border); }
  .wb-status.error { color: #e0706b; }
  .wb-empty { flex: 1; display: grid; place-items: center; color: var(--text-dim); }

  .wb-body { flex: 1; overflow: auto; padding: 10px 12px;
    font-family: var(--mono, ui-monospace, Consolas, monospace); font-size: 12px; }
  .file { border: 1px solid var(--border); border-radius: 6px; margin-bottom: 12px; overflow: hidden; }
  .file-head { padding: 6px 10px; background: var(--surface-alt); border-bottom: 1px solid var(--border);
    font-family: system-ui, sans-serif; font-size: 12px; font-weight: 600; }
  .binary { font-weight: 400; opacity: .6; }
  .hunk-head { padding: 3px 10px; color: #3a72b5; background: rgba(58,114,181,0.08); opacity: .9; }
  .line { display: flex; white-space: pre; }
  .gutter { flex: 0 0 auto; width: 16px; text-align: center; opacity: .6; user-select: none; }
  .code { flex: 1; overflow-x: auto; }
  .line.add { background: rgba(58,157,74,0.14); }
  .line.del { background: rgba(201,79,79,0.14); }
</style>
