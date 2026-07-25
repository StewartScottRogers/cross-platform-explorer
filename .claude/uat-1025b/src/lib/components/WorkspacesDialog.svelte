<script lang="ts">
  /**
   * Workspace switcher (CPE-788, epic CPE-708). Save the current window state (open tab paths +
   * view/sort/filter) as a named workspace, and switch / rename / delete saved ones. A thin CRUD over the
   * tested `workspaces` store; App captures the current tabs and applies a chosen workspace, and persists
   * the list. `change` persists; `switch` applies.
   */
  import { createEventDispatcher } from "svelte";
  import { addWorkspace, renameWorkspace, removeWorkspace, type Workspace, type WorkspaceTab } from "../workspaces";

  export let workspaces: Workspace[] = [];
  /** The current window's tabs, captured by App when the dialog opened. */
  export let currentTabs: WorkspaceTab[] = [];
  /** CPE-789: whether the app reopens the last session on launch. */
  export let autoRestore = false;

  const dispatch = createEventDispatcher<{ change: Workspace[]; switch: Workspace; cancel: void; autoRestore: boolean }>();

  let list: Workspace[] = workspaces.map((w) => ({ ...w, tabs: [...w.tabs] }));
  let newName = "";
  let renamingId = "";
  let renameValue = "";

  function persist() {
    dispatch("change", list);
  }

  function saveCurrent() {
    const n = newName.trim();
    if (!n) return;
    list = addWorkspace(list, n, currentTabs);
    newName = "";
    persist();
  }

  function beginRename(w: Workspace) {
    renamingId = w.id;
    renameValue = w.name;
  }
  function commitRename() {
    const n = renameValue.trim();
    if (n) list = renameWorkspace(list, renamingId, n);
    renamingId = "";
    persist();
  }
  function del(id: string) {
    list = removeWorkspace(list, id);
    persist();
  }
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("cancel")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("cancel")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Workspaces" on:click|stopPropagation>
    <h2>Workspaces</h2>
    <p>A workspace remembers your open tabs. Switch to reopen them all.</p>

    <div class="save-row" data-testid="save-workspace">
      <input class="grow" placeholder="Save current {currentTabs.length} tab{currentTabs.length === 1 ? '' : 's'} as…" bind:value={newName} aria-label="New workspace name" on:keydown={(e) => e.key === 'Enter' && saveCurrent()} />
      <button class="btn primary" data-testid="save-btn" disabled={!newName.trim()} on:click={saveCurrent}>Save</button>
    </div>

    <div class="list" data-testid="workspace-list">
      {#if list.length === 0}<div class="empty">No saved workspaces yet.</div>{/if}
      {#each list as w (w.id)}
        <div class="ws" data-testid="workspace-row">
          {#if renamingId === w.id}
            <input class="grow" bind:value={renameValue} aria-label="Rename workspace" on:keydown={(e) => e.key === 'Enter' && commitRename()} on:blur={commitRename} />
          {:else}
            <button class="name" data-testid="switch-btn" title="Switch to this workspace" on:click={() => dispatch('switch', w)}>{w.name}</button>
            <span class="count">{w.tabs.length} tab{w.tabs.length === 1 ? '' : 's'}</span>
            <button class="mini" aria-label="Rename" on:click={() => beginRename(w)}>✎</button>
            <button class="mini danger" aria-label="Delete" on:click={() => del(w.id)}>✕</button>
          {/if}
        </div>
      {/each}
    </div>

    <label class="restore-row" title="Reopen the tabs you had open when you last closed the app">
      <input type="checkbox" data-testid="autorestore-toggle" checked={autoRestore}
             on:change={(e) => dispatch('autoRestore', e.currentTarget.checked)} />
      Reopen last session on launch
    </label>

    <div class="actions">
      <button class="btn primary" on:click={() => dispatch('cancel')}>Close</button>
    </div>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.25); display: grid; place-items: center; z-index: 200; }
  .dialog { width: 480px; max-width: 92vw; background: var(--surface); border: 1px solid var(--border-strong); border-radius: 10px; box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25); padding: 20px; }
  h2 { font-size: 16px; margin-bottom: 8px; }
  p { color: var(--text-dim); font-size: 12.5px; margin-bottom: 12px; }
  .save-row { display: flex; gap: 8px; margin-bottom: 12px; }
  .grow { flex: 1 1 auto; height: 32px; padding: 0 8px; font: inherit; color: var(--text); background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); min-width: 0; }
  .list { max-height: 44vh; overflow-y: auto; display: flex; flex-direction: column; gap: 5px; }
  .empty { color: var(--text-dim); font-size: 12.5px; padding: 8px 2px; }
  .ws { display: flex; align-items: center; gap: 8px; padding: 5px 6px; border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface-alt); }
  .name { flex: 1 1 auto; text-align: left; background: none; border: none; color: var(--text); font: inherit; font-weight: 600; cursor: pointer; padding: 2px 4px; border-radius: var(--radius); }
  .name:hover { background: var(--surface); }
  .count { flex: 0 0 auto; font-size: 11.5px; color: var(--text-dim); }
  .mini { width: 24px; height: 24px; border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); color: var(--text); }
  .restore-row { display: flex; align-items: center; gap: 6px; margin-top: 14px; font-size: 12.5px; color: var(--text-dim); }
  .actions { display: flex; justify-content: flex-end; margin-top: 16px; }
  .btn { height: 32px; padding: 0 16px; border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-alt); color: var(--text); }
  .btn:disabled { opacity: 0.4; }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
</style>
