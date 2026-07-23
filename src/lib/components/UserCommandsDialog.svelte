<script lang="ts">
  /**
   * User-command manager (CPE-783, epic CPE-711): define / edit / remove / reorder templated commands and
   * choose where each surfaces (toolbar / context menu / palette). Pure list ops from `userCommands.ts`;
   * this dialog just edits a working copy and dispatches `change` so the host persists it. Launching a
   * command is the separate, confirmed `RunCommandConfirm` flow.
   */
  import { createEventDispatcher } from "svelte";
  import { addCommand, updateCommand, removeCommand, moveCommand, type UserCommand, type CommandSurface } from "../userCommands";
  import Icon from "./Icon.svelte";

  export let commands: UserCommand[] = [];

  const dispatch = createEventDispatcher<{ change: UserCommand[]; close: void }>();
  const SURFACES: CommandSurface[] = ["toolbar", "context", "palette"];

  let editingId: string | null = null; // "new" while adding, an id while editing, null when idle
  let fName = "";
  let fTemplate = "";
  let fMode: "each" | "joined" = "each";
  let fSurfaces: CommandSurface[] = ["context"];

  function commit(list: UserCommand[]) {
    commands = list;
    dispatch("change", list);
  }
  function startAdd() {
    editingId = "new";
    fName = "";
    fTemplate = "";
    fMode = "each";
    fSurfaces = ["context"];
  }
  function startEdit(c: UserCommand) {
    editingId = c.id;
    fName = c.name;
    fTemplate = c.template;
    fMode = c.mode;
    fSurfaces = [...c.surfaces];
  }
  function toggleSurface(s: CommandSurface) {
    fSurfaces = fSurfaces.includes(s) ? fSurfaces.filter((x) => x !== s) : [...fSurfaces, s];
  }
  function saveForm() {
    const name = fName.trim();
    const template = fTemplate.trim();
    if (!name || !template) return;
    const surfaces = fSurfaces.length ? fSurfaces : ["context" as CommandSurface];
    if (editingId === "new") commit(addCommand(commands, name, template, { mode: fMode, surfaces }));
    else if (editingId) commit(updateCommand(commands, editingId, { name, template, mode: fMode, surfaces }));
    editingId = null;
  }
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("close")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" on:click|stopPropagation>
    <header>
      <Icon name="code" size={15} />
      <h2>User commands</h2>
      <button class="add" on:click={startAdd}>+ New command</button>
      <button class="x" title="Close" on:click={() => dispatch("close")}><Icon name="close" size={14} /></button>
    </header>

    <p class="hint">Templated commands you can run over the selection. Tokens: <code>{"{path}"}</code>
      <code>{"{name}"}</code> <code>{"{dir}"}</code>. They surface where you choose, and always ask before running.</p>

    <div class="list">
      {#each commands as c, i (c.id)}
        <div class="row">
          <div class="row-main">
            <span class="row-name">{c.name}</span>
            <span class="row-tpl">{c.template}</span>
            <span class="row-tags">
              <span class="pill">{c.mode}</span>
              {#each c.surfaces as s}<span class="pill surf">{s}</span>{/each}
            </span>
          </div>
          <div class="row-actions">
            <button class="mini" title="Move up" disabled={i === 0} on:click={() => commit(moveCommand(commands, c.id, -1))}>↑</button>
            <button class="mini" title="Move down" disabled={i === commands.length - 1} on:click={() => commit(moveCommand(commands, c.id, 1))}>↓</button>
            <button class="mini" title="Edit" on:click={() => startEdit(c)}>✎</button>
            <button class="mini" title="Remove" on:click={() => commit(removeCommand(commands, c.id))}><Icon name="delete" size={13} /></button>
          </div>
        </div>
      {/each}
      {#if commands.length === 0 && editingId === null}
        <div class="empty">No commands yet. Click <b>+ New command</b> to add one.</div>
      {/if}
    </div>

    {#if editingId !== null}
      <div class="editor">
        <div class="field"><label for="uc-name">Name</label><input id="uc-name" bind:value={fName} placeholder="Open in VS Code" spellcheck="false" /></div>
        <div class="field"><label for="uc-tpl">Command template</label><input id="uc-tpl" bind:value={fTemplate} placeholder="code {'{path}'}" spellcheck="false" /></div>
        <div class="opts">
          <div class="opt">
            <span class="opt-label">Run</span>
            <label><input type="radio" bind:group={fMode} value="each" /> once per item</label>
            <label><input type="radio" bind:group={fMode} value="joined" /> once (joined)</label>
          </div>
          <div class="opt">
            <span class="opt-label">Show in</span>
            {#each SURFACES as s}<label><input type="checkbox" checked={fSurfaces.includes(s)} on:change={() => toggleSurface(s)} /> {s}</label>{/each}
          </div>
        </div>
        <div class="editor-actions">
          <button class="btn" on:click={() => (editingId = null)}>Cancel</button>
          <button class="btn primary" on:click={saveForm} disabled={!fName.trim() || !fTemplate.trim()}>Save</button>
        </div>
      </div>
    {/if}
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.3); display: grid; place-items: center; z-index: 205; }
  .dialog {
    width: 640px; max-width: 94vw; max-height: 86vh; display: flex; flex-direction: column;
    background: var(--surface); color: var(--text); border: 1px solid var(--border-strong);
    border-radius: 10px; box-shadow: 0 20px 50px rgba(0, 0, 0, 0.28); padding: 14px 18px 16px;
  }
  header { display: flex; align-items: center; gap: 8px; margin-bottom: 6px; }
  h2 { font-size: 15px; flex: 1; }
  .add { height: 28px; padding: 0 12px; font-size: 12px; border: 1px solid var(--border-strong); border-radius: 6px; background: var(--surface-alt); }
  .add:hover { background: rgba(128,128,128,0.14); }
  .x { width: 28px; height: 28px; display: grid; place-items: center; color: var(--text-dim); }
  .x:hover { color: var(--text); }
  .hint { font-size: 12px; color: var(--text-dim); line-height: 1.5; margin-bottom: 10px; }
  .hint code { font-family: ui-monospace, monospace; background: rgba(128,128,128,0.15); padding: 1px 5px; border-radius: 4px; font-size: 11.5px; }
  .list { overflow: auto; display: flex; flex-direction: column; gap: 6px; min-height: 40px; }
  .row { display: flex; align-items: center; gap: 8px; padding: 8px 10px; border: 1px solid var(--border); border-radius: 6px; }
  .row-main { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 3px; }
  .row-name { font-weight: 600; font-size: 13px; }
  .row-tpl { font-family: ui-monospace, monospace; font-size: 12px; color: var(--text-dim); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .row-tags { display: flex; flex-wrap: wrap; gap: 4px; }
  .pill { white-space: nowrap; flex: 0 0 auto; font-size: 10px; padding: 1px 7px; border-radius: 999px; border: 1px solid var(--border-strong); background: var(--surface-alt); }
  .pill.surf { color: var(--accent); border-color: var(--accent); }
  .row-actions { display: flex; gap: 3px; flex: 0 0 auto; }
  .mini { width: 26px; height: 26px; display: grid; place-items: center; border: 1px solid var(--border); border-radius: 5px; background: var(--surface-alt); color: var(--text-dim); }
  .mini:hover:not(:disabled) { color: var(--text); background: rgba(128,128,128,0.14); }
  .mini:disabled { opacity: 0.4; }
  .empty { color: var(--text-faint); text-align: center; padding: 18px; font-size: 13px; }
  .editor { margin-top: 12px; padding-top: 12px; border-top: 1px solid var(--border); display: flex; flex-direction: column; gap: 10px; }
  .field { display: flex; flex-direction: column; gap: 4px; }
  .field label { font-size: 11px; text-transform: uppercase; letter-spacing: .05em; color: var(--text-faint); font-weight: 600; }
  .field input { height: 32px; padding: 0 10px; border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-alt); color: var(--text); font: inherit; }
  .field input:focus { outline: none; border-color: var(--accent); }
  .opts { display: flex; gap: 24px; flex-wrap: wrap; }
  .opt { display: flex; align-items: center; gap: 12px; font-size: 12.5px; }
  .opt-label { font-size: 11px; text-transform: uppercase; letter-spacing: .05em; color: var(--text-faint); font-weight: 600; }
  .opt label { display: flex; align-items: center; gap: 5px; }
  .editor-actions { display: flex; justify-content: flex-end; gap: 8px; }
  .btn { height: 32px; padding: 0 16px; border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-alt); }
  .btn:disabled { opacity: 0.5; }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
</style>
