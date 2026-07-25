<script lang="ts">
  // In-app three-way conflict resolver (CPE-496). When a two-way-mirror sync reconciles a divergence
  // (merge/rebase) and leaves unmerged files, this lists them with their state and lets the user pick
  // (ours / theirs / base) or edit the resolution per file, stages it, then Continue or Abort. Abort
  // restores the pre-sync state — work is never lost. Backed by the host forge_conflict_* commands.
  import { createEventDispatcher, onMount } from "svelte";
  import { unwrap } from "../invoke";
  import { commands } from "../bindings.gen"; // typed client (CPE-964)
  import Icon from "./Icon.svelte";

  export let path: string;

  const dispatch = createEventDispatcher<{ close: void; done: void }>();

  interface ConflictFile { path: string; code: string; label: string }
  interface Versions { base: string | null; ours: string | null; theirs: string | null; merged: string | null; truncated: boolean }

  let operation = "none";
  let files: ConflictFile[] = [];
  let selected: string | null = null;
  let versions: Versions | null = null;
  let resolution = "";
  let busy = false;
  let error = "";
  let note = "";
  let showBase = false;

  async function loadState() {
    try {
      const s = await commands.forgeConflictState(path);
      operation = s.operation;
      files = s.files;
      if (selected && !files.some((f) => f.path === selected)) { selected = null; versions = null; }
      if (!selected && files.length) await select(files[0].path);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }
  onMount(loadState);

  async function select(file: string) {
    selected = file;
    versions = null;
    error = "";
    try {
      versions = await commands.forgeConflictVersions(path, file);
      // Start the resolution from the working-tree merge (markers included) so the user edits in place.
      resolution = versions.merged ?? versions.ours ?? versions.theirs ?? "";
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  function use(which: "ours" | "theirs" | "base") {
    const v = versions?.[which];
    if (v != null) resolution = v;
  }

  async function markResolved() {
    if (!selected) return;
    busy = true; error = ""; note = "";
    try {
      unwrap(await commands.forgeResolveFile(path, selected, resolution));
      note = `Staged ${selected}`;
      selected = null;
      await loadState(); // resolved file drops off the list; auto-selects the next
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  async function finish(kind: "continue" | "abort") {
    busy = true; error = "";
    try {
      const msg = unwrap(
        kind === "continue"
          ? await commands.forgeConflictContinue(path)
          : await commands.forgeConflictAbort(path),
      );
      note = msg;
      dispatch("done");
      dispatch("close");
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  $: unresolved = files.length;
  $: opLabel = operation === "rebase" ? "Rebase" : operation === "merge" ? "Merge" : "";
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && !busy && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => !busy && dispatch("close")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" on:click|stopPropagation>
    <div class="head">
      <h2><Icon name="code" size={16} /> Resolve conflicts {opLabel ? `— ${opLabel}` : ""}</h2>
      <button class="x" title="Close" aria-label="Close" on:click={() => dispatch("close")}>×</button>
    </div>

    {#if operation === "none" && files.length === 0}
      <div class="empty">No conflicts — nothing to resolve.</div>
    {:else}
      <div class="body">
        <aside class="files">
          <div class="files-head">{unresolved} unresolved</div>
          {#each files as f (f.path)}
            <button class="file" class:sel={selected === f.path} on:click={() => select(f.path)} title={f.path}>
              <span class="file-name">{f.path}</span>
              <span class="file-kind">{f.label}</span>
            </button>
          {/each}
          {#if files.length === 0}
            <div class="all-clear">✓ All files staged. Continue to finish {opLabel.toLowerCase()}.</div>
          {/if}
        </aside>

        <section class="pane">
          {#if selected && versions}
            <div class="picks">
              <span>Fill resolution from:</span>
              <button class="chip-btn" on:click={() => use("ours")} disabled={versions.ours == null}>Ours</button>
              <button class="chip-btn" on:click={() => use("theirs")} disabled={versions.theirs == null}>Theirs</button>
              <button class="chip-btn" on:click={() => use("base")} disabled={versions.base == null}>Base</button>
              <button class="chip-btn ghost" on:click={() => (showBase = !showBase)}>{showBase ? "Hide" : "Show"} three-way</button>
            </div>

            {#if versions.truncated}
              <p class="warn">A version was too large or binary to show — edit the resolution directly or abort.</p>
            {/if}

            {#if showBase}
              <div class="threeway">
                <div class="col"><div class="col-h">Base</div><pre>{versions.base ?? "— absent —"}</pre></div>
                <div class="col"><div class="col-h ours">Ours</div><pre>{versions.ours ?? "— absent —"}</pre></div>
                <div class="col"><div class="col-h theirs">Theirs</div><pre>{versions.theirs ?? "— absent —"}</pre></div>
              </div>
            {/if}

            <div class="res-head">Resolution <em>(edit inline — remove the <code>&lt;&lt;&lt;&lt;&lt;&lt;&lt;</code> / <code>=======</code> / <code>&gt;&gt;&gt;&gt;&gt;&gt;&gt;</code> markers)</em></div>
            <textarea class="res" bind:value={resolution} spellcheck="false"></textarea>
            <div class="res-actions">
              <button class="btn primary" on:click={markResolved} disabled={busy}>Mark resolved &amp; stage</button>
            </div>
          {:else if files.length}
            <div class="pane-empty">Select a file to resolve.</div>
          {/if}
        </section>
      </div>
    {/if}

    <div class="status" class:error={!!error} class:ok={!!note && !error}>{error || note || `${opLabel || "No"} operation in progress`}</div>

    <div class="foot">
      <button class="btn danger" on:click={() => finish("abort")} disabled={busy || operation === "none"}
        title="Abort and restore the pre-sync state — no work is lost">Abort</button>
      <div class="spacer"></div>
      <button class="btn" on:click={() => dispatch("close")} disabled={busy}>Close</button>
      <button class="btn primary" on:click={() => finish("continue")} disabled={busy || operation === "none" || unresolved > 0}
        title={unresolved > 0 ? "Resolve every file first" : `Continue the ${opLabel.toLowerCase()}`}>
        Continue {opLabel.toLowerCase()}
      </button>
    </div>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0,0,0,0.4); display: grid; place-items: center; z-index: 210; }
  .dialog { width: min(880px, 95vw); height: min(680px, 92vh); display: flex; flex-direction: column;
    background: var(--surface); color: var(--text); border: 1px solid var(--border-strong);
    border-radius: 10px; box-shadow: 0 20px 50px rgba(0,0,0,0.3); overflow: hidden; }

  .head { display: flex; align-items: center; justify-content: space-between; padding: 12px 16px;
    border-bottom: 1px solid var(--border); }
  .head h2 { display: flex; align-items: center; gap: 8px; font-size: 15px; }
  .x { border: 0; background: transparent; color: var(--text-dim); font-size: 20px; cursor: pointer; line-height: 1; padding: 0 4px; }
  .x:hover { color: var(--text); }
  .empty { flex: 1; display: grid; place-items: center; color: var(--text-dim); }

  .body { flex: 1; display: flex; min-height: 0; }
  .files { width: 240px; flex: 0 0 auto; border-right: 1px solid var(--border); overflow-y: auto; padding: 6px; }
  .files-head { font-size: 10px; text-transform: uppercase; letter-spacing: .05em; color: var(--text-faint); padding: 4px 6px; }
  .file { display: flex; flex-direction: column; gap: 2px; width: 100%; text-align: left; border: 0;
    background: transparent; color: inherit; cursor: pointer; padding: 6px 8px; border-radius: 6px; }
  .file:hover { background: rgba(128,128,128,0.14); }
  .file.sel { background: var(--selection, rgba(128,128,128,0.22)); }
  .file-name { font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .file-kind { font-size: 10px; color: var(--text-dim); }
  .all-clear { padding: 10px 8px; font-size: 12px; color: var(--text-dim); line-height: 1.4; }

  .pane { flex: 1; min-width: 0; display: flex; flex-direction: column; padding: 10px 14px; overflow: hidden; }
  .pane-empty { margin: auto; color: var(--text-dim); }
  .picks { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; margin-bottom: 8px; font-size: 12px; }
  .picks > span { color: var(--text-dim); }
  .chip-btn { white-space: nowrap; flex: 0 0 auto; font-size: 11px; padding: 3px 10px; border-radius: 6px;
    border: 1px solid var(--border-strong); background: var(--surface-alt); color: var(--text); cursor: pointer; }
  .chip-btn:hover:not(:disabled) { background: rgba(128,128,128,0.16); }
  .chip-btn.ghost { margin-left: auto; }
  .chip-btn:disabled { opacity: .45; cursor: default; }

  .threeway { display: flex; gap: 8px; height: 170px; margin-bottom: 10px; }
  .col { flex: 1; min-width: 0; display: flex; flex-direction: column; border: 1px solid var(--border); border-radius: 6px; overflow: hidden; }
  .col-h { font-size: 10px; text-transform: uppercase; letter-spacing: .05em; padding: 4px 8px;
    background: var(--surface-alt); border-bottom: 1px solid var(--border); color: var(--text-dim); }
  .col-h.ours { color: #3b83c0; }
  .col-h.theirs { color: #7a5cc0; }
  .col pre { flex: 1; overflow: auto; margin: 0; padding: 6px 8px; font-family: var(--mono, monospace);
    font-size: 11px; white-space: pre; }

  .res-head { font-size: 11px; color: var(--text-faint); text-transform: uppercase; letter-spacing: .04em; margin-bottom: 6px; }
  .res-head em { font-style: normal; text-transform: none; letter-spacing: 0; opacity: .8; }
  .res-head code { font-size: 10px; }
  .res { flex: 1; min-height: 120px; resize: none; font-family: var(--mono, monospace); font-size: 12px;
    padding: 8px; border: 1px solid var(--border-strong); border-radius: 6px; background: var(--surface-alt); color: inherit; }
  .res-actions { display: flex; justify-content: flex-end; margin-top: 8px; }
  .warn { color: #b5872b; font-size: 11px; margin: 0 0 8px; }

  .status { padding: 6px 16px; font-size: 12px; min-height: 20px; color: var(--text-dim);
    border-top: 1px solid var(--border); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .status.error { color: #e0706b; }
  .status.ok { color: var(--accent); }

  .foot { display: flex; align-items: center; gap: 8px; padding: 12px 16px; border-top: 1px solid var(--border); }
  .spacer { flex: 1; }
  .btn { height: 32px; padding: 0 16px; border: 1px solid var(--border-strong); border-radius: var(--radius);
    background: var(--surface-alt); color: var(--text); cursor: pointer; }
  .btn:disabled { opacity: .5; cursor: default; }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
  .btn.primary:not(:disabled):hover { background: var(--accent-hover); }
  /* The menu-colour rule bans red menu *text*; a destructive push-button may carry an accent fill. */
  .btn.danger { border-color: #c42b1c; color: #c42b1c; background: transparent; }
  .btn.danger:not(:disabled):hover { background: #c42b1c; color: #fff; }
</style>
