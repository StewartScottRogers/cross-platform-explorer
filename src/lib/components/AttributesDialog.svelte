<script lang="ts">
  /**
   * File attributes editor (CPE-786, epic CPE-710). Reads each target's current attributes
   * (`read_attributes`) and applies changes via the CPE-785 write commands: on Windows the
   * readonly/hidden/system/archive flags (`set_readonly` / `set_file_attribute`); on POSIX the octal
   * permission mode (`set_permissions`); plus the modified timestamp (`set_file_times`). Supports a
   * **multi-select batch** (edited fields apply to every target), a **change preview**, and in-dialog
   * **undo** (each target's prior state is captured up front and re-applied). A thin shell over the
   * tested backend.
   */
  import { createEventDispatcher, onMount } from "svelte";
  import { invoke } from "../invoke";
  import { octalToMode, modeToSymbolic } from "../permissions";
  import { msToLocalInput, localInputToMs } from "../datetimeInput";

  interface FileAttributes {
    readonly: boolean;
    hidden: boolean;
    system: boolean;
    archive: boolean;
    mode: string | null;
  }
  interface Target { path: string; name: string; modifiedMs: number | null }
  interface Baseline { path: string; attrs: FileAttributes; modifiedMs: number | null }

  /** The selected entries to edit (one or many). */
  export let targets: Target[] = [];

  const dispatch = createEventDispatcher<{ applied: void; cancel: void }>();

  let baselines: Baseline[] = [];
  // Editable copies, seeded from the first target.
  let readonly = false;
  let hidden = false;
  let system = false;
  let archive = false;
  let mode = "";
  let modifiedInput = "";
  let seed = { readonly: false, hidden: false, system: false, archive: false, mode: "", modifiedInput: "" };
  let loading = true;
  let error = "";
  let notice = "";
  let applied = false; // post-apply state: show Undo + Close

  $: isWindows = baselines.length > 0 && baselines[0].attrs.mode === null;
  $: batch = targets.length > 1;
  $: modePreview = /^[0-7]{3,4}$/.test(mode) ? modeToSymbolic(octalToMode(mode) ?? 0) : "";

  // The set of fields the user actually changed from the seed — drives the preview + what Apply writes.
  $: changes = (() => {
    const c: string[] = [];
    if (isWindows) {
      if (readonly !== seed.readonly) c.push(`Read-only → ${readonly ? "on" : "off"}`);
      if (hidden !== seed.hidden) c.push(`Hidden → ${hidden ? "on" : "off"}`);
      if (system !== seed.system) c.push(`System → ${system ? "on" : "off"}`);
      if (archive !== seed.archive) c.push(`Archive → ${archive ? "on" : "off"}`);
    } else if (mode !== seed.mode && mode !== "") {
      c.push(`Permissions → ${mode}`);
    }
    if (modifiedInput !== seed.modifiedInput && modifiedInput !== "") c.push(`Modified → ${modifiedInput.replace("T", " ")}`);
    return c;
  })();

  onMount(async () => {
    try {
      baselines = await Promise.all(
        targets.map(async (t) => ({
          path: t.path,
          attrs: await invoke<FileAttributes>("read_attributes", { path: t.path }),
          modifiedMs: t.modifiedMs,
        })),
      );
      const b = baselines[0];
      readonly = b.attrs.readonly; hidden = b.attrs.hidden; system = b.attrs.system; archive = b.attrs.archive;
      mode = b.attrs.mode ?? "";
      modifiedInput = msToLocalInput(b.modifiedMs);
      seed = { readonly, hidden, system, archive, mode, modifiedInput };
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  });

  /** Apply the fields the user changed to a single target. Returns an error string or "".
      On Windows, writing the modified time fails on a read-only file ("Access is denied"), so read-only is
      cleared first when needed and set to its target value LAST — after the timestamp is written. */
  async function applyTo(b: Baseline): Promise<string> {
    const timesChanged = modifiedInput !== seed.modifiedInput && modifiedInput !== "";
    let ms: number | null = null;
    if (timesChanged) {
      ms = localInputToMs(modifiedInput);
      if (ms === null) return `Invalid date "${modifiedInput}"`;
    }
    try {
      if (isWindows) {
        const roChanged = readonly !== seed.readonly;
        const tempCleared = timesChanged && b.attrs.readonly; // must clear RO before writing times
        if (tempCleared) await invoke("set_readonly", { path: b.path, readonly: false });
        for (const [attr, next] of [["hidden", hidden], ["system", system], ["archive", archive]] as const) {
          if (next !== seed[attr]) await invoke("set_file_attribute", { path: b.path, attr, value: next });
        }
        if (ms !== null) await invoke("set_file_times", { path: b.path, modifiedMs: ms, accessedMs: null });
        if (roChanged || tempCleared) await invoke("set_readonly", { path: b.path, readonly });
      } else {
        if (mode !== seed.mode && mode !== "") {
          const m = octalToMode(mode);
          if (m === null) return `Invalid mode "${mode}"`;
          await invoke("set_permissions", { path: b.path, mode: m });
        }
        if (ms !== null) await invoke("set_file_times", { path: b.path, modifiedMs: ms, accessedMs: null });
      }
      return "";
    } catch (e) {
      return String(e);
    }
  }

  async function apply() {
    error = ""; notice = "";
    if (!isWindows && mode !== seed.mode && mode !== "" && octalToMode(mode) === null) {
      error = `Invalid mode "${mode}" (use octal like 644).`; return;
    }
    if (changes.length === 0) { notice = "No changes."; return; }
    const results = await Promise.all(baselines.map(applyTo));
    const failed = results.filter((r) => r !== "").length;
    if (failed === results.length) { error = results.find((r) => r) ?? "Apply failed."; return; }
    applied = true;
    notice = failed === 0
      ? `Applied to ${results.length} item${results.length === 1 ? "" : "s"}.`
      : `Applied to ${results.length - failed} of ${results.length}; ${failed} failed.`;
    dispatch("applied"); // let the parent refresh the listing
  }

  /** Revert every target to its captured baseline (attrs flags, mode, modified time). */
  async function undo() {
    error = ""; notice = "";
    for (const b of baselines) {
      try {
        if (b.attrs.mode === null) {
          // Windows: keep the file writable while restoring hidden/system/archive + the modified time,
          // then restore read-only LAST (a read-only file rejects the timestamp write).
          await invoke("set_readonly", { path: b.path, readonly: false });
          for (const attr of ["hidden", "system", "archive"] as const) {
            await invoke("set_file_attribute", { path: b.path, attr, value: b.attrs[attr] });
          }
          if (b.modifiedMs !== null) await invoke("set_file_times", { path: b.path, modifiedMs: b.modifiedMs, accessedMs: null });
          await invoke("set_readonly", { path: b.path, readonly: b.attrs.readonly });
        } else {
          const m = octalToMode(b.attrs.mode);
          if (m !== null) await invoke("set_permissions", { path: b.path, mode: m });
          if (b.modifiedMs !== null) await invoke("set_file_times", { path: b.path, modifiedMs: b.modifiedMs, accessedMs: null });
        }
      } catch (e) {
        error = String(e);
      }
    }
    applied = false;
    // Re-seed the controls to the (restored) baseline so the dialog reflects reality.
    const b0 = baselines[0];
    readonly = b0.attrs.readonly; hidden = b0.attrs.hidden; system = b0.attrs.system; archive = b0.attrs.archive;
    mode = b0.attrs.mode ?? ""; modifiedInput = msToLocalInput(b0.modifiedMs);
    seed = { readonly, hidden, system, archive, mode, modifiedInput };
    if (!error) notice = "Reverted.";
    dispatch("applied");
  }

  $: heading = batch ? `Attributes — ${targets.length} items` : `Attributes — ${targets[0]?.name ?? ""}`;
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("cancel")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => dispatch("cancel")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label="File attributes" on:click|stopPropagation>
    <h2>{heading}</h2>
    {#if batch}<p class="note">Edited fields apply to all {targets.length} selected items.</p>{/if}

    {#if loading}
      <p class="note">Reading…</p>
    {:else if error && baselines.length === 0}
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

    {#if !loading && baselines.length > 0}
      <label class="ts-row">Modified <input type="datetime-local" bind:value={modifiedInput} data-testid="attr-modified" aria-label="Modified time" /></label>
    {/if}

    {#if changes.length > 0}
      <div class="preview" data-testid="attr-preview">
        <span class="preview-h">Will change:</span>
        {#each changes as ch}<span class="chip">{ch}</span>{/each}
      </div>
    {/if}

    {#if error && baselines.length > 0}<p class="err">{error}</p>{/if}
    {#if notice}<p class="ok" data-testid="attr-notice">{notice}</p>{/if}

    <div class="actions">
      {#if applied}
        <button class="btn" data-testid="attr-undo" on:click={undo}>Undo</button>
        <button class="btn primary" on:click={() => dispatch('cancel')}>Close</button>
      {:else}
        <button class="btn" on:click={() => dispatch('cancel')}>Cancel</button>
        <button class="btn primary" data-testid="attr-apply" disabled={loading || (!!error && baselines.length === 0) || changes.length === 0} on:click={apply}>Apply</button>
      {/if}
    </div>
  </div>
</div>

<style>
  .backdrop { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.25); display: grid; place-items: center; z-index: 200; }
  .dialog { width: 400px; max-width: 92vw; background: var(--surface); border: 1px solid var(--border-strong); border-radius: 10px; box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25); padding: 20px; }
  h2 { font-size: 15px; margin-bottom: 10px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .grid { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
  .row { display: flex; align-items: center; gap: 8px; font-size: 13px; color: var(--text); }
  .posix { display: flex; align-items: center; gap: 10px; }
  .mode { width: 90px; height: 30px; padding: 0 8px; font: inherit; font-family: ui-monospace, monospace; color: var(--text); background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); }
  .sym { font-family: ui-monospace, monospace; color: var(--text-dim); }
  .ts-row { display: flex; align-items: center; gap: 8px; font-size: 13px; color: var(--text); margin-top: 12px; }
  .ts-row input { height: 30px; padding: 0 8px; font: inherit; color: var(--text); background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); }
  .preview { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; margin-top: 12px; }
  .preview-h { font-size: 12px; color: var(--text-dim); }
  .chip { flex: 0 0 auto; white-space: nowrap; padding: 1px 8px; border-radius: 999px; font-size: 11.5px; background: var(--surface-alt); border: 1px solid var(--border); color: var(--text); }
  .note, .err, .ok { font-size: 12.5px; margin-top: 10px; }
  .err { color: #c0392b; }
  .ok { color: #2e9e4f; }
  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 18px; }
  .btn { height: 32px; padding: 0 16px; border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-alt); color: var(--text); }
  .btn:disabled { opacity: 0.4; }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
</style>
