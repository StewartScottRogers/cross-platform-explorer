<script lang="ts">
  /**
   * Compare view (CPE-779, epic CPE-722). Two folders → a classified tree (`scan_tree` + `diffTrees` +
   * `summarizeDiff`/`flattenDiff`, with expand/collapse). Two files → a **byte compare** (`byteDiff`,
   * CPE-778, over `read_file_range`): equal / first-differing offset / differing-range count / length diff.
   * The mode is auto-detected — folders scan, files don't — so the same two path fields drive both. Text
   * line-diff is a follow-up (needs a line-level diff algorithm the codebase doesn't yet have).
   */
  import { createEventDispatcher } from "svelte";
  import { invoke } from "../invoke";
  import { diffTrees, summarizeDiff, flattenDiff, type CompareNode, type DiffRow } from "../treeDiff";
  import { byteDiff, type ByteDiff } from "../byteDiff";

  export let initialLeft = "";
  export let initialRight = "";

  const dispatch = createEventDispatcher<{ cancel: void }>();

  /** Max bytes read from each file for the byte compare (keeps a huge pair bounded). */
  const FILE_CMP_CAP = 4 * 1024 * 1024;

  let left = initialLeft;
  let right = initialRight;
  let rows: DiffRow[] = [];
  let summary = { added: 0, removed: 0, changed: 0, identical: 0 };
  let collapsed = new Set<string>();
  let diff: ReturnType<typeof diffTrees> = [];
  let loading = false;
  let error = "";
  let compared = false;
  let mode: "folder" | "file" = "folder";
  let fileDiff: ByteDiff | null = null;

  async function readBytes(path: string): Promise<Uint8Array> {
    const arr = await invoke<number[]>("read_file_range", { path, offset: 0, len: FILE_CMP_CAP });
    return new Uint8Array(arr);
  }

  async function compare() {
    if (!left.trim() || !right.trim()) return;
    loading = true;
    error = "";
    fileDiff = null;
    try {
      // Folders scan; a file makes scan_tree error ("not a folder") — fall through to the byte compare.
      const [l, r] = await Promise.all([
        invoke<CompareNode[]>("scan_tree", { path: left.trim(), maxDepth: 32 }),
        invoke<CompareNode[]>("scan_tree", { path: right.trim(), maxDepth: 32 }),
      ]);
      diff = diffTrees(l, r);
      summary = summarizeDiff(diff);
      collapsed = new Set();
      reflow();
      mode = "folder";
      compared = true;
    } catch (folderErr) {
      try {
        const [a, b] = await Promise.all([readBytes(left.trim()), readBytes(right.trim())]);
        fileDiff = byteDiff(a, b);
        mode = "file";
        compared = true;
      } catch {
        error = String(folderErr); // neither two folders nor two readable files
        rows = [];
        compared = false;
      }
    } finally {
      loading = false;
    }
  }

  function reflow() {
    rows = flattenDiff(diff, collapsed);
  }

  function toggle(row: DiffRow) {
    if (!row.hasChildren) return;
    const next = new Set(collapsed);
    next.has(row.path) ? next.delete(row.path) : next.add(row.path);
    collapsed = next;
    reflow();
  }

  const STATUS_LABEL: Record<string, string> = {
    added: "added",
    removed: "removed",
    changed: "changed",
    identical: "same",
  };
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("cancel")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("cancel")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Compare" on:click|stopPropagation>
    <h2>Compare</h2>

    <div class="paths">
      <input class="path" placeholder="Left folder or file…" bind:value={left} aria-label="Left path" />
      <span class="vs">vs</span>
      <input class="path" placeholder="Right folder or file…" bind:value={right} aria-label="Right path" />
      <button class="btn primary" data-testid="compare-btn" on:click={compare} disabled={loading || !left.trim() || !right.trim()}>Compare</button>
    </div>

    {#if compared && !error && mode === "folder"}
      <div class="summary" data-testid="compare-summary">
        <span class="s-added">+{summary.added}</span>
        <span class="s-removed">−{summary.removed}</span>
        <span class="s-changed">~{summary.changed}</span>
        <span class="s-identical">={summary.identical}</span>
      </div>
    {/if}

    <div class="tree" data-testid="compare-tree">
      {#if error}
        <div class="err">{error}</div>
      {:else if loading}
        <div class="empty">Comparing…</div>
      {:else if !compared}
        <div class="empty">Enter two folders (or two files) and press Compare.</div>
      {:else if mode === "file" && fileDiff}
        <div class="filecmp" data-testid="file-compare">
          {#if fileDiff.equal}
            <div class="fc-equal" data-testid="file-equal">✓ Files are byte-for-byte identical.</div>
          {:else}
            <div class="fc-row"><span class="fc-k">First difference</span><span class="fc-v">at offset 0x{(fileDiff.firstDiff ?? 0).toString(16).toUpperCase()} ({fileDiff.firstDiff})</span></div>
            <div class="fc-row"><span class="fc-k">Differing ranges</span><span class="fc-v" data-testid="file-ranges">{fileDiff.ranges.length}</span></div>
            <div class="fc-row"><span class="fc-k">Lengths</span><span class="fc-v">{fileDiff.lengthDiffers ? "differ" : "match"}</span></div>
          {/if}
        </div>
      {:else if mode === "folder" && rows.length === 0}
        <div class="empty">Both folders are empty.</div>
      {:else}
        {#each rows as row (row.path)}
          <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
          <div
            class="node status-{row.node.status}"
            data-testid="compare-node"
            data-status={row.node.status}
            style="padding-left: {8 + row.depth * 16}px"
            on:click={() => toggle(row)}
          >
            <span class="caret">{row.hasChildren ? (collapsed.has(row.path) ? "▸" : "▾") : ""}</span>
            <span class="nname">{row.node.name}</span>
            <span class="nstatus">{STATUS_LABEL[row.node.status]}</span>
          </div>
        {/each}
      {/if}
    </div>

    <div class="actions">
      <button class="btn" on:click={() => dispatch("cancel")}>Close</button>
    </div>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.25); display: grid; place-items: center; z-index: 200; }
  .dialog { width: 720px; max-width: 95vw; background: var(--surface); border: 1px solid var(--border-strong); border-radius: 10px; box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25); padding: 20px; }
  h2 { font-size: 16px; margin-bottom: 12px; }
  .paths { display: flex; align-items: center; gap: 8px; margin-bottom: 10px; }
  .path { flex: 1 1 auto; height: 30px; padding: 0 8px; font: inherit; color: var(--text); background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); min-width: 0; }
  .vs { color: var(--text-dim); font-size: 12px; }
  .summary { display: flex; gap: 12px; margin-bottom: 8px; font-size: 12.5px; font-variant-numeric: tabular-nums; }
  .s-added { color: #2e9e4f; }
  .s-removed { color: #c0392b; }
  .s-changed { color: #b8860b; }
  .s-identical { color: var(--text-dim); }
  .tree { height: 50vh; overflow: auto; border: 1px solid var(--border); border-radius: var(--radius); }
  .node { display: flex; align-items: baseline; gap: 6px; padding: 2px 8px; font-size: 12.5px; cursor: default; white-space: nowrap; }
  .node:hover { background: var(--surface-alt); }
  .caret { flex: 0 0 12px; color: var(--text-dim); }
  .nname { flex: 1 1 auto; overflow: hidden; text-overflow: ellipsis; }
  .nstatus { flex: 0 0 auto; font-size: 10px; text-transform: uppercase; letter-spacing: 0.03em; color: var(--text-dim); }
  .status-added .nname { color: #2e9e4f; }
  .status-removed .nname { color: #c0392b; }
  .status-changed .nname { color: #b8860b; }
  .status-identical .nname { color: var(--text-dim); }
  .filecmp { padding: 12px; font-size: 13px; }
  .fc-equal { color: #2e9e4f; }
  .fc-row { display: flex; gap: 12px; padding: 3px 0; }
  .fc-k { flex: 0 0 130px; color: var(--text-dim); }
  .fc-v { font-family: ui-monospace, monospace; }
  .empty, .err { padding: 12px; color: var(--text-dim); font-size: 12.5px; }
  .err { color: #c0392b; }
  .actions { display: flex; justify-content: flex-end; margin-top: 14px; }
  .btn { height: 30px; padding: 0 14px; border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-alt); color: var(--text); }
  .btn:disabled { opacity: 0.4; }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
</style>
