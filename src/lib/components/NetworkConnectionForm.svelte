<script lang="ts">
  /** "+ Add a connection" / Edit inline form for the Network sidebar section (CPE-1513, epic CPE-1498) — an
      inline/instant control ([[prefer-inline-instant-controls]]), not a modal: protocol dropdown
      (sftp/webdav/smb/ftp/s3), host, optional user/port/path, and (for key auth) a key-file field with a
      native Browse picker ([[path-inputs-need-picker]]). Field labels follow the protocol — for `s3`
      (CPE-1686, epic CPE-1503) the same inputs are the endpoint, region and bucket/prefix, and the auth
      choice is an access key ID whose *secret* half is never typed here: it's collected by the existing
      connect-time NetworkSecretPrompt and stored in the OS keychain (CPE-1510), exactly like a password. Mirrors SmartFolderMenu/NetworkSecretPrompt's popover shape — a
      clearly-bordered box anchored at the trigger's click point ([[dialogs-need-visible-border]]), not a
      full-screen dialog. Pure validation lives in `network.ts`'s `buildConnection` (unit-tested); this
      component only wires the fields to it and shows the returned error string inline. */
  import { createEventDispatcher, onMount } from "svelte";
  import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
  import Icon from "./Icon.svelte";
  import {
    SUPPORTED_SCHEMES,
    authKindsFor,
    blankConnectionForm,
    buildConnection,
    coerceAuthKind,
    schemeFieldHints,
    type ConnectionFormInput,
    type FormAuthKind,
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

  /** Field labels/placeholders follow the protocol (CPE-1686): for `s3` the same three inputs mean
   *  endpoint / region / bucket-and-prefix. The mapping is pure and unit-tested in `network.ts`. */
  $: hints = schemeFieldHints(form.scheme);
  /** Only the auth kinds the chosen protocol can actually use (S3 signs with an access key; the SSH/HTTP
   *  providers reject access keys), so the form can't save a profile that could only fail at connect. */
  $: allowedAuthKinds = authKindsFor(form.scheme);
  const AUTH_LABELS: Record<FormAuthKind, string> = {
    password: "Password",
    key: "Key file",
    access_key: "Access key",
  };

  /** Switching protocol snaps the auth choice back to something that protocol supports. Done on `change`
   *  rather than in a `$:` so the assignment can't feed back into its own reactive dependency. */
  function onSchemeChange() {
    form.authKind = coerceAuthKind(form.scheme, form.authKind);
  }

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
        <select bind:value={form.scheme} on:change={onSchemeChange}>
          {#each SUPPORTED_SCHEMES as s (s)}
            <option value={s}>{s}</option>
          {/each}
        </select>
      </label>
      <label class="field">
        <span>{hints.hostLabel}</span>
        <input bind:value={form.host} spellcheck="false" autocomplete="off" placeholder={hints.hostPlaceholder} />
      </label>
      <label class="field half">
        <span>{hints.userLabel}</span>
        <input bind:value={form.user} spellcheck="false" autocomplete="off" placeholder={hints.userPlaceholder} />
      </label>
      <label class="field half">
        <span>Port</span>
        <input bind:value={form.port} spellcheck="false" autocomplete="off" placeholder="default" />
      </label>
      <label class="field">
        <span>{hints.pathLabel}</span>
        <input bind:value={form.path} spellcheck="false" autocomplete="off" placeholder={hints.pathPlaceholder} />
      </label>
      <div class="field">
        <span>Authentication</span>
        <div class="auth-choice">
          {#each allowedAuthKinds as kind (kind)}
            <label><input type="radio" bind:group={form.authKind} value={kind} /> {AUTH_LABELS[kind]}</label>
          {/each}
        </div>
      </div>
      {#if form.authKind === "access_key"}
        <label class="field">
          <span>Access key ID</span>
          <input bind:value={form.accessKeyId} spellcheck="false" autocomplete="off" placeholder="AKIA…" />
        </label>
        <div class="note">
          The secret access key isn’t stored here — you’re asked for it when you first connect, and it goes
          straight to your operating system’s keychain.
        </div>
      {/if}
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
  /* Reflows onto more rows as auth kinds are added, rather than overflowing ([[tick-tacks-reflow]]). */
  .auth-choice { display: flex; flex-wrap: wrap; gap: 6px 12px; font-size: 12px; color: var(--text); }
  .auth-choice label { display: flex; align-items: center; gap: 5px; white-space: nowrap; flex: 0 0 auto; }
  .note { font-size: 11px; line-height: 1.35; color: var(--text-dim); }
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
