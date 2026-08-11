<script lang="ts">
  /**
   * "Securely delete…" (shred) confirm dialog (CPE-1240, epic CPE-738).
   *
   * The one destructive action in this app with NO trash fallback — `shred_paths` overwrites a file's
   * bytes then unlinks it, so there is nothing to restore afterward. That means this dialog carries all
   * the safety weight `ConfirmDialog` usually shares with "it's recoverable from the Bin". It must state,
   * plainly and with no false comfort:
   *   1. PERMANENT / NON-RECOVERABLE — unlike Delete, this never goes to the Recycle Bin / Trash.
   *   2. The honest platform caveat the engine itself ships (`secure_delete::plan_shred`'s caveat text,
   *      mirrored here): overwriting is BEST-EFFORT, not a guarantee — SSD wear-levelling, copy-on-write
   *      filesystems (APFS/Btrfs/ZFS snapshots), and journaling can all leave remnants behind. The Rust
   *      engine currently always plans for the conservative "plain disk, in place" case (v1 scope, see
   *      `secure_shred`'s module doc) — so this dialog surfaces ALL three caveat branches
   *      `plan_shred` can produce, rather than silently assuming the friendliest one.
   * No stronger claim than that is made anywhere in this file.
   *
   * Owns its own backend call (same pattern as `RepairLinkDialog`/`NewLinkDialog`): picks a scheme,
   * requires the explicit danger-button confirm (the safeguard called for since there's no trash
   * fallback), calls `commands.shredPaths`, and dispatches `done` with the per-path results for the
   * caller to summarize + refresh the listing.
   *
   * **CPE-1611:** the backend engine (`secure_shred::shred_paths`) now refuses the whole call unless
   * `confirmed: true` is passed — this dialog's `confirmShred`, fired only by the "Shred permanently"
   * button below, is the ONE place in the codebase allowed to pass it. That closes the gap where the
   * dialog was a pure frontend invariant: a devtools call or a future automation surface could invoke
   * `shred_paths` directly and skip this confirm entirely.
   */
  import { createEventDispatcher } from "svelte";
  import Icon from "./Icon.svelte";
  import { commands } from "../bindings.gen";
  import type { ShredScheme, ShredResult } from "../bindings.gen";

  /** Full paths to shred. */
  export let paths: string[] = [];
  /** Display label for the header/body, e.g. `"report.txt"` or `"3 items"` — computed by the caller,
   *  same convention as the plain `ConfirmDialog`'s delete-permanently flow. */
  export let what = "";

  const dispatch = createEventDispatcher<{ done: ShredResult[]; error: string; close: void }>();

  const SCHEMES: { value: ShredScheme; label: string; hint: string }[] = [
    { value: "zero", label: "Zero-fill", hint: "1 pass — fast" },
    { value: "random", label: "Random", hint: "1 pass" },
    { value: "dod_3", label: "DoD 5220.22-M", hint: "3 passes — zeros, ones, random" },
    { value: "gutmann", label: "Gutmann", hint: "7 passes — slowest, most thorough" },
  ];

  let scheme: ShredScheme = "zero";
  let busy = false;
  let error = "";

  async function confirmShred() {
    if (paths.length === 0 || busy) return;
    busy = true;
    error = "";
    try {
      const res = await commands.shredPaths(paths, scheme, true);
      if (res.status === "ok") {
        dispatch("done", res.data);
      } else {
        const message = String(res.error);
        error = message;
        dispatch("error", message);
      }
    } catch (e) {
      const message = String(e);
      error = message;
      dispatch("error", message);
    } finally {
      busy = false;
    }
  }
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && !busy && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => !busy && dispatch("close")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Securely delete?" on:click|stopPropagation>
    <h2><span class="warn"><Icon name="delete" size={18} /></span> Securely delete {what}?</h2>

    <p class="permanence" data-testid="shred-permanence">
      This is <strong>permanent and non-recoverable</strong>. Unlike Delete, it does <strong>not</strong>
      go to the Recycle Bin / Trash — there is no undo and no trash fallback once you confirm.
    </p>

    <div class="caveat" data-testid="shred-caveat">
      <span class="caveat-icon"><Icon name="lock" size={14} /></span>
      <p>
        Overwriting is <strong>best-effort, not a guarantee</strong>. On an SSD or other flash storage,
        wear-levelling means the original cells may not actually be erased. On a copy-on-write filesystem
        (e.g. APFS, Btrfs, ZFS), overwriting writes new blocks and old data can remain in snapshots. Even
        on a conventional disk, copies left in backups, temp files, or filesystem journals are not
        touched. For guaranteed erasure, use full-disk encryption or an encrypted vault.
      </p>
    </div>

    <label class="field-label" for="shred-scheme">Overwrite scheme</label>
    <select id="shred-scheme" class="scheme-select" bind:value={scheme} disabled={busy} data-testid="shred-scheme">
      {#each SCHEMES as s}
        <option value={s.value}>{s.label} — {s.hint}</option>
      {/each}
    </select>

    {#if error}<div class="err" data-testid="shred-error">{error}</div>{/if}

    <div class="actions">
      <button class="btn" data-testid="shred-cancel" disabled={busy} on:click={() => dispatch("close")}>
        Cancel
      </button>
      <button class="btn primary danger" data-testid="shred-confirm" disabled={busy || paths.length === 0} on:click={confirmShred}>
        {busy ? "Shredding…" : "Shred permanently"}
      </button>
    </div>
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
    margin-bottom: 12px;
  }
  .warn { color: var(--danger); display: grid; place-items: center; }
  .permanence { color: var(--text); margin-bottom: 12px; line-height: 1.5; font-size: 12.5px; }
  .caveat {
    display: flex;
    gap: 8px;
    background: var(--surface-alt);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 10px;
    margin-bottom: 14px;
  }
  .caveat-icon { color: var(--text-dim); flex: 0 0 auto; margin-top: 2px; }
  .caveat p { color: var(--text-dim); font-size: 12px; line-height: 1.5; margin: 0; }
  .field-label {
    display: block;
    font-size: 12px;
    color: var(--text-dim);
    margin-bottom: 4px;
  }
  .scheme-select {
    width: 100%;
    height: 32px;
    padding: 0 8px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--surface-alt);
    color: var(--text);
  }
  .err { margin-top: 10px; font-size: 12.5px; font-weight: 600; color: var(--text); }
  .actions { display: flex; justify-content: flex-end; gap: 8px; margin-top: 18px; }
  .btn {
    height: 32px;
    padding: 0 16px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--surface-alt);
    color: var(--text);
  }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
  .btn.primary:hover { background: var(--accent-hover); }
  .btn.primary.danger { background: var(--danger); border-color: var(--danger); }
  .btn.primary.danger:hover { background: var(--danger-hover); }
  .btn:disabled { opacity: 0.6; }
</style>
