<script lang="ts">
  /**
   * Folder templates gallery (CPE-837, epic CPE-740). Lists stored templates, captures the current
   * folder as a reusable template, and stamps a chosen template into the current folder with `{token}`
   * substitution. A thin render over the tested `cpe_server::folder_template` core (CPE-835/836) via the
   * typed `commands.*` client — capture/stamp do the real filesystem work in the backend.
   */
  import { createEventDispatcher, onMount } from "svelte";
  import { unwrap } from "../invoke";
  import { commands } from "../bindings.gen"; // typed client (CPE-964)
  import type { TemplateSummary } from "../bindings.gen";
  import { buildVars } from "../templateVars";

  /** The current folder: the capture source AND the stamp destination. Supplied by App. */
  export let path = "";

  const dispatch = createEventDispatcher<{ close: void; stamped: { dest: string; count: number } }>();

  let templates: TemplateSummary[] = [];
  let selected = "";
  let captureName = "";
  /** Value for the `{name}` token when stamping; `{date}` is auto-filled with today. */
  let nameVar = "";
  let importJson = "";
  let showImport = false;
  let busy = false;
  let error = "";
  let note = "";

  const base = (p: string) => p.split(/[\\/]/).pop() || p;

  async function refresh() {
    try {
      templates = unwrap(await commands.templateList());
      if (selected && !templates.some((t) => t.name === selected)) selected = "";
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }
  onMount(refresh);

  async function captureCurrent() {
    const name = captureName.trim();
    if (!name || !path) return;
    busy = true; error = ""; note = "";
    try {
      const template = unwrap(await commands.templateCapture(path, name));
      unwrap(await commands.templateSave(template));
      captureName = "";
      note = `Captured "${name}" from ${base(path)}.`;
      await refresh();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  async function stampSelected() {
    if (!selected || !path) return;
    busy = true; error = ""; note = "";
    try {
      const template = unwrap(await commands.templateLoad(selected));
      if (!template) { error = `Template "${selected}" is gone.`; return; }
      const created = unwrap(await commands.templateStamp(template, path, buildVars(nameVar)));
      note = `Stamped ${created.length} item${created.length === 1 ? "" : "s"} into ${base(path)}.`;
      dispatch("stamped", { dest: path, count: created.length });
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  async function del(name: string) {
    busy = true; error = ""; note = "";
    try {
      unwrap(await commands.templateDelete(name));
      note = `Deleted "${name}".`;
      await refresh();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  async function exportOne(name: string) {
    try {
      const template = unwrap(await commands.templateLoad(name));
      if (!template) return;
      const json = unwrap(await commands.templateExport(template));
      await navigator.clipboard.writeText(json);
      note = `Copied "${name}" JSON to the clipboard.`;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  async function importPasted() {
    if (!importJson.trim()) return;
    busy = true; error = ""; note = "";
    try {
      unwrap(await commands.templateImport(importJson));
      importJson = "";
      showImport = false;
      note = "Imported.";
      await refresh();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("close")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Folder templates" on:click|stopPropagation>
    <h2>Folder templates</h2>
    <p>Capture a folder's structure as a reusable template, then stamp it into any folder.
       <code>{"{date}"}</code> and <code>{"{name}"}</code> in names/contents are substituted on stamp.</p>

    <div class="row">
      <input class="in" placeholder="New template name…" bind:value={captureName}
             aria-label="New template name" disabled={busy} />
      <button class="btn" data-testid="capture-btn" disabled={busy || !captureName.trim() || !path}
              on:click={captureCurrent} title={path ? `Capture ${base(path)}` : "No folder"}>
        Capture this folder
      </button>
    </div>

    <div class="list" data-testid="template-list">
      {#if templates.length === 0}
        <div class="empty">No templates yet — capture one above.</div>
      {:else}
        {#each templates as t (t.name)}
          <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
          <div class="item" class:sel={selected === t.name} data-testid="tpl-{t.name}"
               on:click={() => (selected = t.name)}>
            <span class="name" title={t.name}>{t.name}</span>
            <span class="pills"><span class="pill">{t.dirs} dirs</span><span class="pill">{t.files} files</span></span>
            <span class="rowbtns">
              <button class="mini" title="Copy JSON" on:click|stopPropagation={() => exportOne(t.name)}>Export</button>
              <button class="mini danger" title="Delete" on:click|stopPropagation={() => del(t.name)}>Delete</button>
            </span>
          </div>
        {/each}
      {/if}
    </div>

    <div class="row">
      <input class="in" placeholder="{'{name}'} value (optional)…" bind:value={nameVar}
             aria-label="Name variable" disabled={busy} />
      <button class="btn primary" data-testid="stamp-btn" disabled={busy || !selected || !path}
              on:click={stampSelected} title={path ? `Stamp into ${base(path)}` : "No folder"}>
        Stamp here
      </button>
    </div>

    {#if showImport}
      <textarea class="import" placeholder="Paste a template (or catalog) JSON…" bind:value={importJson}
                aria-label="Import JSON" disabled={busy}></textarea>
    {/if}

    <div class="status">
      {#if error}<span class="err" data-testid="error">{error}</span>
      {:else if note}<span class="note" data-testid="note">{note}</span>
      {:else}<span class="dim">{templates.length} template{templates.length === 1 ? "" : "s"}</span>{/if}
    </div>

    <div class="actions">
      {#if showImport}
        <button class="btn" disabled={busy || !importJson.trim()} on:click={importPasted}>Import JSON</button>
      {:else}
        <button class="btn" on:click={() => (showImport = true)}>Import…</button>
      {/if}
      <button class="btn primary" on:click={() => dispatch("close")}>Close</button>
    </div>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.25); display: grid; place-items: center; z-index: 200; }
  .dialog { width: 560px; max-width: 95vw; background: var(--surface); border: 1px solid var(--border-strong); border-radius: 10px; box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25); padding: 20px; }
  h2 { font-size: 16px; margin-bottom: 8px; }
  p { color: var(--text-dim); font-size: 12.5px; margin-bottom: 12px; line-height: 1.5; }
  code { background: var(--surface-alt); padding: 0 4px; border-radius: 4px; font-size: 12px; }
  .row { display: flex; gap: 8px; margin: 8px 0; }
  .in { flex: 1 1 auto; height: 30px; padding: 0 8px; font: inherit; color: var(--text); background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); min-width: 0; }
  .list { height: 40vh; overflow: auto; border: 1px solid var(--border); border-radius: var(--radius); margin: 6px 0; }
  .item { display: flex; align-items: center; gap: 10px; padding: 6px 10px; cursor: pointer; border-bottom: 1px solid var(--border); }
  .item:hover { background: var(--surface-alt); }
  .item.sel { background: color-mix(in srgb, var(--accent) 16%, var(--surface)); }
  .name { flex: 1 1 auto; font-size: 13px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .pills { display: flex; flex-wrap: wrap; gap: 6px; flex: 0 0 auto; }
  .pill { flex: 0 0 auto; white-space: nowrap; font-size: 11px; color: var(--text-dim); background: var(--surface-alt); border: 1px solid var(--border); border-radius: 999px; padding: 1px 8px; }
  .rowbtns { display: flex; gap: 6px; flex: 0 0 auto; }
  .mini { height: 24px; padding: 0 8px; font-size: 11px; border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); color: var(--text); }
  .mini.danger:hover { border-color: #c0392b; color: #c0392b; }
  .import { width: 100%; height: 96px; margin: 4px 0; padding: 8px; font: 12px/1.4 var(--mono, monospace); color: var(--text); background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); resize: vertical; box-sizing: border-box; }
  .status { min-height: 18px; margin: 6px 2px; font-size: 12px; }
  .note { color: var(--accent); }
  .err { color: #c0392b; }
  .dim { color: var(--text-dim); }
  .empty { padding: 14px; color: var(--text-dim); font-size: 12.5px; }
  .actions { display: flex; justify-content: space-between; align-items: center; margin-top: 12px; }
  .btn { height: 30px; padding: 0 14px; border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-alt); color: var(--text); }
  .btn:disabled { opacity: 0.4; }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
</style>
