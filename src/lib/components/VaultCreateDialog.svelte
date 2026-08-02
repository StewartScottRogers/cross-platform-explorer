<script lang="ts">
  /**
   * "Create encrypted vault…" dialog (CPE-1250, epic CPE-738) — the CREATE side of the vaults feature
   * (unlock/browse/lock shipped in CPE-1249). Seals a folder into a `.cpevault` blob via the merged
   * backend (`vault_create`, CPE-1248), with the optional DESTRUCTIVE "securely delete the original"
   * path behind an honest confirm.
   *
   * Owns its own backend call (same pattern as ShredConfirmDialog / RepairLinkDialog): validates locally
   * (passphrase must match its confirmation — there is NO recovery if mistyped), calls `vaultCreate`,
   * optionally remembers the passphrase in the OS keychain, and dispatches `created` (with the new blob
   * path) / `error` / `close`. The passphrase lives in this component's memory only — it flows straight
   * into the backend command (and, if opted in, `vaultRememberPassphrase`) and is never stored elsewhere
   * or logged (mirrors vaultStore.ts + vault_manager.rs).
   *
   * Destructive-path safety: "Securely delete the original" defaults OFF. When ON, the honest warning
   * (reusing ShredConfirmDialog's permanent / best-effort-on-SSD/CoW tone) is shown — and the backend
   * only shreds AFTER a full decrypt round-trip proves the encrypted copy is recoverable
   * (verify-before-shred, vault_manager::create_vault). The default destination is a SIBLING of the
   * folder, never inside it, because the backend refuses an inside-the-folder dest when shredding.
   */
  import { createEventDispatcher, onMount, tick } from "svelte";
  import { save as saveFileDialog } from "@tauri-apps/plugin-dialog";
  import Icon from "./Icon.svelte";
  import { commands } from "../bindings.gen";
  import { unwrap } from "../invoke";
  import { siblingVaultDest, checkPassphrases, canCreate, VAULT_EXT } from "../vaultCreate";

  /** Absolute path of the folder to seal. */
  export let folderPath: string;
  /** Display name of the folder (for the header/labels). */
  export let folderName = "";
  /** Default state of "Remember passphrase in the keychain" — driven by the Settings preference. */
  export let rememberDefault = false;

  const dispatch = createEventDispatcher<{ created: string; error: string; close: void }>();

  let passphrase = "";
  let confirm = "";
  let dest = siblingVaultDest(folderPath);
  let shredOriginal = false;
  let remember = rememberDefault;
  let busy = false;
  let error = ""; // backend error surfaced after a failed create

  let passField: HTMLInputElement;
  onMount(async () => {
    await tick();
    passField?.focus();
  });

  // Live confirm-mismatch feedback: only once the user has typed something into the confirm field, so an
  // untouched form isn't pre-decorated with an error. `checkPassphrases` is the pure, unit-tested gate.
  $: mismatch = confirm.length > 0 && !checkPassphrases(passphrase, confirm).ok;
  $: creatable = canCreate({ passphrase, confirm, dest, busy });

  async function browseDest() {
    const suggested = dest || siblingVaultDest(folderPath);
    const picked = await saveFileDialog({
      defaultPath: suggested,
      filters: [{ name: "Encrypted vault", extensions: [VAULT_EXT.replace(/^\./, "")] }],
    });
    if (picked) dest = picked;
  }

  async function create() {
    if (!creatable || busy) return;
    busy = true;
    error = "";
    try {
      unwrap(await commands.vaultCreate(folderPath, dest, passphrase, shredOriginal));
      // Opt-in keychain persistence — keyed to the NEW blob path, exactly what CPE-1249's unlock looks up.
      if (remember) unwrap(await commands.vaultRememberPassphrase(dest, passphrase));
      dispatch("created", dest);
    } catch (e) {
      // Surface the backend VaultError clearly (e.g. the dest-inside-folder refusal or an unsealable
      // filename — both come back as a `Format(...)` string). Stay open so the user can adjust + retry.
      error = String(e);
      dispatch("error", error);
    } finally {
      busy = false;
    }
  }
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && !busy && dispatch("close")} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={() => !busy && dispatch("close")}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Create encrypted vault" on:click|stopPropagation>
    <h2>
      <span class="hd-icon"><Icon name="lock" size={18} /></span>
      Create encrypted vault{folderName ? ` from "${folderName}"` : ""}
    </h2>

    <p class="intro" data-testid="vault-intro">
      Seals this folder into a single encrypted <code>{VAULT_EXT}</code> file that can only be opened with
      the passphrase you choose.
    </p>

    <label class="field-label" for="vault-passphrase">Passphrase</label>
    <input
      id="vault-passphrase"
      class="field"
      type="password"
      bind:this={passField}
      bind:value={passphrase}
      disabled={busy}
      autocomplete="new-password"
      data-testid="vault-passphrase"
    />

    <label class="field-label" for="vault-passphrase-confirm">Confirm passphrase</label>
    <input
      id="vault-passphrase-confirm"
      class="field"
      type="password"
      bind:value={confirm}
      disabled={busy}
      autocomplete="new-password"
      data-testid="vault-passphrase-confirm"
    />

    {#if mismatch}
      <div class="err" data-testid="vault-passphrase-error">The passphrases don't match.</div>
    {/if}

    <p class="warn-note" data-testid="vault-passphrase-note">
      <strong>There is no way to recover a forgotten passphrase</strong> — if you lose it, the contents
      are gone for good. That's the point of a vault; keep the passphrase somewhere safe.
    </p>

    <label class="field-label" for="vault-dest">Vault file</label>
    <div class="dest-row">
      <input
        id="vault-dest"
        class="field"
        type="text"
        bind:value={dest}
        disabled={busy}
        spellcheck="false"
        data-testid="vault-dest"
      />
      <button class="btn" type="button" disabled={busy} on:click={browseDest} data-testid="vault-dest-browse">
        Browse…
      </button>
    </div>

    <label class="check-row">
      <input
        type="checkbox"
        bind:checked={shredOriginal}
        disabled={busy}
        data-testid="vault-shred"
      />
      <span>Securely delete the original folder after sealing</span>
    </label>

    {#if shredOriginal}
      <div class="caveat" data-testid="vault-shred-warning">
        <span class="caveat-icon"><Icon name="delete" size={14} /></span>
        <div>
          <p class="permanence">
            The original folder will be <strong>permanently deleted</strong> — it does <strong>not</strong>
            go to the Recycle Bin / Trash. This only happens <strong>after</strong> the encrypted vault is
            verified to open, so the data is never lost; but once it runs there is no undo.
          </p>
          <p class="best-effort">
            Overwriting the old files is <strong>best-effort, not a guarantee</strong>. On an SSD or other
            flash storage, wear-levelling means the original bytes may not truly be erased; on a
            copy-on-write filesystem (APFS, Btrfs, ZFS) old data can remain in snapshots; copies in
            backups, temp files, or filesystem journals are not touched.
          </p>
        </div>
      </div>
    {/if}

    <label class="check-row">
      <input
        type="checkbox"
        bind:checked={remember}
        disabled={busy}
        data-testid="vault-remember"
      />
      <span>Remember this passphrase in this device's keychain</span>
    </label>

    {#if error}<div class="err create-err" data-testid="vault-create-error">{error}</div>{/if}

    <div class="actions">
      <button class="btn" data-testid="vault-cancel" disabled={busy} on:click={() => dispatch("close")}>
        Cancel
      </button>
      <button
        class="btn primary"
        data-testid="vault-create-confirm"
        disabled={!creatable}
        on:click={create}
      >
        {busy ? "Creating…" : "Create vault"}
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
  .intro code { font-family: var(--mono, monospace); font-size: 12px; }
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
  .dest-row .field { flex: 1 1 auto; }
  .dest-row .btn { flex: 0 0 auto; }
  .warn-note { color: var(--text-dim); font-size: 12px; line-height: 1.5; margin: 2px 0 14px; }
  .warn-note strong { color: var(--text); }
  .check-row {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    font-size: 12.5px;
    color: var(--text);
    margin-bottom: 12px;
    cursor: pointer;
  }
  .check-row input { margin-top: 2px; flex: 0 0 auto; }
  .caveat {
    display: flex;
    gap: 8px;
    background: var(--surface-alt);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 10px;
    margin: -4px 0 14px;
  }
  .caveat-icon { color: var(--text-dim); flex: 0 0 auto; margin-top: 2px; }
  .caveat p { font-size: 12px; line-height: 1.5; margin: 0; }
  .caveat .permanence { color: var(--text); }
  .caveat .best-effort { color: var(--text-dim); margin-top: 8px; }
  .err { font-size: 12.5px; font-weight: 600; color: var(--text); margin: -6px 0 12px; }
  .create-err { margin-top: 4px; }
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
  .btn.primary:hover { background: var(--accent-hover); }
  .btn:disabled { opacity: 0.6; cursor: default; }
</style>
