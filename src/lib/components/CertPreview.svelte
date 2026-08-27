<script lang="ts">
  // Certificate/CSR/key preview (CPE-1422, epic CPE-1417): wires the CPE-1419 `cert_decode` backend
  // command into the preview pane. A read-only VIEWER — decodes X.509 certs (PEM/DER), PKCS#10 CSRs,
  // standalone public keys, and (algorithm/size only, never key material) private-key files. Never
  // verifies a signature, a chain, or trust. Self-contained like DataBrowser.svelte / JwtPreview.svelte.
  import { unwrap } from "../invoke";
  import { commands } from "../bindings.gen";
  import type { CertPreview as CertPreviewData, KeyInfo } from "../bindings.gen";
  import { formatDate } from "../datetime";
  import Icon from "./Icon.svelte";

  /** The certificate/CSR/key file's path. */
  export let path: string;

  let data: CertPreviewData | null = null;
  let loading = false;
  let loadError = "";
  let copiedKey = "";

  let loadedPath = "";
  $: if (path && path !== loadedPath) { loadedPath = path; void load(); }

  async function load() {
    loading = true;
    loadError = "";
    data = null;
    try {
      data = unwrap(await commands.certDecode(path));
    } catch (e) {
      loadError = String(e);
    } finally {
      loading = false;
    }
  }

  // The backend guarantees exactly one of these is set when there's no `error` — narrow once here so
  // the markup below can stay flat instead of nesting `data.kind === … && data.x` everywhere.
  $: cert = data && !data.error && data.kind === "certificate" ? data.certificate : null;
  $: csr = data && !data.error && data.kind === "csr" ? data.csr : null;
  $: pubKey = data && !data.error && data.kind === "public_key" ? data.public_key : null;
  $: privKey = data && !data.error && data.kind === "private_key" ? data.private_key : null;

  function keyLabel(k: KeyInfo): string {
    let s = k.algorithm;
    if (k.curve) s += ` (${k.curve})`;
    else if (k.size_bits != null) s += ` — ${k.size_bits}-bit`;
    return s;
  }

  /** `not_before`/`not_after` arrive as RFC 3339 strings; render like Explorer's own date column. */
  function humanIso(iso: string): string {
    const ms = Date.parse(iso);
    if (Number.isNaN(ms)) return iso;
    return formatDate(ms) || iso;
  }

  async function copy(text: string, key: string) {
    try {
      await navigator.clipboard.writeText(text);
      copiedKey = key;
      setTimeout(() => { if (copiedKey === key) copiedKey = ""; }, 1500);
    } catch {
      /* clipboard unavailable — leave the text on screen to copy manually */
    }
  }
</script>

