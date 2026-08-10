<script lang="ts">
  /**
   * Split file… dialog (CPE-1509, parent CPE-1491): the frontend half of the split/join feature — the
   * backend (`commands.splitFile`, CPE-1491) chunks a file into fixed-size numbered parts (`.001`,
   * `.002`, …) plus a small JSON manifest. Opened from a non-empty regular file's context menu.
   *
   * Owns its own backend call (same pattern as CreateCertDialog/VaultCreateDialog): a part-size preset
   * or free-entry MiB/GiB field (pure parsing — `parseCustomPartSize`, `splitJoin.ts`) plus a native
   * Browse output-folder picker, defaulted to the source file's own folder. On success the dialog swaps
   * to a summary panel (part count, per-part size, output folder) rather than closing immediately, so the
   * result is visible before the parent refreshes the listing — "Done" then closes it.
   *
   * Progress: split/join stream on the backend but this dialog shows only the app-wide busy cursor
   * (`invoke` from `src/lib/invoke.ts`, never `@tauri-apps/api/core` — BUSY-CURSOR.md) while a split runs,
   * not its own progress bar — an explicit first-cut simplification (Low/small scope per the ticket), not
   * an oversight. A future revisit could stream part-written events the way BatchMediaDialog does if a
   * multi-GB split's silent wait proves too opaque in practice.
   */
  import { createEventDispatcher, onMount, tick } from "svelte";
  import { open as openFolderDialog } from "@tauri-apps/plugin-dialog";
  import Icon from "./Icon.svelte";
  import { commands } from "../bindings.gen";
  import type { SplitManifest } from "../bindings.gen";
  import { unwrap } from "../invoke";
  import { formatSize } from "../format";
  import { baseName, parentDir } from "../contentSearch";
  import { PART_SIZE_PRESETS, parseCustomPartSize } from "../splitJoin";

  /** Full path of the file to split. */
  export let path: string;

  const dispatch = createEventDispatcher<{ split: { manifest: SplitManifest; outDir: string }; error: string; close: void }>();

  let presetIndex = 1; // default to the 650 MB CD preset — a reasonable middle-ground part size; -1 = custom
  let customValue = 100;
  let customUnit: "MiB" | "GiB" = "MiB";
  let outDir = parentDir(path); // defaults to the source file's own folder
  let busy = false;
  let error = "";
  let result: SplitManifest | null = null;

  $: useCustom = presetIndex === -1;
  $: partSizeBytes = useCustom ? parseCustomPartSize(customValue, customUnit) : PART_SIZE_PRESETS[presetIndex].bytes;
  $: canSplit = !busy && !!partSizeBytes && partSizeBytes > 0 && outDir.trim().length > 0;

  let firstField: HTMLElement;
  onMount(async () => {
    await tick();
    firstField?.focus();
  });

  async function browseOutDir() {
    try {
      const picked = await openFolderDialog({
        directory: true,
        multiple: false,
        defaultPath: outDir || undefined,
        title: "Choose an output folder",
      });
      if (typeof picked === "string") outDir = picked;
    } catch {
      // Cancelled or unavailable — leave the current folder untouched.
    }
  }

  async function doSplit() {
    if (!canSplit || !partSizeBytes) return;
    busy = true;
    error = "";
    try {
      const manifest = unwrap(await commands.splitFile(path, partSizeBytes, outDir));
      result = manifest;
    } catch (e) {
      error = String(e);
      dispatch("error", error);
    } finally {
      busy = false;
    }
  }

  function finish() {
    const manifest = result;
    if (!manifest) return;
    dispatch("split", { manifest, outDir });
  }
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && !busy && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => !busy && dispatch("close")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Split file" on:click|stopPropagation>
    <h2>
      <span class="hd-icon"><Icon name="cut" size={18} /></span>
      Split file
    </h2>

    {#if result}
      <p class="intro">
        Split <strong>{baseName(path)}</strong> into {result.part_count} part{result.part_count === 1 ? "" : "s"}.
      </p>
      <div class="summary" data-testid="split-summary">
        <div class="summary-row"><span class="summary-label">Parts</span><span>{result.part_count}</span></div>
        <div class="summary-row"><span class="summary-label">Part size</span><span>{formatSize(result.part_size)}</span></div>
        <div class="summary-row"><span class="summary-label">Total size</span><span>{formatSize(result.total_size)}</span></div>
        <div class="summary-row"><span class="summary-label">Output folder</span><span class="path" title={outDir}>{outDir}</span></div>
      </div>
      <div class="actions">
        <button class="btn primary" data-testid="split-done" on:click={finish}>Done</button>
      </div>
    {:else}
      <p class="intro">
        Splits <strong>{baseName(path)}</strong> into fixed-size numbered parts (<code>.001</code>,
        <code>.002</code>, …) plus a small manifest, for transferring or storing a large file somewhere
        that caps individual file sizes.
      </p>

      <label class="field-label" for="split-preset">Part size</label>
      <select
        id="split-preset"
        class="field"
        bind:this={firstField}
        bind:value={presetIndex}
        disabled={busy}
        data-testid="split-preset"
      >
        {#each PART_SIZE_PRESETS as p, i}
          <option value={i}>{p.label}</option>
        {/each}
        <option value={-1}>Custom…</option>
      </select>

      {#if useCustom}
        <div class="custom-row">
          <input
            class="field num"
            type="number"
            min="0"
            step="any"
            bind:value={customValue}
            disabled={busy}
            aria-label="Custom part size"
            data-testid="split-custom-value"
          />
          <select
            class="field unit"
            bind:value={customUnit}
            disabled={busy}
            aria-label="Custom part size unit"
            data-testid="split-custom-unit"
          >
            <option value="MiB">MiB</option>
            <option value="GiB">GiB</option>
          </select>
        </div>
      {/if}

      <label class="field-label" for="split-outdir">Output folder</label>
      <div class="dest-row">
        <input
          id="split-outdir"
          class="field"
          type="text"
          bind:value={outDir}
          disabled={busy}
          spellcheck="false"
          title={outDir}
          data-testid="split-outdir"
        />
        <button class="btn" type="button" disabled={busy} on:click={browseOutDir} data-testid="split-outdir-browse">
          Browse…
        </button>
      </div>

      {#if error}<div class="err" data-testid="split-error">{error}</div>{/if}

      <div class="actions">
        <button class="btn" data-testid="split-cancel" disabled={busy} on:click={() => dispatch("close")}>
          Cancel
        </button>
        <button class="btn primary" data-testid="split-confirm" disabled={!canSplit} on:click={doSplit}>
          {busy ? "Splitting…" : "Split"}
        </button>
      </div>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.25);
    display: grid;
    place-items: center;
    z-index: 200;
  }
  .dialog {
    width: 480px;
    max-width: 90vw;
    max-height: 90vh;
    overflow-y: auto;
    background: var(--surface);
    border: 1px solid var(--dialog-border);
    border-radius: 10px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25);
    padding: 20px;
  }
  h2 {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 16px;
    margin-bottom: 10px;
  }
  .hd-icon { color: var(--text); display: grid; place-items: center; }
  .intro { color: var(--text-dim); font-size: 12.5px; line-height: 1.5; margin-bottom: 14px; }
  .intro strong { color: var(--text); }
  .field-label {
    display: block;
    font-size: 12px;
    color: var(--text-dim);
    margin-bottom: 4px;
  }
  .field {
    width: 100%;
    height: 32px;
    padding: 0 8px;
    font: inherit;
    color: var(--text);
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-sizing: border-box;
    margin-bottom: 12px;
  }
  select.field { cursor: pointer; }
  .custom-row { display: flex; gap: 8px; margin-top: -6px; }
  .custom-row .num { flex: 1 1 auto; }
  .custom-row .unit { flex: 0 0 90px; }
  .dest-row { display: flex; gap: 8px; align-items: flex-start; }
  .dest-row .field { flex: 1 1 auto; min-width: 0; text-overflow: ellipsis; }
  .dest-row .btn { flex: 0 0 auto; }
  .err { font-size: 12.5px; font-weight: 600; color: var(--text); margin: -6px 0 12px; }
  .summary {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 10px 12px;
    margin-bottom: 14px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .summary-row { display: flex; justify-content: space-between; gap: 12px; font-size: 12.5px; }
  .summary-label { color: var(--text-dim); }
  .summary-row .path {
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 280px;
  }
  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 6px; }
  .btn {
    height: 32px;
    padding: 0 16px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--surface-alt);
    color: var(--text);
  }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
  .btn.primary:hover:not(:disabled) { background: var(--accent-hover); }
  .btn:disabled { opacity: 0.6; cursor: default; }
</style>
