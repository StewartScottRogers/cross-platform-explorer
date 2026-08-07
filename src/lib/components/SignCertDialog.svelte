<script lang="ts">
  /**
   * Sign / issue from CSR… dialog (CPE-1423, epic CPE-1417): the ISSUE side of certificate management —
   * parses a PKCS#10 CSR and issues a leaf X.509 certificate for it, signed by an existing CA's
   * cert + private key, via the shipped `cert_issue_from_csr` backend (CPE-1421). Opened from the
   * pane-aware context menu's "Issue cert from this CSR…" (pre-fills `csrPath` from the clicked `.csr`
   * file) / "Sign with this as CA…" (pre-fills `caCertPath` from the clicked cert file) — CPE-1424 — or
   * the command palette with nothing pre-filled. Every path field has a native Browse picker (memory:
   * every path field needs one): three `open` file pickers for the CSR/CA cert/CA key, and a `save`
   * picker for the output cert path, matching VaultCreateDialog's `browseDest` convention.
   *
   * Owns its own backend call: validates locally (every path set + a positive validity —
   * `canSignCert`, `certCreate.ts`), calls `commands.certIssueFromCsr`, and dispatches `created` (the
   * issued cert's full path) / `error` / `close`.
   */
  import { createEventDispatcher, onMount, tick } from "svelte";
  import { open as openFileDialog, save as saveFileDialog } from "@tauri-apps/plugin-dialog";
  import Icon from "./Icon.svelte";
  import { commands } from "../bindings.gen";
  import { unwrap } from "../invoke";
  import { joinPath, defaultIssuedCertName, canSignCert } from "../certCreate";

  /** Pre-filled from a clicked `.csr` file ("Issue cert from this CSR…"), else "". */
  export let csrPath = "";
  /** Pre-filled from a clicked cert file ("Sign with this as CA…"), else "". */
  export let caCertPath = "";
  /** Default folder for the output cert path picker — the clicked/active pane's folder (CPE-1424), or
   *  "" from the command palette when no folder context is available. */
  export let outDir = "";

  const dispatch = createEventDispatcher<{ created: string; error: string; close: void }>();

  let caKeyPath = "";
  let validityDays = 365;
  let outCertPath = "";
  let outEdited = false;
  let busy = false;
  let error = "";

  onMount(async () => {
    await tick();
    document.getElementById("cert-sign-csr")?.focus();
  });

  // Smart default output path from the CSR's basename + the target folder — stops auto-tracking the
  // moment the user browses/edits it directly.
  $: if (!outEdited) outCertPath = outDir ? joinPath(outDir, defaultIssuedCertName(csrPath)) : "";

  $: signable = canSignCert({ csrPath, caCertPath, caKeyPath, outCertPath, validityDays, busy });

  const CERT_FILTER = [{ name: "Certificate / CSR", extensions: ["pem", "crt", "cer", "der", "csr"] }];
  const KEY_FILTER = [{ name: "Private key", extensions: ["pem", "key"] }];

  async function browseCsr() {
    try {
      const picked = await openFileDialog({ directory: false, multiple: false, filters: CERT_FILTER, title: "Choose the CSR" });
      if (typeof picked === "string") csrPath = picked;
    } catch {
      // Cancelled or unavailable — leave the current value untouched.
    }
  }
  async function browseCaCert() {
    try {
      const picked = await openFileDialog({ directory: false, multiple: false, filters: CERT_FILTER, title: "Choose the CA certificate" });
      if (typeof picked === "string") caCertPath = picked;
    } catch {
      // no-op
    }
  }
  async function browseCaKey() {
    try {
      const picked = await openFileDialog({ directory: false, multiple: false, filters: KEY_FILTER, title: "Choose the CA private key" });
      if (typeof picked === "string") caKeyPath = picked;
    } catch {
      // no-op
    }
  }
  async function browseOut() {
    try {
      const picked = await saveFileDialog({
        defaultPath: outCertPath || undefined,
        filters: [{ name: "Certificate", extensions: ["crt", "pem"] }],
        title: "Choose where to write the issued certificate",
      });
      if (picked) {
        outCertPath = picked;
        outEdited = true;
      }
    } catch {
      // no-op
    }
  }

  async function issue() {
    if (!signable || busy) return;
    busy = true;
    error = "";
    try {
      unwrap(await commands.certIssueFromCsr(csrPath, caCertPath, caKeyPath, validityDays, outCertPath));
      dispatch("created", outCertPath);
    } catch (e) {
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
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Sign / issue certificate" on:click|stopPropagation>
    <h2>
      <span class="hd-icon"><Icon name="certificate" size={18} /></span>
      Sign / issue certificate
    </h2>
    <p class="intro">
      Issues a leaf certificate for a CSR, signed by an existing CA's certificate and private key. The
      CA key is read only to sign — it's never written anywhere or sent back over IPC.
    </p>

    <label class="field-label" for="cert-sign-csr">CSR file</label>
    <div class="dest-row">
      <input
        id="cert-sign-csr"
        class="field"
        type="text"
        bind:value={csrPath}
        disabled={busy}
        spellcheck="false"
        title={csrPath}
        data-testid="cert-sign-csr"
      />
      <button class="btn" type="button" disabled={busy} on:click={browseCsr} data-testid="cert-sign-csr-browse">
        Browse…
      </button>
    </div>

    <label class="field-label" for="cert-sign-ca-cert">CA certificate</label>
    <div class="dest-row">
      <input
        id="cert-sign-ca-cert"
        class="field"
        type="text"
        bind:value={caCertPath}
        disabled={busy}
        spellcheck="false"
        title={caCertPath}
        data-testid="cert-sign-ca-cert"
      />
      <button class="btn" type="button" disabled={busy} on:click={browseCaCert} data-testid="cert-sign-ca-cert-browse">
        Browse…
      </button>
    </div>

    <label class="field-label" for="cert-sign-ca-key">CA private key</label>
    <div class="dest-row">
      <input
        id="cert-sign-ca-key"
        class="field"
        type="text"
        bind:value={caKeyPath}
        disabled={busy}
        spellcheck="false"
        title={caKeyPath}
        data-testid="cert-sign-ca-key"
      />
      <button class="btn" type="button" disabled={busy} on:click={browseCaKey} data-testid="cert-sign-ca-key-browse">
        Browse…
      </button>
    </div>

    <label class="field-label" for="cert-sign-validity">Validity (days)</label>
    <input
      id="cert-sign-validity"
      class="field"
      type="number"
      min="1"
      bind:value={validityDays}
      disabled={busy}
      data-testid="cert-sign-validity"
    />

    <label class="field-label" for="cert-sign-out">Output certificate</label>
    <div class="dest-row">
      <input
        id="cert-sign-out"
        class="field"
        type="text"
        value={outCertPath}
        on:input={(e) => { outEdited = true; outCertPath = e.currentTarget.value; }}
        disabled={busy}
        spellcheck="false"
        title={outCertPath}
        data-testid="cert-sign-out"
      />
      <button class="btn" type="button" disabled={busy} on:click={browseOut} data-testid="cert-sign-out-browse">
        Browse…
      </button>
    </div>

    {#if error}<div class="err" data-testid="cert-sign-error">{error}</div>{/if}

    <div class="actions">
      <button class="btn" data-testid="cert-sign-cancel" disabled={busy} on:click={() => dispatch("close")}>
        Cancel
      </button>
      <button class="btn primary" data-testid="cert-sign-confirm" disabled={!signable} on:click={issue}>
        {busy ? "Issuing…" : "Issue certificate"}
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
    width: 520px;
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