<div class="crypto-preview" data-testid="cert-preview">
  {#if loading}
    <p class="cp-note">Loading…</p>
  {:else if loadError}
    <p class="cp-error" data-testid="cert-load-error">Can't preview this file: {loadError}</p>
  {:else if data}
    <div class="cp-banner">
      <Icon name="lock" size={14} />
      <span>Certificate viewer — decodes the structure for display. It does not verify trust or a chain.</span>
    </div>

    {#if data.error}
      <p class="cp-error" data-testid="cert-decode-error">{data.error}</p>
    {:else if cert}
      <div class="cp-section">
        <dl class="cp-rows">
          <div><dt>Subject</dt><dd class="wrap">{cert.subject}</dd></div>
          <div><dt>Issuer</dt><dd class="wrap">{cert.issuer}</dd></div>
          <div><dt>Serial</dt><dd class="mono wrap">{cert.serial}</dd></div>
          <div><dt>Version</dt><dd>{cert.version}</dd></div>
          <div>
            <dt>Valid from</dt>
            <dd>
              {humanIso(cert.not_before)}
              {#if cert.not_yet_valid}<span class="cp-badge danger" data-testid="cert-not-yet-valid">NOT YET VALID</span>{/if}
            </dd>
          </div>
          <div>
            <dt>Valid until</dt>
            <dd>
              {humanIso(cert.not_after)}
              {#if cert.expired}<span class="cp-badge danger" data-testid="cert-expired">EXPIRED</span>{/if}
            </dd>
          </div>
          <div><dt>Signature algo</dt><dd>{cert.signature_algorithm}</dd></div>
          <div><dt>Public key</dt><dd>{keyLabel(cert.public_key)}</dd></div>
          <div><dt>CA certificate</dt><dd>{cert.is_ca ? "Yes" : "No"}</dd></div>
        </dl>
      </div>

      {#if cert.subject_alt_names.length}
        <div class="cp-section">
          <div class="cp-title">Subject alternative names</div>
          <!-- Reflow rule (tick-tacks, CLAUDE.md): the pill row wraps onto more rows and grows; each
               pill keeps its own text on one line and never wraps internally. -->
          <div class="cp-pills">
            {#each cert.subject_alt_names as san}
              <span class="cp-pill" title={san}><span class="cp-pill-text">{san}</span></span>
            {/each}
          </div>
        </div>
      {/if}

      {#if cert.key_usage.length}
        <div class="cp-section">
          <div class="cp-title">Key usage</div>
          <div class="cp-pills">
            {#each cert.key_usage as ku}<span class="cp-pill">{ku}</span>{/each}
          </div>
        </div>
      {/if}

      {#if cert.extended_key_usage.length}
        <div class="cp-section">
          <div class="cp-title">Extended key usage</div>
          <div class="cp-pills">
            {#each cert.extended_key_usage as eku}<span class="cp-pill">{eku}</span>{/each}
          </div>
        </div>
      {/if}

      <div class="cp-section">
        <div class="cp-title">Fingerprints</div>
        <div class="cp-fp-row">
          <span class="cp-fp-label">SHA-256</span>
          <code class="cp-hash">{cert.sha256_fingerprint}</code>
          <button class="cp-copy" on:click={() => copy(cert.sha256_fingerprint, "sha256")}>
            <Icon name={copiedKey === "sha256" ? "check" : "copy"} size={12} />
            {copiedKey === "sha256" ? "Copied" : "Copy"}
          </button>
        </div>
        <div class="cp-fp-row">
          <span class="cp-fp-label">SHA-1</span>
          <code class="cp-hash">{cert.sha1_fingerprint}</code>
          <button class="cp-copy" on:click={() => copy(cert.sha1_fingerprint, "sha1")}>
            <Icon name={copiedKey === "sha1" ? "check" : "copy"} size={12} />
            {copiedKey === "sha1" ? "Copied" : "Copy"}
          </button>
        </div>
      </div>
    {:else if csr}
      <div class="cp-section">
        <dl class="cp-rows">
          <div><dt>Requested subject</dt><dd class="wrap">{csr.subject}</dd></div>
          <div><dt>Public key</dt><dd>{keyLabel(csr.public_key)}</dd></div>
        </dl>
      </div>
      {#if csr.requested_sans.length}
        <div class="cp-section">
          <div class="cp-title">Requested SANs</div>
          <div class="cp-pills">
            {#each csr.requested_sans as san}
              <span class="cp-pill" title={san}><span class="cp-pill-text">{san}</span></span>
            {/each}
          </div>
        </div>
      {/if}
    {:else if pubKey}
      <div class="cp-section">
        <dl class="cp-rows">
          <div><dt>Algorithm</dt><dd>{keyLabel(pubKey)}</dd></div>
        </dl>
      </div>
    {:else if privKey}
      <div class="cp-banner warn">
        <Icon name="lock" size={14} />
        <span>Private key file — only the algorithm and size are shown below. Key material is never read.</span>
      </div>
      <div class="cp-section">
        <dl class="cp-rows">
          <div><dt>Algorithm</dt><dd>{keyLabel(privKey)}</dd></div>
        </dl>
      </div>
    {:else}
      <p class="cp-error">Unrecognized decode result.</p>
    {/if}

    {#if data.encoding}
      <p class="cp-encoding">Encoding: {data.encoding.toUpperCase()}</p>
    {/if}
  {/if}
</div>

<style>
  .crypto-preview { padding: 12px; font-size: 12px; }
  .cp-note { color: var(--text-faint); }
  .cp-error { color: var(--danger); white-space: pre-wrap; overflow-wrap: anywhere; }
  .cp-banner {
    display: flex; align-items: center; gap: 8px; padding: 7px 10px; border-radius: var(--radius);
    background: var(--surface-alt); border: 1px solid var(--border); color: var(--text-dim);
    margin-bottom: 12px; font-size: 11.5px;
  }
  .cp-banner.warn {
    color: var(--danger); border-color: var(--danger);
    background: color-mix(in srgb, var(--danger) 8%, var(--surface));
  }
  .cp-section { margin-bottom: 14px; }
  .cp-section:last-child { margin-bottom: 0; }
  .cp-title { font-size: 11px; font-weight: 600; color: var(--text-dim); text-transform: uppercase; letter-spacing: 0.03em; margin-bottom: 6px; }
  .cp-rows { display: grid; gap: 6px; }
  .cp-rows > div { display: flex; gap: 10px; align-items: baseline; }
  .cp-rows dt { color: var(--text-dim); width: 110px; flex: none; }
  .cp-rows dd { flex: 1; overflow-wrap: anywhere; }
  .mono { font-family: var(--mono); font-size: 11.5px; }
  .wrap { overflow-wrap: anywhere; }
  .cp-badge {
    display: inline-flex; align-items: center; margin-left: 8px; padding: 1px 8px;
    border-radius: 999px; font-size: 10px; font-weight: 700; white-space: nowrap; letter-spacing: 0.02em;
  }
  .cp-badge.danger { color: var(--danger); border: 1px solid var(--danger); background: color-mix(in srgb, var(--danger) 12%, var(--surface)); }
  /* Reflow row: pills wrap onto more rows and grow the container; each pill keeps its text on one line. */
  .cp-pills { display: flex; flex-wrap: wrap; gap: 6px; }
  .cp-pill {
    display: inline-flex; align-items: center; flex: 0 0 auto; max-width: 100%;
    padding: 2px 8px; background: var(--surface-alt); border: 1px solid var(--border);
    border-radius: 999px; font-size: 11.5px; white-space: nowrap;
  }
  .cp-pill-text { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 260px; }
  .cp-fp-row { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; margin-bottom: 6px; }
  .cp-fp-row:last-child { margin-bottom: 0; }
  .cp-fp-label { color: var(--text-dim); width: 60px; flex: none; }
  .cp-hash { font-family: var(--mono); font-size: 11px; overflow-wrap: anywhere; flex: 1; min-width: 160px; }
  .cp-copy {
    display: inline-flex; align-items: center; gap: 4px; height: 22px; padding: 0 8px;
    border: 1px solid var(--border-strong); border-radius: var(--radius); background: var(--surface-alt);
    color: var(--text); font-size: 11px; cursor: pointer; flex: none;
  }
  .cp-copy:hover { background: var(--surface); }
  .cp-encoding { margin: 12px 0 0; color: var(--text-faint); font-size: 11px; }
</style>
