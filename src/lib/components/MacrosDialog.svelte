<script lang="ts">
  /**
   * Macro library dialog (CPE-1189, epic CPE-739). Modeled closely on `TemplatesDialog.svelte`: list
   * saved macros (name + step count) via `commands.macroList`/`macroLoad`, a step editor to
   * add/remove/reorder Rename/Move/Tag/Convert steps, create/delete/save via
   * `commands.macroSave`/`macroDelete`, export-to-clipboard (`macroExport`) + import-from-paste
   * (`macroImport`). Running a macro is a SEPARATE, confirmed flow (`MacroRunConfirm`, CPE-1191) —
   * this dialog only manages the catalog, it never executes anything.
   *
   * Each row also owns its CPE-1191 surface/hotkey binding (context menu / palette / hotkey) — this is
   * the only place a macro gets exposed anywhere else in the UI, mirroring `UserCommandsDialog`'s
   * per-row surface checkboxes. `bindings` is a controlled prop (App.svelte owns persistence via
   * `settings.saveMacroBindings`); this dialog only edits a working copy and dispatches
   * `bindingschange` with the full updated list, exactly like `UserCommandsDialog`'s `change` event.
   */
  import { createEventDispatcher, onMount } from "svelte";
  import { unwrap } from "../invoke";
  import { commands } from "../bindings.gen"; // typed client (CPE-964)
  import type { ActionMacro, MacroStep, MacroSummary } from "../bindings.gen";
  import { setBinding, bindingFor, type MacroBinding, type MacroSurface } from "../macroBindings";
  import Icon from "./Icon.svelte";

  export let bindings: MacroBinding[] = [];

  const dispatch = createEventDispatcher<{ close: void; changed: void; bindingschange: MacroBinding[] }>();

  function isBound(name: string, surface: MacroSurface): boolean {
    return bindingFor(bindings, name)?.surfaces.includes(surface) ?? false;
  }
  function toggleSurface(name: string, surface: MacroSurface) {
    const current = bindingFor(bindings, name)?.surfaces ?? [];
    const surfaces = current.includes(surface) ? current.filter((s) => s !== surface) : [...current, surface];
    bindings = setBinding(bindings, name, { surfaces });
    dispatch("bindingschange", bindings);
  }
  function setHotkey(name: string, raw: string) {
    bindings = setBinding(bindings, name, { hotkey: raw });
    dispatch("bindingschange", bindings);
  }

  let macros: MacroSummary[] = [];
  /** `null` = editor closed; `""` = creating a new macro; otherwise the saved name being edited. */
  let editingName: string | null = null;
  let formName = "";
  let steps: MacroStep[] = [];
  let importJson = "";
  let showImport = false;
  let busy = false;
  let error = "";
  let note = "";

  const STEP_KINDS = ["rename", "move", "tag", "convert"] as const;
  type StepKind = (typeof STEP_KINDS)[number];
  let newStepKind: StepKind = "rename";

  const STEP_LABEL: Record<StepKind, string> = { rename: "Rename", move: "Move", tag: "Tag", convert: "Convert" };
  const FIELD_LABEL: Record<StepKind, string> = {
    rename: "Rename template — {name} {stem} {ext} {n}, or {ask:label}",
    move: "Destination folder (or {ask:label})",
    tag: "Tag label (or {ask:label})",
    convert: "Target extension, no dot (or {ask:label})",
  };

  function defaultStep(kind: StepKind): MacroStep {
    if (kind === "rename") return { rename: { template: "" } };
    if (kind === "move") return { move: { dest: "" } };
    if (kind === "tag") return { tag: { label: "" } };
    return { convert: { to_ext: "" } };
  }

  function kindOf(step: MacroStep): StepKind {
    if ("rename" in step) return "rename";
    if ("move" in step) return "move";
    if ("tag" in step) return "tag";
    return "convert";
  }

  function fieldValue(step: MacroStep): string {
    if ("rename" in step) return step.rename.template;
    if ("move" in step) return step.move.dest;
    if ("tag" in step) return step.tag.label;
    return step.convert.to_ext;
  }

  function withField(step: MacroStep, value: string): MacroStep {
    if ("rename" in step) return { rename: { template: value } };
    if ("move" in step) return { move: { dest: value } };
    if ("tag" in step) return { tag: { label: value } };
    return { convert: { to_ext: value } };
  }

  async function refresh() {
    try {
      macros = unwrap(await commands.macroList());
      if (editingName && !macros.some((m) => m.name === editingName)) {
        editingName = null;
        formName = "";
        steps = [];
      }
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }
  onMount(refresh);

  function startNew() {
    editingName = "";
    formName = "";
    steps = [];
    error = "";
    note = "";
  }

  async function startEdit(name: string) {
    busy = true;
    error = "";
    note = "";
    try {
      const m = unwrap(await commands.macroLoad(name));
      if (!m) {
        error = `Macro "${name}" is gone.`;
        await refresh();
        return;
      }
      editingName = name;
      formName = m.name;
      steps = [...m.steps];
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  function cancelEdit() {
    editingName = null;
    formName = "";
    steps = [];
  }

  function addStep() {
    steps = [...steps, defaultStep(newStepKind)];
  }

  function removeStep(i: number) {
    steps = steps.filter((_, idx) => idx !== i);
  }

  /** Swap step `i` with its neighbor in direction `dir` (-1 up / +1 down); clamped at the ends. */
  function moveStep(i: number, dir: number) {
    const j = i + dir;
    if (j < 0 || j >= steps.length) return;
    const next = [...steps];
    [next[i], next[j]] = [next[j], next[i]];
    steps = next;
  }

  function setStepField(i: number, value: string) {
    steps = steps.map((s, idx) => (idx === i ? withField(s, value) : s));
  }

  async function saveMacro() {
    const name = formName.trim();
    if (!name || steps.length === 0) return;
    busy = true;
    error = "";
    note = "";
    try {
      unwrap(await commands.macroSave({ name, steps }));
      note = `Saved "${name}".`;
      cancelEdit();
      await refresh();
      dispatch("changed");
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  async function del(name: string) {
    busy = true;
    error = "";
    note = "";
    try {
      unwrap(await commands.macroDelete(name));
      note = `Deleted "${name}".`;
      if (editingName === name) cancelEdit();
      await refresh();
      dispatch("changed");
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      busy = false;
    }
  }

  async function exportOne(name: string) {
    try {
      const m = unwrap(await commands.macroLoad(name));
      if (!m) return;
      const json = unwrap(await commands.macroExport(m));
      await navigator.clipboard.writeText(json);
      note = `Copied "${name}" JSON to the clipboard.`;
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  async function importPasted() {
    if (!importJson.trim()) return;
    busy = true;
    error = "";
    note = "";
    try {
      unwrap(await commands.macroImport(importJson));
      importJson = "";
      showImport = false;
      note = "Imported.";
      await refresh();
      dispatch("changed");
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
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Macros" on:click|stopPropagation>
    <header>
      <Icon name="function" size={15} />
      <h2>Macros</h2>
      <button class="add" data-testid="new-macro-btn" on:click={startNew}>+ New macro</button>
      <button class="x" title="Close" on:click={() => dispatch("close")}><Icon name="close" size={14} /></button>
    </header>

    <p class="hint">
      A macro chains Rename / Move / Tag / Convert steps over a selection. Any step's text field may
      contain <code>{"{ask:label}"}</code> to prompt for a value each time it runs.
    </p>

    <div class="list" data-testid="macro-list">
      {#if macros.length === 0}
        <div class="empty">No macros yet — click <b>+ New macro</b> to add one.</div>
      {:else}
        {#each macros as m (m.name)}
          <div class="row" data-testid="macro-{m.name}">
            <span class="row-name" title={m.name}>{m.name}</span>
            <span class="pill">{m.steps} step{m.steps === 1 ? "" : "s"}</span>
            <span class="row-bind">
              <label title="Show in the right-click context menu">
                <input
                  type="checkbox"
                  checked={isBound(m.name, "context")}
                  on:change={() => toggleSurface(m.name, "context")}
                  data-testid="bind-context-{m.name}"
                /> Menu
              </label>
              <label title="Show in the command palette (Ctrl+Shift+P)">
                <input
                  type="checkbox"
                  checked={isBound(m.name, "palette")}
                  on:change={() => toggleSurface(m.name, "palette")}
                  data-testid="bind-palette-{m.name}"
                /> Palette
              </label>
              <input
                class="hotkey"
                placeholder="Hotkey (Ctrl+Alt+1)"
                value={bindingFor(bindings, m.name)?.hotkey ?? ""}
                on:change={(e) => setHotkey(m.name, e.currentTarget.value)}
                data-testid="hotkey-{m.name}"
                spellcheck="false"
              />
            </span>
            <span class="row-actions">
              <button class="mini" title="Edit" data-testid="edit-btn-{m.name}" on:click={() => startEdit(m.name)}>✎</button>
              <button class="mini" title="Copy JSON" data-testid="export-btn-{m.name}" on:click={() => exportOne(m.name)}>Export</button>
              <button class="mini danger" title="Delete" data-testid="delete-btn-{m.name}" on:click={() => del(m.name)}>
                <Icon name="delete" size={13} />
              </button>
            </span>
          </div>
        {/each}
      {/if}
    </div>

    {#if editingName !== null}
      <div class="editor">
        <div class="field">
          <label for="macro-name">Name</label>
          <input id="macro-name" bind:value={formName} placeholder="Tidy screenshots" spellcheck="false" disabled={busy} />
        </div>

        <div class="steps" data-testid="step-list">
          {#each steps as step, i (i)}
            <div class="step-row" data-testid="step-row-{i}">
              <span class="step-kind">{STEP_LABEL[kindOf(step)]}</span>
              <input
                class="step-field"
                value={fieldValue(step)}
                on:input={(e) => setStepField(i, e.currentTarget.value)}
                placeholder={FIELD_LABEL[kindOf(step)]}
                spellcheck="false"
                disabled={busy}
                data-testid="step-field-{i}"
              />
              <span class="step-actions">
                <button class="mini" title="Move up" disabled={i === 0} data-testid="up-step-{i}" on:click={() => moveStep(i, -1)}>↑</button>
                <button class="mini" title="Move down" disabled={i === steps.length - 1} data-testid="down-step-{i}" on:click={() => moveStep(i, 1)}>↓</button>
                <button class="mini" title="Remove step" data-testid="remove-step-{i}" on:click={() => removeStep(i)}>
                  <Icon name="delete" size={13} />
                </button>
              </span>
            </div>
          {/each}
          {#if steps.length === 0}
            <div class="empty small">No steps yet — add one below.</div>
          {/if}
        </div>

        <div class="add-step">
          <select bind:value={newStepKind} data-testid="new-step-kind" disabled={busy}>
            {#each STEP_KINDS as k}<option value={k}>{STEP_LABEL[k]}</option>{/each}
          </select>
          <button class="btn" data-testid="add-step-btn" on:click={addStep} disabled={busy}>+ Add step</button>
        </div>

        <div class="editor-actions">
          <button class="btn" data-testid="cancel-edit-btn" on:click={cancelEdit} disabled={busy}>Cancel</button>
          <button
            class="btn primary"
            data-testid="save-macro-btn"
            on:click={saveMacro}
            disabled={busy || !formName.trim() || steps.length === 0}
          >
            Save
          </button>
        </div>
      </div>
    {/if}

    {#if showImport}
      <textarea
        class="import"
        placeholder="Paste a macro (or catalog) JSON…"
        bind:value={importJson}
        aria-label="Import JSON"
        disabled={busy}
        data-testid="import-textarea"
      ></textarea>
    {/if}

    <div class="status">
      {#if error}<span class="err" data-testid="error">{error}</span>
      {:else if note}<span class="note" data-testid="note">{note}</span>
      {:else}<span class="dim">{macros.length} macro{macros.length === 1 ? "" : "s"}</span>{/if}
    </div>

    <div class="actions">
      {#if showImport}
        <button class="btn" data-testid="import-btn" disabled={busy || !importJson.trim()} on:click={importPasted}>Import JSON</button>
      {:else}
        <button class="btn" data-testid="import-toggle-btn" on:click={() => (showImport = true)}>Import…</button>
      {/if}
      <button class="btn primary" on:click={() => dispatch("close")}>Close</button>
    </div>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.3); display: grid; place-items: center; z-index: 200; }
  .dialog {
    width: 620px; max-width: 95vw; max-height: 88vh; display: flex; flex-direction: column; overflow: auto;
    background: var(--surface); color: var(--text); border: 1px solid var(--dialog-border);
    border-radius: 10px; box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25); padding: 20px;
  }
  header { display: flex; align-items: center; gap: 8px; margin-bottom: 6px; }
  h2 { font-size: 16px; flex: 1; }
  .add { height: 28px; padding: 0 12px; font-size: 12px; border: 1px solid var(--border-strong); border-radius: 6px; background: var(--surface-alt); color: var(--text); }
  .add:hover { background: rgba(128,128,128,0.14); }
  .x { width: 28px; height: 28px; display: grid; place-items: center; color: var(--text-dim); }
  .x:hover { color: var(--text); }
  .hint { color: var(--text-dim); font-size: 12.5px; margin-bottom: 12px; line-height: 1.5; }
  code { background: var(--surface-alt); padding: 0 4px; border-radius: 4px; font-size: 12px; }
  .list { max-height: 30vh; overflow: auto; border: 1px solid var(--border); border-radius: var(--radius); margin: 6px 0; }
  .row { display: flex; align-items: center; flex-wrap: wrap; gap: 10px; padding: 6px 10px; border-bottom: 1px solid var(--border); }
  .row-name { flex: 1 1 auto; font-size: 13px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 90px; }
  .pill { flex: 0 0 auto; white-space: nowrap; font-size: 11px; color: var(--text-dim); background: var(--surface-alt); border: 1px solid var(--border); border-radius: 999px; padding: 1px 8px; }
  .row-bind { display: flex; align-items: center; gap: 10px; flex: 0 0 auto; font-size: 11px; color: var(--text-dim); }
  .row-bind label { display: flex; align-items: center; gap: 4px; white-space: nowrap; }
  .hotkey { width: 116px; height: 22px; font-size: 11px; padding: 0 6px; border: 1px solid var(--border); border-radius: 4px; background: var(--surface); color: var(--text); }
  .row-actions { display: flex; gap: 6px; flex: 0 0 auto; }
  .mini { height: 24px; padding: 0 8px; font-size: 11px; border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); color: var(--text); display: inline-flex; align-items: center; gap: 4px; }
  .mini:disabled { opacity: 0.4; }
  .mini.danger:hover { border-color: var(--danger); color: var(--danger); }
  .empty { padding: 14px; color: var(--text-dim); font-size: 12.5px; }
  .empty.small { padding: 8px; }
  .editor { margin-top: 12px; padding-top: 12px; border-top: 1px solid var(--border); display: flex; flex-direction: column; gap: 10px; }
  .field { display: flex; flex-direction: column; gap: 4px; }
  .field label { font-size: 11px; text-transform: uppercase; letter-spacing: .05em; color: var(--text-faint); font-weight: 600; }
  .field input, .add-step select { height: 32px; padding: 0 10px; border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-alt); color: var(--text); font: inherit; }
  .field input:focus { outline: none; border-color: var(--accent); }
  .steps { display: flex; flex-direction: column; gap: 6px; }
  .step-row { display: flex; align-items: center; gap: 8px; }
  .step-kind { flex: 0 0 64px; font-size: 12px; color: var(--text-dim); }
  .step-field { flex: 1 1 auto; height: 30px; padding: 0 8px; font: inherit; color: var(--text); background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); min-width: 0; }
  .step-actions { display: flex; gap: 4px; flex: 0 0 auto; }
  .add-step { display: flex; gap: 8px; align-items: center; }
  .editor-actions { display: flex; justify-content: flex-end; gap: 8px; }
  .import { width: 100%; height: 96px; margin: 4px 0; padding: 8px; font: 12px/1.4 var(--mono, monospace); color: var(--text); background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); resize: vertical; box-sizing: border-box; }
  .status { min-height: 18px; margin: 6px 2px; font-size: 12px; }
  .note { color: var(--accent); }
  .err { color: var(--danger); }
  .dim { color: var(--text-dim); }
  .actions { display: flex; justify-content: space-between; align-items: center; margin-top: 12px; }
  .btn { height: 30px; padding: 0 14px; border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-alt); color: var(--text); }
  .btn:disabled { opacity: 0.4; }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
</style>
