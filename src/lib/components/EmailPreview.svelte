<script lang="ts">
  // .eml email preview (CPE-1434, epic CPE-1433 "Structured previews"): wires the CPE-1434
  // `email_preview` backend command into the preview pane. A read-only VIEWER — it shows the headers,
  // the MIME part / attachment list, and a SANITIZED plain-text body. It never renders HTML and never
  // loads remote resources (the backend strips an HTML-only body down to text), so opening an email
  // here can't fetch a tracking pixel or run a script. Self-contained like JwtPreview.svelte /
  // CertPreview.svelte: fetches its own data from `path`, no prop-drilled callback.
  import { unwrap } from "../invoke";
  import { commands } from "../bindings.gen";
  import type { EmailPreview as EmailPreviewData } from "../bindings.gen";
  import { formatSize } from "../format";
  import { displaySafeName } from "../filename";
  import Icon from "./Icon.svelte";

  /** The `.eml` file's path. */
  export let path: string;

  let data: EmailPreviewData | null = null;
  let loading = false;
  let loadError = "";

  // Reload whenever the previewed file changes (mirrors JwtPreview's `loadedPath` guard).
  let loadedPath = "";
  $: if (path && path !== loadedPath) { loadedPath = path; void load(); }

  async function load() {
    loading = true;
    loadError = "";
    data = null;
    try {
      data = unwrap(await commands.emailPreview(path));
    } catch (e) {
      loadError = String(e);
    } finally {
      loading = false;
    }
  }

  /** Prefer the humanized RFC 3339 date; fall back to the raw header value. */
  $: dateText = data ? (data.date_rfc3339 ?? data.date ?? "") : "";
</script>

<div class="crypto-preview" data-testid="email-preview">
  {#if loading}
    <p class="cp-note">Loading…</p>
  {:else if loadError}
    <p class="cp-error" data-testid="email-load-error">Can't preview this file: {loadError}</p>
  {:else if data}
    <div class="cp-banner">
      <Icon name="mail" size={14} />
      <span>Email viewer — remote content is not loaded; the body is shown as sanitized text.</span>
    </div>

    {#if data.error}
      <p class="cp-error" data-testid="email-decode-error">{data.error}</p>
    {/if}

    <div class="cp-section">
      <dl class="cp-rows">
        <div><dt>From</dt><dd class="wrap">{data.from ?? "—"}</dd></div>
        {#if data.to.length}
          <div><dt>To</dt><dd class="wrap" data-testid="email-to">{data.to.join(", ")}</dd></div>
        {/if}
        {#if data.cc.length}
          <div><dt>Cc</dt><dd class="wrap" data-testid="email-cc">{data.cc.join(", ")}</dd></div>
        {/if}
        <div><dt>Subject</dt><dd class="wrap">{data.subject ?? "—"}</dd></div>
        {#if dateText}
          <div><dt>Date</dt><dd class="wrap">{dateText}</dd></div>
        {/if}
      </dl>
    </div>

    {#if data.attachments.length}
      <div class="cp-section">
        <div class="cp-title">{data.attachments.length === 1 ? "1 attachment" : `${data.attachments.length} attachments`}</div>
        <div class="email-pills" data-testid="email-attachments">
          {#each data.attachments as att}
            <span class="email-pill" title={`${displaySafeName(att.filename)} — ${att.content_type}`}>
              <Icon name="paperclip" size={11} />
              <span class="email-pill-name">{displaySafeName(att.filename)}</span>
              <span class="email-pill-size">{formatSize(att.size)}</span>
            </span>
          {/each}
        </div>
      </div>
    {/if}

    <div class="cp-section">
      <div class="cp-title">
        Body{#if data.body_is_html}<span class="email-note" data-testid="email-html-note"> — HTML message, shown as text</span>{/if}
      </div>
      {#if data.body}
        <pre class="email-body" data-testid="email-body">{data.body}{#if data.body_truncated}{"\n\n…(truncated)"}{/if}</pre>
      {:else}
        <p class="cp-note" data-testid="email-empty-body">No text body.</p>
      {/if}
    </div>
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
  .cp-rows dt { color: var(--text-dim); width: 64px; flex: none; }
  .cp-rows dd { flex: 1; overflow-wrap: anywhere; }
  .wrap { overflow-wrap: anywhere; }
  .email-note { text-transform: none; font-weight: 400; color: var(--text-faint); letter-spacing: 0; }
  /* Tick-tacks rule (memory: pill rows reflow): the row wraps onto more lines and grows its height, while
     each pill keeps its text on one line and never shrinks. */
  .email-pills { display: flex; flex-wrap: wrap; gap: 6px; }
  .email-pill {
    display: inline-flex; align-items: center; gap: 6px; flex: 0 0 auto; max-width: 260px;
    padding: 3px 9px; border: 1px solid var(--border); border-radius: 999px;
    background: var(--surface-alt); color: var(--text); font-size: 11px; white-space: nowrap;
  }
  .email-pill-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .email-pill-size { color: var(--text-dim); flex: none; }
  .email-body {
    margin: 0; padding: 10px; border: 1px solid var(--border); border-radius: var(--radius);
    background: var(--surface-alt); font-family: var(--mono); font-size: 11.5px;
    white-space: pre-wrap; word-break: break-word; max-height: 420px; overflow: auto;
  }
</style>
