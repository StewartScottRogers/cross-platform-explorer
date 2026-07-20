<script lang="ts">
  /**
   * File attributes editor (CPE-786, epic CPE-710). Reads a file's current attributes (`read_attributes`)
   * and applies changes via the CPE-785 write commands: on Windows the readonly/hidden/system/archive
   * flags (`set_readonly` / `set_file_attribute`); on POSIX the octal permission mode (`set_permissions`).
   * Single-file v1 — batch/timestamps/undo are follow-ups. A thin shell over the tested backend.
   */
  import { createEventDispatcher, onMount } from "svelte";
  import { invoke } from "../invoke";
  import { octalToMode, modeToSymbolic } from "../permissions";

  interface FileAttributes {
    readonly: boolean;
    hidden: boolean;
    system: boolean;
    archive: boolean;
    mode: string | null;
  }

  export let path = "";
  export let name = "";

  const dispatch = createEventDispatcher<{ applied: void; cancel: void }>();

  let attrs: FileAttributes | null = null;
  // Editable copies.
  let readonly = false;
  let hidden = false;
  let system = false;
  let archive = false;
  let mode = "";
  let loading = true;
  let error = "";
  let notice = "";

  $: isWindows = attrs !== null && attrs.mode === null;
  $: modePreview = /^[0-7]{3,4}$/.test(mode) ? modeToSymbolic(octalToMode(mode) ?? 0) : "";

  onMount(async () => {
    try {
      attrs = await invoke<FileAttributes>("read_attributes", { path });
      readonly = attrs.readonly;
      hidden = attrs.hidden;
      system = attrs.system;
      archive = attrs.archive;
      mode = attrs.mode ?? "";
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  });

  async function apply() {
    if (!attrs) return;
    error = ""; notice = "";
    try {
      if (isWindows) {
        if (readonly !== attrs.readonly) await invoke("set_readonly", { path, readonly });
        for (const [attr, next, prev] of [
          ["hidden", hidden, attrs.hidden],
          ["system", system, attrs.system],
          ["archive", archive, attrs.archive],
        ] as const) {
          if (next !== prev) await invoke("set_file_attribute", { path, attr, value: next });
        }
      } else {
        const m = octalToMode(mode);
        if (m === null) { error = `Invalid mode "${mode}" (use octal like 644).`; return; }
        await invoke("set_permissions", { path, mode: m });
      }
      notice = "Applied.";
      dispatch("applied");
    } catch (e) {
      error = String(e);
    }
  }
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("cancel")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("cancel")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label="File attributes" on:click|stopPropagation>
    <h2>Attributes — {name}</h2>

    {#if loading}
      <p class="note">Reading…</p>
    {:else if error && !attrs}
      <p class="err">{error}</p>
    {:else if isWindows}
      <div class="grid" data-testid="attr-grid">
        <label class="row"><input type="checkbox" bind:checked={readonly} data-testid="attr-readonly" /> Read-only</label>
        <label class="row"><input type="checkbox" bind:checked={hidden} data-testid="attr-hidden" /> Hidden</label>
        <label class="row"><input type="checkbox" bind:checked={system} data-testid="attr-system" /> System</label>
        <label class="row"><input type="checkbox" bind:checked={archive} data-testid="attr-archive" /> Archive</label>
      </div>
    {:else}
      <div class="posix" data-testid="attr-posix">
        <label class="row">Permissions (octal) <input class="mode" bind:value={mode} data-testid="attr-mode" aria-label="Octal mode" /></label>
        {#if modePreview}<span class="sym" data-testid="attr-sym">{modePreview}</span>{/if}
      </div>
    {/if}

    {#if error && attrs}<p class="err">{error}</p>{/if}
    {#if notice}<p class="ok" data-testid="attr-notice">{notice}</p>{/if}

    <div class="actions">
      <button class="btn" on:click={() => dispatch('cancel')}>Cancel</button>
      <button class="btn primary" data-testid="attr-apply" disabled={loading || (!!error && !attrs)} on:click={apply}>Apply</button>
    </div>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.25); display: grid; place-items: center; z-index: 200; }
  .dialog { width: 380px; max-width: 92vw; background: var(--surface); border: 1px solid var(--border-strong); border-radius: 10px; box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25); padding: 20px; }
  h2 { font-size: 15px; margin-bottom: 14px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
  .row { display: flex; align-items: center; gap: 8px; font-size: 13px; color: var(--text); }
  .posix { display: flex; align-items: center; gap: 10px; }
  .mode { width: 90px; height: 30px; padding: 0 8px; font: inherit; font-family: ui-monospace, monospace; color: var(--text); background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); }
  .sym { font-family: ui-monospace, monospace; color: var(--text-dim); }
  .note, .err, .ok { font-size: 12.5px; margin-top: 10px; }
  .err { color: #c0392b; }
  .ok { color: #2e9e4f; }
  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 18px; }
  .btn { height: 32px; padding: 0 16px; border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-alt); color: var(--text); }
  .btn:disabled { opacity: 0.4; }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
</style>
