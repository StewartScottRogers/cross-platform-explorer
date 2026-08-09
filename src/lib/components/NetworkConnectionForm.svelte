<script lang="ts">
  /** "+ Add a connection" / Edit inline form for the Network sidebar section (CPE-1513, epic CPE-1498) — an
      inline/instant control ([[prefer-inline-instant-controls]]), not a modal: protocol dropdown (sftp/webdav
      to start), host, optional user/port/path, and (for key auth) a key-file field with a native Browse
      picker ([[path-inputs-need-picker]]). Mirrors SmartFolderMenu/NetworkSecretPrompt's popover shape — a
      clearly-bordered box anchored at the trigger's click point ([[dialogs-need-visible-border]]), not a
      full-screen dialog. Pure validation lives in `network.ts`'s `buildConnection` (unit-tested); this
      component only wires the fields to it and shows the returned error string inline. */
  import { createEventDispatcher, onMount } from "svelte";
  import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
  import Icon from "./Icon.svelte";
  import {
    SUPPORTED_SCHEMES,
    blankConnectionForm,
    buildConnection,
    type ConnectionFormInput,
  } from "../network";
  import type { Connection } from "../types";

  export let x = 0;
  export let y = 0;
  /** Editing an existing connection pre-fills the form; `null` is "+ Add a connection". */
  export let editing: Connection | null = null;
  export let initial: ConnectionFormInput = blankConnectionForm();

  const dispatch = createEventDispatcher<{ save: Connection; close: void }>();

  let form: ConnectionFormInput = { ...initial };
  let error = "";

  let el: HTMLDivElement;
  let left = x;
  let top = y;
  onMount(() => {
    const rect = el.getBoundingClientRect();
    const pad = 6;
    left = Math.max(pad, Math.min(x, window.innerWidth - rect.width - pad));
    top = Math.max(pad, Math.min(y, window.innerHeight - rect.height - pad));
  });

  async function browseKey() {
    try {
      const picked = await openFileDialog({ directory: false, multiple: false, title: "Choose a private key file" });
      if (typeof picked === "string") form.keyPath = picked;
    } catch {
      // No native dialog available (e.g. headless test env) — the text field still accepts typed input.
    }
  }

  function save() {
    const result = buildConnection(form);
    if (typeof result === "string") {
      error = result;
      return;
    }
    dispatch("save", result);
  }
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("close")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div
    class="menu"
    role="dialog"
    aria-label={editing ? `Edit connection ${editing.name}` : "Add a connection"}
    style="left:{left}px; top:{top}px"
    bind:this={el}
    on:click|stopPropagation
  >
    <div class="head">{editing ? `Edit “${editing.name}”` : "Add a connection"}</div>

    <div class="grid">
      <label class="field">
        <span>Name</span>
        <input bind:value={form.name} spellcheck="false" autocomplete="off" disabled={!!editing} placeholder="my-server" />
      </label>
      <label class="field">
        <span>Protocol</span>
        <select bind:value={form.scheme}>
          {#each SUPPORTED_SCHEMES as s (s)}
            <option value={s}>{s}</option>
          {/each}
        </select>
      </label>
      <label class="field">
        <span>Host</span>
        <input bind:value={form.host} spellcheck="false" autocomplete="off" placeholder="host.example.com" />
      </label>
      <label class="field half">
        <span>User</span>
        <input bind:value={form.user} spellcheck="false" autocomplete="off" placeholder="(optional)" />
      </label>
      <label class="field half">
        <span>Port</span>
        <input bind:value={form.port} spellcheck="false" autocomplete="off" placeholder="default" />
      </label>
      <label class="field">
        <span>Remote path</span>
        <input bind:value={form.path} spellcheck="false" autocomplete="off" placeholder="/ (server root)" />
      </label>
      <div class="field">
        <span>Authentication</span>
        <div class="auth-choice">
          <label><input type="radio" bind:group={form.authKind} value="password" /> Password</label>
          <label><input type="radio" bind:group={form.authKind} value="key" /> Key file</label>
        </div>
      </div>
      {#if form.authKind === "key"}
        <label class="field">
          <span>Private key path</span>
          <div class="path-row">
            <input bind:value={form.keyPath} spellcheck="false" autocomplete="off" placeholder="~/.ssh/id_ed25519" />
            <button class="btn icon" type="button" title="Browse…" aria-label="Browse for a key file" on:click={browseKey}>
              <Icon name="folder" size={14} />
            </button>
          </div>
        </label>
      {/if}
    </div>

    {#if error}<div class="error">{error}</div>{/if}

    <div class="row">
      <button class="btn primary" on:click={save}>{editing ? "Save" : "Add"}</button>
      <button class="btn ghost" on:click={() => dispatch("close")}>Cancel</button>
    </div>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; z-index: 220; }
  .menu {
    position: fixed; width: 300px;
    background: var(--surface); color: var(--text);
    border: 1px solid var(--border-strong); border-radius: 8px;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.25); padding: 10px;
  }
  .head { font-size: 12px; color: var(--text-dim); margin-bottom: 8px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .grid { display: flex; flex-direction: column; gap: 8px; }
  .field { display: flex; flex-direction: column; gap: 3px; font-size: 11px; color: var(--text-dim); }
  .field input, .field select {
    height: 28px; padding: 0 8px; font: inherit; font-size: 13px; color: var(--text);
    border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-alt);
  }
  .field input:focus, .field select:focus { outline: none; border-color: var(--accent); }
  .field input:disabled { opacity: 0.6; }
  .auth-choice { display: flex; gap: 12px; font-size: 12px; color: var(--text); }
  .auth-choice label { display: flex; align-items: center; gap: 5px; }
  .path-row { display: flex; gap: 6px; }
  .path-row input { flex: 1 1 auto; min-width: 0; }
  .error { margin-top: 8px; font-size: 12px; color: var(--danger); }
  .row { display: flex; gap: 6px; margin-top: 10px; justify-content: flex-end; }
  .btn { height: 28px; padding: 0 10px; font: inherit; font-size: 12px; border-radius: var(--radius);
    border: 1px solid var(--border-strong); background: var(--surface-alt); color: var(--text); }
  .btn.icon { display: flex; align-items: center; justify-content: center; width: 28px; padding: 0; flex: 0 0 auto; }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
  .btn.ghost { border-color: transparent; background: transparent; color: var(--text-dim); }
  .btn:hover:not(:disabled) { filter: brightness(1.05); }
</style>
