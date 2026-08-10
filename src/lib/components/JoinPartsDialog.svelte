<script lang="ts">
  /**
   * Join parts… dialog (CPE-1509, parent CPE-1491): the frontend half of the split/join feature — the
   * backend (`commands.joinFiles`, CPE-1491) rejoins a manifest's numbered parts back into the original
   * file, verifying the reconstructed SHA-256. Opened from a `.NNN` part's or a `.split-manifest.json`'s
   * context menu.
   *
   * On mount it best-effort `readFileText`s the manifest (`manifestPathFor`, `splitJoin.ts`, mirrors the
   * backend's own `resolve_manifest_path`) to show a small preview (part count, total size) and pre-fill
   * the output path with the manifest's `original_name` in the same folder — a native Browse (save)
   * picker lets the user pick anywhere else. If the manifest can't be read/parsed yet, the output path
   * falls back to a name GUESSED from the clicked file alone (`guessOriginalName`); either way the real,
   * disk-validating check happens in `joinFiles` itself, so a wrong guess here is never silently wrong —
   * it's just a starting point the user can edit or Browse away from.
   *
   * Error surfacing (ticket requirement): every backend error — corrupt/missing part, checksum mismatch,
   * or the overwrite-refusal when `out_path` already exists — reaches the user as this dialog's visible
   * `error` banner, never a swallowed promise rejection. No in-dialog "replace existing" confirmation in
   * this first cut (ticket's explicitly-acceptable scope): a bare error message + edit-the-path-or-delete-
   * the-target-first is the whole recovery path for now.
   */
  import { createEventDispatcher, onMount, tick } from "svelte";
  import { save as saveFileDialog } from "@tauri-apps/plugin-dialog";
  import Icon from "./Icon.svelte";
  import { commands } from "../bindings.gen";
  import { unwrap } from "../invoke";
  import { formatSize } from "../format";
  import { baseName } from "../contentSearch";
  import { manifestPathFor, defaultJoinOutputPath, guessOriginalName } from "../splitJoin";

  /** Full path of the clicked part or manifest file. */
  export let path: string;

  const dispatch = createEventDispatcher<{ joined: string; error: string; close: void }>();

  /** Manifest preview, once (if) it loads — part count/total size shown above the output-path field.
   *  `null` while loading or when the read/parse failed; the Join button doesn't depend on this loading
   *  successfully (the backend re-validates the manifest for real on join). */
  let preview: { partCount: number; totalSize: number } | null = null;
  let previewError = "";

  let outPath = defaultJoinOutputPath(path, guessOriginalName(baseName(path)));
  let outEdited = false;
  let busy = false;
  let error = "";
  let joinedPath = "";

  const MAX_MANIFEST_BYTES = 65536; // generous cap for a small JSON manifest — mirrors the backend's own sanity cap

  onMount(async () => {
    await loadManifestPreview();
    await tick();
    document.getElementById("join-outpath")?.focus();
  });

  async function loadManifestPreview() {
    const manifestPath = manifestPathFor(path);
    try {
      const res = await commands.readFileText(manifestPath, MAX_MANIFEST_BYTES);
      if (res.status !== "ok") {
        previewError = res.error;
        return;
      }
      const m = JSON.parse(res.data) as {
        original_name?: string;
        total_size?: number;
        part_count?: number;
      };
      if (typeof m.original_name === "string") {
        preview = { partCount: m.part_count ?? 0, totalSize: m.total_size ?? 0 };
        if (!outEdited) outPath = defaultJoinOutputPath(path, m.original_name);
      } else {
        previewError = "Manifest is missing expected fields.";
      }
    } catch (e) {
      // Not fatal — the dialog still works off the guessed default name, and the real backend `joinFiles`
      // call re-validates the manifest from scratch (corrupt/missing manifest surfaces there instead).
      previewError = String(e);
    }
  }

  $: canJoin = !busy && outPath.trim().length > 0;

  async function browseOut() {
    try {
      const picked = await saveFileDialog({
        defaultPath: outPath || undefined,
        title: "Choose where to write the rejoined file",
      });
      if (picked) {
        outPath = picked;
        outEdited = true;
      }
    } catch {
      // Cancelled or unavailable — leave the current value untouched.
    }
  }

  async function doJoin() {
    if (!canJoin) return;
    busy = true;
    error = "";
    try {
      unwrap(await commands.joinFiles(path, outPath));
      joinedPath = outPath;
    } catch (e) {
      error = String(e);
      dispatch("error", error);
    } finally {
      busy = false;
    }
  }

  function finish() {
    if (joinedPath) dispatch("joined", joinedPath);
  }
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && !busy && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => !busy && dispatch("close")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Join parts" on:click|stopPropagation>
    <h2>
      <span class="hd-icon"><Icon name="link" size={18} /></span>
      Join parts
    </h2>

    {#if joinedPath}
      <p class="intro">Rejoined into <strong>{baseName(joinedPath)}</strong>, checksum verified.</p>
      <div class="summary" data-testid="join-summary">
        <div class="summary-row"><span class="summary-label">Output file</span><span class="path" title={joinedPath}>{joinedPath}</span></div>
      </div>
      <div class="actions">
        <button class="btn primary" data-testid="join-done" on:click={finish}>Done</button>
      </div>
    {:else}
      <p class="intro">
        Rejoins <strong>{baseName(path)}</strong>'s numbered parts back into the original file, verifying
        the reconstructed checksum against the manifest.
      </p>

      {#if preview}
        <div class="summary" data-testid="join-preview">
          <div class="summary-row"><span class="summary-label">Parts</span><span>{preview.partCount}</span></div>
          <div class="summary-row"><span class="summary-label">Total size</span><span>{formatSize(preview.totalSize)}</span></div>
        </div>
      {:else if previewError}
        <div class="warn" data-testid="join-preview-error">
          Couldn't preview the manifest ({previewError}) — Join will still validate it for real.
        </div>
      {/if}

      <label class="field-label" for="join-outpath">Output path</label>
      <div class="dest-row">
        <input
          id="join-outpath"
          class="field"
          type="text"
          value={outPath}
          on:input={(e) => { outEdited = true; outPath = e.currentTarget.value; }}
          disabled={busy}
          spellcheck="false"
          title={outPath}
          data-testid="join-outpath"
        />
        <button class="btn" type="button" disabled={busy} on:click={browseOut} data-testid="join-outpath-browse">
          Browse…
        </button>
      </div>

      {#if error}<div class="err" data-testid="join-error">{error}</div>{/if}

      <div class="actions">
        <button class="btn" data-testid="join-cancel" disabled={busy} on:click={() => dispatch("close")}>
          Cancel
        </button>
        <button class="btn primary" data-testid="join-confirm" disabled={!canJoin} on:click={doJoin}>
          {busy ? "Joining…" : "Join"}
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
  .dest-row { display: flex; gap: 8px; align-items: flex-start; }
  .dest-row .field { flex: 1 1 auto; min-width: 0; text-overflow: ellipsis; }
  .dest-row .btn { flex: 0 0 auto; }
  .err { font-size: 12.5px; font-weight: 600; color: var(--text); margin: -6px 0 12px; }
  .warn { font-size: 12px; color: var(--text-dim); margin-bottom: 12px; }
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
    max-width: 300px;
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
