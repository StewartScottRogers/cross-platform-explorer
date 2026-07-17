<script lang="ts">
  // Integrated workbench (CPE-526, epic CPE-505) — read an agent's changes as a diff without leaving
  // the window. Loads `git diff` (working tree vs HEAD) for the current folder via the workbench_diff
  // command and renders it per-file with add/del/context styling. The editor + embedded browser panes
  // are wave 2 (CPE-527).
  import { createEventDispatcher, onMount } from "svelte";
  import { invoke } from "../invoke";
  import Icon from "./Icon.svelte";
  import { parseDiff, diffStats, fileStats, fileLabel, annotateInline, toPatch, type DiffFile } from "../diff";
  import { isBrowsableUrl, normalizeUrl, workbenchState } from "../workbench";

  /** Repo root to diff. */
  export let root: string;

  const dispatch = createEventDispatcher<{ close: void; browse: string; edit: string }>();

  interface WorkbenchDiff { is_repo: boolean; branch: string | null; diff: string }

  let files: DiffFile[] = [];
  let loading = true;
  let error = "";
  let isRepo = false;
  let branch: string | null = null;
  // Embedded-browser address (CPE-527), remembered across opens so your dev-server URL sticks (CPE-575).
  function loadUrl(): string {
    try { return localStorage.getItem("cpe.workbenchUrl") ?? ""; } catch { return ""; }
  }
  let url = loadUrl();
  $: { try { localStorage.setItem("cpe.workbenchUrl", url); } catch { /* ignore */ } }

  $: state = workbenchState({ loading, error, isRepo, fileCount: files.length });

  function openBrowser() {
    if (isBrowsableUrl(url)) dispatch("browse", normalizeUrl(url));
    else error = "Enter an http/https or localhost URL.";
  }
  function editFile(f: DiffFile) {
    const p = f.newPath && f.newPath !== "/dev/null" ? f.newPath : f.oldPath;
    if (p && p !== "/dev/null") dispatch("edit", root.replace(/[\\/]$/, "") + "/" + p);
  }

  $: stats = diffStats(files);

  // Collapsed files in the diff (CPE-568) — click a file header to fold its hunks away on big diffs.
  let collapsed = new Set<string>();
  function toggleCollapse(key: string) {
    if (collapsed.has(key)) collapsed.delete(key);
    else collapsed.add(key);
    collapsed = collapsed; // reassign so Svelte re-renders
  }
  const fileKey = (f: DiffFile) => f.oldPath + "→" + f.newPath;
  function collapseAll() { collapsed = new Set(files.map(fileKey)); }
  function expandAll() { collapsed = new Set(); }

  // Copy a single file's reconstructed diff to the clipboard, with a brief ✓ (CPE-572).
  let copiedFile: string | null = null;
  let copiedFileTimer: ReturnType<typeof setTimeout> | undefined;
  async function copyPatch(f: DiffFile) {
    try { await navigator.clipboard.writeText(toPatch(f)); } catch { /* clipboard unavailable — ignore */ }
    copiedFile = fileKey(f);
    clearTimeout(copiedFileTimer);
    copiedFileTimer = setTimeout(() => (copiedFile = null), 1100);
  }

  async function load() {
    loading = true;
    error = "";
    try {
      const r = await invoke<WorkbenchDiff>("workbench_diff", { root });
      isRepo = r.is_repo;
      branch = r.branch;
      files = r.is_repo ? parseDiff(r.diff) : [];
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
      files = [];
      isRepo = false;
      branch = null;
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
      <span class="wb-title">
        <Icon name="code" size={15} /> Workbench — Diff
        {#if state === "changes" || state === "clean"}<span class="wb-branch" title="Current branch">⎇ {branch || "detached"}</span>{/if}
      </span>
      <div class="wb-tools">
        {#if state === "changes"}
          <span class="wb-stat"><span class="add">+{stats.added}</span> <span class="del">−{stats.removed}</span> · {stats.files} file{stats.files === 1 ? "" : "s"}</span>
        {/if}
        {#if files.length > 1}
          <button class="wb-btn" on:click={collapseAll} title="Collapse every file">Collapse all</button>
          <button class="wb-btn" on:click={expandAll} title="Expand every file">Expand all</button>
        {/if}
        <button class="wb-btn" on:click={load} title="Re-run git diff">Refresh</button>
        <button class="wb-x" title="Close" aria-label="Close" on:click={() => dispatch("close")}>×</button>
      </div>
    </div>

    <div class="wb-address">
      <input
        class="wb-url"
        placeholder="localhost:3000 — open the running app in a browser window"
        bind:value={url}
        spellcheck="false"
        on:keydown={(e) => e.key === "Enter" && openBrowser()}
      />
      <button class="wb-btn" on:click={openBrowser} title="Open this URL in a browser window">Open in browser</button>
    </div>

    <div class="wb-body">
      {#if state === "loading"}
        <div class="wb-empty">Running git diff…</div>
      {:else if state === "no-folder"}
        <div class="wb-empty wb-edge"><div class="wb-edge-h">Open a folder first</div><p>Navigate to a project folder in the explorer, then reopen the Workbench to review its changes.</p></div>
      {:else if state === "git-missing"}
        <div class="wb-empty wb-edge"><div class="wb-edge-h">Git isn't available</div><p>The Workbench needs <code>git</code> on your PATH to read changes. Install Git, then try again.</p></div>
      {:else if state === "not-a-repo"}
        <div class="wb-empty wb-edge"><div class="wb-edge-h">Not a Git repository</div>
          <p><code>{root || "(no folder)"}</code> isn't a Git repo, so there are no changes to show. Open a repository folder (one with a <code>.git</code>), or clone one from <b>Repositories</b>.</p></div>
      {:else if state === "error"}
        <div class="wb-empty wb-edge error"><div class="wb-edge-h">Couldn't read the diff</div><p>{error}</p></div>
      {:else if state === "clean"}
        <div class="wb-empty">✓ No changes — <b>{branch || "the working tree"}</b> matches HEAD.</div>
      {:else}
        {#each files as f (f.oldPath + "→" + f.newPath)}
          {@const fs = fileStats(f)}
          {@const key = f.oldPath + "→" + f.newPath}
          {@const isCollapsed = collapsed.has(key)}
          <div class="file">
            <!-- svelte-ignore a11y-no-static-element-interactions a11y-click-events-have-key-events -->
            <div class="file-head" on:click={() => toggleCollapse(key)} title={isCollapsed ? "Expand" : "Collapse"}>
              <span class="chevron" aria-hidden="true">{isCollapsed ? "▸" : "▾"}</span>
              <span class="file-name">{fileLabel(f)}{#if f.binary} <span class="binary">binary</span>{/if}</span>
              {#if !f.binary}<span class="file-stat"><span class="fs-add">+{fs.added}</span> <span class="fs-del">−{fs.removed}</span></span>{/if}
              {#if !f.binary}<button class="edit-btn" on:click|stopPropagation={() => copyPatch(f)} title="Copy this file's diff">{copiedFile === key ? "✓ Copied" : "Copy"}</button>{/if}
              <button class="edit-btn" on:click|stopPropagation={() => editFile(f)} title="Open this file in the editor">Edit</button>
            </div>
            {#if !f.binary && !isCollapsed}
              {#each f.hunks as h}
                <div class="hunk-head">{h.header}</div>
                {#each annotateInline(h.lines) as l}
                  <div class="line {l.kind}"><span class="lno" aria-hidden="true">{l.oldLine ?? ""}</span><span class="lno" aria-hidden="true">{l.newLine ?? ""}</span><span class="gutter">{l.kind === "add" ? "+" : l.kind === "del" ? "−" : " "}</span><span class="code">{#if l.segs}{#each l.segs as s}{#if s.changed}<span class="chg">{s.text}</span>{:else}{s.text}{/if}{/each}{:else}{l.text}{/if}</span></div>
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

  .wb-branch { font-size: 12px; opacity: .7; margin-left: 6px; font-weight: 400; }
  .wb-empty { flex: 1; display: grid; place-items: center; color: var(--text-dim); text-align: center; }
  .wb-edge { align-content: center; max-width: 480px; margin: 0 auto; line-height: 1.5; }
  .wb-edge-h { font-size: 15px; color: var(--text); margin-bottom: 6px; }
  .wb-edge p { margin: 0; } .wb-edge code { font-size: 11px; }
  .wb-edge.error .wb-edge-h { color: #e0706b; }

  .wb-body { flex: 1; overflow: auto; padding: 10px 12px;
    font-family: var(--mono, ui-monospace, Consolas, monospace); font-size: 12px; }
  .file { border: 1px solid var(--border); border-radius: 6px; margin-bottom: 12px; overflow: hidden; }
  .file-head { display: flex; align-items: center; justify-content: space-between; gap: 8px;
    padding: 6px 10px; background: var(--surface-alt); border-bottom: 1px solid var(--border);
    font-family: system-ui, sans-serif; font-size: 12px; font-weight: 600; cursor: pointer; }
  .file-head:hover { background: rgba(128,128,128,0.1); }
  .chevron { flex: 0 0 auto; width: 12px; opacity: .6; font-size: 10px; }
  .file-name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .binary { font-weight: 400; opacity: .6; }
  /* Per-file add/remove badge (CPE-567). */
  .file-stat { flex: 0 0 auto; font-size: 11px; font-variant-numeric: tabular-nums; }
  .fs-add { color: #3a9d4a; }
  .fs-del { color: #c94f4f; }
  .edit-btn { flex: 0 0 auto; font: inherit; font-size: 11px; font-weight: 500; height: 22px; padding: 0 9px;
    border-radius: 5px; cursor: pointer; border: 1px solid var(--border-strong); background: var(--surface); color: var(--text); }
  .edit-btn:hover { background: rgba(128,128,128,0.14); }
  .wb-address { display: flex; gap: 8px; padding: 8px 14px; border-bottom: 1px solid var(--border); }
  .wb-url { flex: 1; height: 30px; padding: 0 10px; border: 1px solid var(--border); border-radius: 6px;
    background: var(--surface); color: var(--text); font: inherit; }
  .wb-url:focus { outline: none; border-color: var(--accent); }
  .hunk-head { padding: 3px 10px; color: #3a72b5; background: rgba(58,114,181,0.08); opacity: .9; }
  .line { display: flex; white-space: pre; }
  /* Old/new line-number gutter (CPE-566) — muted, fixed-width, not selectable so copying grabs code only. */
  .lno { flex: 0 0 auto; width: 40px; padding: 0 6px; text-align: right; opacity: .4; user-select: none;
    font-variant-numeric: tabular-nums; }
  .gutter { flex: 0 0 auto; width: 16px; text-align: center; opacity: .6; user-select: none; }
  .code { flex: 1; overflow-x: auto; }
  .line.add { background: rgba(58,157,74,0.14); }
  .line.del { background: rgba(201,79,79,0.14); }
  /* Intra-line highlight (CPE-570): the exact span that changed within a modified line. */
  .line.add .chg { background: rgba(58,157,74,0.4); border-radius: 2px; }
  .line.del .chg { background: rgba(201,79,79,0.4); border-radius: 2px; }
</style>
