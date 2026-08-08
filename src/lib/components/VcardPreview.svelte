<script lang="ts">
  // .vcf vCard preview (CPE-1436, epic CPE-1433 "Structured previews"): wires the CPE-1436
  // `vcard_preview` backend command into the preview pane. A read-only VIEWER — it shows each contact as a
  // card (name/org/title heading, then phone/email/address/URL rows and reflowing TYPE pills). A card's
  // PHOTO is reported presence-only: the backend never returns the image bytes over IPC, so the card shows
  // a "photo present" note rather than the picture. Self-contained like EmailPreview.svelte: fetches its
  // own data from `path`, no prop-drilled callback.
  import { unwrap } from "../invoke";
  import { commands } from "../bindings.gen";
  import type { VcardPreview as VcardPreviewData } from "../bindings.gen";
  import { formatSize } from "../format";
  import Icon from "./Icon.svelte";

  /** The `.vcf` file's path. */
  export let path: string;

  let data: VcardPreviewData | null = null;
  let loading = false;
  let loadError = "";

  // Reload whenever the previewed file changes (mirrors EmailPreview's `loadedPath` guard).
  let loadedPath = "";
  $: if (path && path !== loadedPath) { loadedPath = path; void load(); }

  async function load() {
    loading = true;
    loadError = "";
    data = null;
    try {
      data = unwrap(await commands.vcardPreview(path));
    } catch (e) {
      loadError = String(e);
    } finally {
      loading = false;
    }
  }

  /** The card's display heading: FN, else the structured N, else a placeholder. */
  function heading(card: VcardPreviewData["cards"][number]): string {
    return card.formatted_name || card.name || "(unnamed contact)";
  }

  /** The org/title sub-heading line, joining whatever is present. */
  function subheading(card: VcardPreviewData["cards"][number]): string {
    return [card.title, card.org].filter(Boolean).join(" · ");
  }
</script>

<div class="crypto-preview" data-testid="vcard-preview">
  {#if loading}
    <p class="cp-note">Loading…</p>
  {:else if loadError}
    <p class="cp-error" data-testid="vcard-load-error">Can't preview this file: {loadError}</p>
  {:else if data}
    <div class="cp-banner">
      <Icon name="contact" size={14} />
      <span>Contact viewer — read-only.{#if data.cards.length > 1}<span class="cp-count" data-testid="vcard-count"> {data.cards.length} contacts</span>{/if}</span>
    </div>

    {#if data.error}
      <p class="cp-error" data-testid="vcard-decode-error">{data.error}</p>
    {/if}

    {#each data.cards as card}
      <div class="cp-card" data-testid="vcard-card">
        <div class="cp-card-head">
          <span class="cp-name" data-testid="vcard-name">{heading(card)}</span>
          {#if subheading(card)}
            <span class="cp-sub" data-testid="vcard-sub">{subheading(card)}</span>
          {/if}
        </div>

        {#if card.has_photo}
          <p class="cp-photo" data-testid="vcard-photo"><Icon name="image" size={12} /> Photo present ({formatSize(card.photo_size)}) — not shown here</p>
        {/if}

        <dl class="cp-rows">
          {#each card.phones as tel}
            <div>
              <dt><Icon name="phone" size={12} /> Phone</dt>
              <dd class="wrap" data-testid="vcard-tel">
                <span class="cp-val">{tel.number}</span>
                {#if tel.types.length}<span class="pill-row inline">{#each tel.types as t}<span class="pill type-pill">{t}</span>{/each}</span>{/if}
              </dd>
            </div>
          {/each}
          {#each card.emails as em}
            <div>
              <dt><Icon name="mail" size={12} /> Email</dt>
              <dd class="wrap" data-testid="vcard-email">
                <span class="cp-val">{em.address}</span>
                {#if em.types.length}<span class="pill-row inline">{#each em.types as t}<span class="pill type-pill">{t}</span>{/each}</span>{/if}
              </dd>
            </div>
          {/each}
          {#each card.addresses as adr}
            <div>
              <dt><Icon name="location" size={12} /> Address</dt>
              <dd class="wrap" data-testid="vcard-adr">
                <span class="cp-val">{adr.label}</span>
                {#if adr.types.length}<span class="pill-row inline">{#each adr.types as t}<span class="pill type-pill">{t}</span>{/each}</span>{/if}
              </dd>
            </div>
          {/each}
          {#each card.urls as url}
            <div><dt><Icon name="globe" size={12} /> URL</dt><dd class="wrap" data-testid="vcard-url">{url}</dd></div>
          {/each}
          {#if card.birthday}
            <div><dt><Icon name="calendar" size={12} /> Birthday</dt><dd class="wrap" data-testid="vcard-bday">{card.birthday}</dd></div>
          {/if}
        </dl>
      </div>
    {/each}
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
  .cp-count { color: var(--text); font-weight: 600; }
  .cp-card {
    border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface-alt);
    padding: 10px 12px; margin-bottom: 12px;
  }
  .cp-card:last-child { margin-bottom: 0; }
  .cp-card-head { margin-bottom: 8px; }
  .cp-name { font-size: 14px; font-weight: 600; color: var(--text); overflow-wrap: anywhere; display: block; }
  .cp-sub { font-size: 11.5px; color: var(--text-dim); overflow-wrap: anywhere; }
  .cp-photo { display: flex; align-items: center; gap: 6px; margin: 0 0 8px; color: var(--text-dim); font-size: 11.5px; }
  .cp-rows { display: grid; gap: 6px; margin: 0; }
  .cp-rows > div { display: flex; gap: 10px; align-items: baseline; }
  .cp-rows dt { color: var(--text-dim); width: 84px; flex: none; display: inline-flex; align-items: center; gap: 5px; }
  .cp-rows dd { flex: 1; margin: 0; overflow-wrap: anywhere; display: flex; flex-wrap: wrap; align-items: center; gap: 6px; }
  .wrap { overflow-wrap: anywhere; }
  .cp-val { overflow-wrap: anywhere; }
  /* Tick-tacks rule (memory: pill rows reflow): the row wraps onto more lines and grows its height, while
     each pill keeps its text on one line and never shrinks. */
  .pill-row { display: flex; flex-wrap: wrap; gap: 6px; }
  .pill-row.inline { gap: 4px; }
  .pill {
    display: inline-flex; align-items: center; gap: 6px; flex: 0 0 auto; max-width: 200px;
    padding: 2px 8px; border: 1px solid var(--border); border-radius: 999px;
    background: var(--surface); color: var(--text-dim); font-size: 10px; white-space: nowrap;
    text-transform: uppercase; letter-spacing: 0.03em;
  }
  .type-pill { font-weight: 600; }
</style>
