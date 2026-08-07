<script lang="ts">
  /**
   * Create certificate… dialog (CPE-1423, epic CPE-1417): the CREATE side of certificate management —
   * generates a fresh keypair + self-signed X.509 certificate via the shipped `cert_create` backend
   * (CPE-1420) and writes both PEM files to disk. Opened from the pane-aware context menu's "Create
   * certificate here…" (CPE-1424, on a folder row or empty space) or the command palette, always
   * pre-filled with the clicked/active pane's folder as the default output location — a native Browse
   * picker (memory: every path field needs one) lets the user pick anywhere else.
   *
   * Owns its own backend call (same pattern as VaultCreateDialog/RepairLinkDialog): validates locally
   * (CN + output folder/filenames all non-empty — `canCreateCert`, `certCreate.ts`), calls
   * `commands.certCreate`, and dispatches `created` (the new cert file's full path) / `error` / `close`.
   * SAN DNS names + IPs are reflowing pill inputs (tick-tacks, CLAUDE.md) — Enter/comma commits a token,
   * Backspace on an empty draft peels the last pill, the same interaction TagEditor's chip input uses.
   */
  import { createEventDispatcher, onMount, tick } from "svelte";
  import { open as openFolderDialog } from "@tauri-apps/plugin-dialog";
  import Icon from "./Icon.svelte";
  import { commands } from "../bindings.gen";
  import type { KeyType } from "../bindings.gen";
  import { unwrap } from "../invoke";
  import { sanitizeFileBase, joinPath, canCreateCert } from "../certCreate";

  /** Default output folder — the clicked/active pane's folder (CPE-1424), or "" from the command
   *  palette when no folder context is available; a native Browse picker fills it in either way. */
  export let outDir = "";

  const dispatch = createEventDispatcher<{ created: string; error: string; close: void }>();

  const KEY_TYPES: { value: KeyType; label: string }[] = [
    { value: "ec_p256", label: "EC-P256 (default)" },
    { value: "ec_p384", label: "EC-P384" },
    { value: "rsa_2048", label: "RSA-2048" },
    { value: "rsa_4096", label: "RSA-4096" },
  ];

  let commonName = "";
  let sanDns: string[] = [];
  let dnsDraft = "";
  let sanIps: string[] = [];
  let ipDraft = "";
  let validityDays = 365;
  let keyType: KeyType = "ec_p256";
  let isCa = false;
  let folder = outDir;
  let certFileName = "";
  let keyFileName = "";
  let certNameEdited = false;
  let keyNameEdited = false;
  let busy = false;
  let error = "";

  let cnField: HTMLInputElement;
  onMount(async () => {
    await tick();
    cnField?.focus();
  });

  // Smart default filenames from the CN — stop auto-tracking the moment the user edits either by hand.
  $: if (!certNameEdited) certFileName = `${sanitizeFileBase(commonName)}.pem`;
  $: if (!keyNameEdited) keyFileName = `${sanitizeFileBase(commonName)}.key`;

  $: creatable = canCreateCert({ commonName, folder, certFileName, keyFileName, busy });

  function addDns() {
    const v = dnsDraft.trim();
    if (v && !sanDns.includes(v)) sanDns = [...sanDns, v];
    dnsDraft = "";
  }
  function removeDns(v: string) {
    sanDns = sanDns.filter((x) => x !== v);
  }
  function onDnsKey(e: KeyboardEvent) {
    if (e.key === "Enter" || e.key === ",") {
      e.preventDefault();
      addDns();
    } else if (e.key === "Backspace" && dnsDraft === "" && sanDns.length > 0) {
      e.preventDefault();
      sanDns = sanDns.slice(0, -1);
    }
  }

  function addIp() {
    const v = ipDraft.trim();
    if (v && !sanIps.includes(v)) sanIps = [...sanIps, v];
    ipDraft = "";
  }
  function removeIp(v: string) {
    sanIps = sanIps.filter((x) => x !== v);
  }
  function onIpKey(e: KeyboardEvent) {
    if (e.key === "Enter" || e.key === ",") {
      e.preventDefault();
      addIp();
    } else if (e.key === "Backspace" && ipDraft === "" && sanIps.length > 0) {
      e.preventDefault();
      sanIps = sanIps.slice(0, -1);
    }
  }

  async function browseFolder() {
    try {
      const picked = await openFolderDialog({
        directory: true,
        multiple: false,
        defaultPath: folder || undefined,
        title: "Choose an output folder",
      });
      if (typeof picked === "string") folder = picked;
    } catch {
      // Cancelled or unavailable — leave the current folder untouched.
    }
  }

  async function create() {
    if (!creatable || busy) return;
    addDns(); // fold any half-typed pill in before submitting
    addIp();
    busy = true;
    error = "";
    const certPath = joinPath(folder, certFileName);
    const keyPath = joinPath(folder, keyFileName);
    try {
      unwrap(
        await commands.certCreate(
          {
            common_name: commonName.trim(),
            san_dns: sanDns,
            san_ips: sanIps,
            validity_days: validityDays,
            key_type: keyType,
            is_ca: isCa,
          },
          certPath,
          keyPath,
        ),
      );
      dispatch("created", certPath);
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
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Create certificate" on:click|stopPropagation>
    <h2>
      <span class="hd-icon"><Icon name="certificate" size={18} /></span>
      Create certificate
    </h2>
    <p class="intro">
      Generates a fresh keypair and a self-signed X.509 certificate, then writes both PEM files to the
      folder below. This is a self-signed cert — to issue one signed by an existing CA instead, use
      "Sign / issue from CSR…".
    </p>

    <label class="field-label" for="cert-create-cn">Common name (CN)</label>
    <input
      id="cert-create-cn"
      class="field"
      type="text"
      bind:this={cnField}
      value={commonName}
      on:input={(e) => (commonName = e.currentTarget.value)}
      disabled={busy}
      placeholder="my-service.local"
      spellcheck="false"
      data-testid="cert-create-cn"
    />

    <label class="field-label" for="cert-create-dns-input">Subject alternative names — DNS</label>
    <div class="pills" data-testid="cert-create-dns-chips">
      {#each sanDns as v (v)}
        <span class="pill">
          <span class="pill-text">{v}</span>
          <button
            class="pill-x"
            type="button"
            title="Remove"
            aria-label={`Remove ${v}`}
            data-testid={`cert-create-dns-remove-${v}`}
            on:click={() => removeDns(v)}
          ><Icon name="close" size={11} /></button>
        </span>
      {/each}
      <input
        id="cert-create-dns-input"
        class="pill-input"
        bind:value={dnsDraft}
        disabled={busy}
        placeholder="alt.example.com"
        spellcheck="false"
        on:keydown={onDnsKey}
        data-testid="cert-create-dns-input"
      />
    </div>

    <label class="field-label" for="cert-create-ip-input">Subject alternative names — IP addresses</label>
    <div class="pills" data-testid="cert-create-ip-chips">
      {#each sanIps as v (v)}
        <span class="pill">
          <span class="pill-text">{v}</span>
          <button
            class="pill-x"
            type="button"
            title="Remove"
            aria-label={`Remove ${v}`}
            data-testid={`cert-create-ip-remove-${v}`}
            on:click={() => removeIp(v)}
          ><Icon name="close" size={11} /></button>
        </span>
      {/each}
      <input
        id="cert-create-ip-input"
        class="pill-input"
        bind:value={ipDraft}
        disabled={busy}
        placeholder="127.0.0.1"
        spellcheck="false"
        on:keydown={onIpKey}
        data-testid="cert-create-ip-input"
      />
    </div>

    <div class="row-2">
      <div>
        <label class="field-label" for="cert-create-validity">Validity (days)</label>
        <input
          id="cert-create-validity"
          class="field"
          type="number"
          min="1"
          bind:value={validityDays}
          disabled={busy}
          data-testid="cert-create-validity"
        />
      </div>
      <div>
        <label class="field-label" for="cert-create-keytype">Key type</label>
        <select
          id="cert-create-keytype"
          class="field"
          bind:value={keyType}
          disabled={busy}
          data-testid="cert-create-keytype"
        >
          {#each KEY_TYPES as kt}
            <option value={kt.value}>{kt.label}</option>
          {/each}
        </select>
      </div>
    </div>

    <label class="check-row">
      <input type="checkbox" bind:checked={isCa} disabled={busy} data-testid="cert-create-isca" />
      <span>This is a CA certificate (can sign other certificates)</span>
    </label>

    <label class="field-label" for="cert-create-folder">Output folder</label>
    <div class="dest-row">
      <input
        id="cert-create-folder"
        class="field"
        type="text"
        bind:value={folder}
        disabled={busy}
        spellcheck="false"
        title={folder}
        data-testid="cert-create-folder"
      />
      <button class="btn" type="button" disabled={busy} on:click={browseFolder} data-testid="cert-create-folder-browse">
        Browse…
      </button>
    </div>

    <div class="row-2">
      <div>
        <label class="field-label" for="cert-create-certname">Certificate file</label>
        <input
          id="cert-create-certname"
          class="field"
          type="text"
          value={certFileName}
          on:input={(e) => { certNameEdited = true; certFileName = e.currentTarget.value; }}
          disabled={busy}
          spellcheck="false"
          data-testid="cert-create-certname"
        />
      </div>
      <div>
        <label class="field-label" for="cert-create-keyname">Key file</label>
        <input
          id="cert-create-keyname"
          class="field"
          type="text"
          value={keyFileName}
          on:input={(e) => { keyNameEdited = true; keyFileName = e.currentTarget.value; }}
          disabled={busy}
          spellcheck="false"
          data-testid="cert-create-keyname"
        />
      </div>
    </div>

    {#if error}<div class="err" data-testid="cert-create-error">{error}</div>{/if}

    <div class="actions">
      <button class="btn" data-testid="cert-create-cancel" disabled={busy} on:click={() => dispatch("close")}>
        Cancel
      </button>
      <button class="btn primary" data-testid="cert-create-confirm" disabled={!creatable} on:click={create}>
        {busy ? "Creating…" : "Create"}
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
  select.field { cursor: pointer; }
  .row-2 { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  /* Reflow row (tick-tacks, CLAUDE.md): pills wrap onto more rows and grow the container; each pill
     keeps its own text on one line and never wraps internally. */
  .pills {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    padding: 6px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    margin-bottom: 12px;
  }
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    flex: 0 0 auto;
    max-width: 100%;
    padding: 2px 4px 2px 8px;
    background: var(--surface-alt);
    border: 1px solid var(--border);
    border-radius: 999px;
    font-size: 12px;
    white-space: nowrap;
  }
  .pill-text { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 220px; }
  .pill-x {
    display: grid;
    place-items: center;
    width: 16px;
    height: 16px;
    border-radius: 999px;
    color: var(--text-dim);
    flex: 0 0 auto;
  }
  .pill-x:hover { background: var(--hover); color: var(--text); }
  .pill-input {
    flex: 1 1 120px;
    min-width: 120px;
    border: 0;
    background: transparent;
    color: var(--text);
    font: inherit;
    height: 22px;
    outline: none;
  }
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
