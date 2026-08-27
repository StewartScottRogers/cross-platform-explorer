<script lang="ts">
  // JWT preview (CPE-1422, epic CPE-1417): wires the CPE-1418 `jwt_preview` backend command into the
  // preview pane. A read-only VIEWER — it decodes header/payload/claims for display and never checks
  // (or claims to check) the signature. Self-contained like DataBrowser.svelte: fetches its own data
  // from `path`, no prop-drilled callback.
  import { unwrap } from "../invoke";
  import { commands } from "../bindings.gen";
  import type { JwtPreview as JwtPreviewData } from "../bindings.gen";
  import { formatDate } from "../datetime";
  import Icon from "./Icon.svelte";

  /** The `.jwt`/`.jws` file's path. */
  export let path: string;
  /** Reports this preview's copyable values up to `PreviewPane` (CPE-1570, epic CPE-1568), keyed to match
   *  the `jwt` provider's declared action ids in `preview/provider.ts` — replaces the copy buttons this
   *  component used to render inline; the pane's generic action bar renders them instead. Called
   *  reactively whenever the decoded claims/header change (including back to "" on a fresh load/error),
   *  so a stale value from a previous file never lingers in the action bar. */
  export let onValues: (values: Record<string, string>) => void = () => {};

  let data: JwtPreviewData | null = null;
  let loading = false;
  let loadError = "";

  // Reload whenever the previewed file changes (mirrors DataBrowser's `loadedPath` guard).
  let loadedPath = "";
  $: if (path && path !== loadedPath) { loadedPath = path; void load(); }

  async function load() {
    loading = true;
    loadError = "";
    data = null;
    try {
      data = unwrap(await commands.jwtPreview(path));
    } catch (e) {
      loadError = String(e);
    } finally {
      loading = false;
    }
  }

  // Local aliases so the `{#if}`-guarded copy buttons below don't need `data.x ?? ""` — narrowing a
  // reactive `data.payload_json` inside an inline arrow-function closure doesn't survive TS's flow
  // analysis, so pull them out here instead.
  $: payloadJson = data?.payload_json ?? "";
  $: headerJson = data?.header_json ?? "";
  // Report the copyable values up to PreviewPane whenever they change (CPE-1570) — mirrors the ids the
  // `jwt` provider's `actions` declare in `preview/provider.ts`.
  $: onValues({ "copy-claims": payloadJson, "copy-header": headerJson });

  /** `exp`/`iat`/`nbf` claims are Unix-epoch seconds; render like Explorer's own date column. */
  function human(rawSeconds: number): string {
    return formatDate(rawSeconds * 1000) || String(rawSeconds);
  }
</script>

<div class="crypto-preview" data-testid="jwt-preview">
  {#if loading}
    <p class="cp-note">Loading…</p>
  {:else if loadError}
    <p class="cp-error" data-testid="jwt-load-error">Can't preview this file: {loadError}</p>
  {:else if data}
    <div class="cp-banner">
      <Icon name="lock" size={14} />
      <span>JWT viewer — decodes the token for display. It does not verify the signature.</span>
    </div>

    {#if data.error}
      <p class="cp-error" data-testid="jwt-decode-error">{data.error}</p>
    {/if}

    <div class="cp-section">
      <div class="cp-title">Header</div>
      <dl class="cp-rows">
        <div><dt>Algorithm</dt><dd>{data.alg ?? "—"}</dd></div>
        <div><dt>Type</dt><dd>{data.typ ?? "—"}</dd></div>
        {#if data.kid}<div><dt>Key ID</dt><dd class="mono wrap">{data.kid}</dd></div>{/if}
      </dl>
    </div>

    {#if data.exp || data.iat || data.nbf}
      <div class="cp-section">
        <div class="cp-title">Validity</div>
        <dl class="cp-rows">
          {#if data.iat}
            <div><dt>Issued at</dt><dd>{human(data.iat.raw)}</dd></div>
          {/if}
          {#if data.nbf}
            <div>
              <dt>Not before</dt>
              <dd>
                {human(data.nbf.raw)}
                {#if data.not_yet_valid}<span class="cp-badge danger" data-testid="jwt-not-yet-valid">NOT YET VALID</span>{/if}
              </dd>
            </div>
          {/if}
          {#if data.exp}
            <div>
              <dt>Expires</dt>
              <dd>
                {human(data.exp.raw)}
                {#if data.expired}<span class="cp-badge danger" data-testid="jwt-expired">EXPIRED</span>{/if}
              </dd>
            </div>
          {/if}
        </dl>
      </div>
    {/if}

    <div class="cp-section">
      <div class="cp-title">Signature</div>
      <p class="cp-sig" data-testid="jwt-signature">
        {#if data.signature_present}
          Signature present ({data.signature_len.toLocaleString()} {data.signature_len === 1 ? "byte" : "bytes"})
        {:else}
          No signature — unsigned token ({data.alg === "none" ? "alg: none" : "empty or malformed"})
        {/if}
      </p>
    </div>

    {#if payloadJson}
      <div class="cp-section">
        <!-- Copy button migrated to the pane's generic action bar (CPE-1570) — see `onValues` above. -->
        <div class="cp-title">Claims</div>
        <pre class="cp-json">{payloadJson}</pre>
      </div>
    {/if}

    {#if headerJson}
      <div class="cp-section">
        <div class="cp-title">Raw header</div>
        <pre class="cp-json">{headerJson}</pre>
      </div>
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
  .cp-section { margin-bottom: 14px; }
  .cp-section:last-child { margin-bottom: 0; }
  .cp-title { font-size: 11px; font-weight: 600; color: var(--text-dim); text-transform: uppercase; letter-spacing: 0.03em; margin-bottom: 6px; }
  .cp-rows { display: grid; gap: 6px; }
  .cp-rows > div { display: flex; gap: 10px; align-items: baseline; }
  .cp-rows dt { color: var(--text-dim); width: 90px; flex: none; }
  .cp-rows dd { flex: 1; overflow-wrap: anywhere; }
  .mono { font-family: var(--mono); font-size: 11.5px; }
  .wrap { overflow-wrap: anywhere; }
  .cp-badge {
    display: inline-flex; align-items: center; margin-left: 8px; padding: 1px 8px;
    border-radius: 999px; font-size: 10px; font-weight: 700; white-space: nowrap; letter-spacing: 0.02em;
  }
  .cp-badge.danger { color: var(--danger); border: 1px solid var(--danger); background: color-mix(in srgb, var(--danger) 12%, var(--surface)); }
  .cp-sig { margin: 0; color: var(--text); }
  .cp-json {
    margin: 0; padding: 10px; border: 1px solid var(--border); border-radius: var(--radius);
    background: var(--surface-alt); font-family: var(--mono); font-size: 11.5px;
    white-space: pre-wrap; word-break: break-word; max-height: 260px; overflow: auto;
  }
</style>
