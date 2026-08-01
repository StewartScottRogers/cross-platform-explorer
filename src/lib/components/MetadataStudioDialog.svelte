<script lang="ts">
  // Metadata Studio (CPE-1041, epic CPE-725): an editable, tabbed metadata inspector spanning the read
  // codecs (ID3/Vorbis audio, EXIF image, PDF doc, MP4 video). Editable formats (mp3/flac today) can be
  // changed and saved back via `metadata_write`; the rest are shown read-only until their write codecs
  // land. Follows the dialog convention (visible border) and the tab standard (accent active tab).
  import { createEventDispatcher, onMount } from "svelte";
  import { unwrap } from "../invoke";
  import { commands, type MetaField } from "../bindings.gen";
  import { joinFieldKey, buildMetaEdits } from "../metaEdits";
  import Icon from "./Icon.svelte";
  import { t } from "../i18n";
  import type { DirEntry } from "../types";

  /** The current selection. The first file is the one edited; extra files enable batch-apply. */
  export let entries: DirEntry[] = [];

  const dispatch = createEventDispatcher<{ close: void }>();

  $: files = entries.filter((e) => !e.is_dir);
  $: primary = files[0] ?? null;

  let fields: MetaField[] = [];
  let writable = false;
  let loading = true;
  let error = "";
  let saving = false;
  let notice = "";
  let applyToAll = false;

  // Friendly tab label per metadata group the codecs emit.
  const GROUP_LABEL: Record<string, string> = {
    id3: "studio.tabAudio",
    vorbis: "studio.tabAudio",
    exif: "studio.tabImage",
    pdf: "studio.tabDocument",
    video: "studio.tabVideo",
  };
  const groupLabel = (g: string) => (GROUP_LABEL[g] ? $t(GROUP_LABEL[g]) : g);

  // Pending edits keyed by "group\0key"; absent = untouched.
  let edited: Record<string, string> = {};
  const ekey = (f: MetaField) => joinFieldKey(f.group, f.key);

  $: groups = Array.from(new Set(fields.map((f) => f.group)));
  let activeGroup = "";
  $: if (groups.length && !groups.includes(activeGroup)) activeGroup = groups[0];
  $: shown = fields.filter((f) => f.group === activeGroup);
  $: dirty = Object.keys(edited).length > 0;

  async function load() {
    if (!primary) {
      loading = false;
      return;
    }
    loading = true;
    error = "";
    edited = {};
    notice = "";
    try {
      fields = unwrap(await commands.metadataRead(primary.path));
      writable = unwrap(await commands.metadataWritable(primary.path));
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }
  onMount(load);

  function currentValue(f: MetaField): string {
    const k = ekey(f);
    return k in edited ? edited[k] : f.value;
  }
  function editable(f: MetaField): boolean {
    return writable && f.editable;
  }
  function onEdit(f: MetaField, v: string) {
    const k = ekey(f);
    if (v === f.value) {
      const { [k]: _drop, ...rest } = edited;
      edited = rest;
    } else {
      edited = { ...edited, [k]: v };
    }
  }

  const buildEdits = () => buildMetaEdits(edited);

  async function save() {
    if (!primary || !dirty || saving) return;
    const edits = buildEdits();
    const targets = applyToAll ? files : [primary];
    saving = true;
    notice = "";
    try {
      for (const f of targets) {
        const res = unwrap(await commands.metadataWrite(f.path, edits));
        if (f.path === primary.path) fields = res;
      }
      edited = {};
      notice = targets.length > 1 ? $t("studio.savedN", { n: targets.length }) : $t("studio.saved");
    } catch (e) {
      notice = String(e);
    } finally {
      saving = false;
    }
  }

  function close() {
    dispatch("close");
  }
  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") close();
  }
</script>

<svelte:window on:keydown={onKeydown} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={close}>
  <!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" on:click|stopPropagation>
    <header>
      <h2>{$t("studio.title")}</h2>
      <button class="x" title={$t("common.close")} on:click={close}><Icon name="close" size={14} /></button>
    </header>

    {#if !primary}
      <p class="muted">{$t("studio.noFile")}</p>
    {:else}
      <div class="filename" title={primary.path}>
        <Icon name="info" size={14} />
        <span>{primary.name}</span>
        {#if !writable}<span class="ro-badge">{$t("studio.viewOnly")}</span>{/if}
      </div>

      {#if loading}
        <p class="muted">{$t("studio.loading")}</p>
      {:else if error}
        <p class="error">{error}</p>
      {:else if fields.length === 0}
        <p class="muted">{$t("studio.noMeta")}</p>
      {:else}
        {#if groups.length > 1}
          <div class="tabs" role="tablist">
            {#each groups as g}
              <button
                class="tab"
                class:active={g === activeGroup}
                role="tab"
                aria-selected={g === activeGroup}
                on:click={() => (activeGroup = g)}
              >
                {groupLabel(g)}
              </button>
            {/each}
          </div>
        {/if}

        <div class="fields">
          {#each shown as f (ekey(f))}
            <div class="row" class:changed={ekey(f) in edited}>
              <label class="k" for={`mf-${ekey(f)}`}>{f.key}</label>
              {#if editable(f)}
                <input
                  id={`mf-${ekey(f)}`}
                  class="v"
                  value={currentValue(f)}
                  on:input={(e) => onEdit(f, e.currentTarget.value)}
                />
              {:else}
                <span class="v ro" title={writable ? $t("studio.fieldReadonly") : $t("studio.viewOnly")}>
                  {currentValue(f) || "—"}
                </span>
              {/if}
            </div>
          {/each}
        </div>
      {/if}
    {/if}

    <footer>
      <div class="left">
        {#if writable && files.length > 1}
          <label class="all">
            <input type="checkbox" bind:checked={applyToAll} />
            {$t("studio.applyAll", { n: files.length })}
          </label>
        {/if}
        {#if notice}<span class="notice">{notice}</span>{/if}
      </div>
      <div class="right">
        <button class="btn" on:click={close}>{$t("common.close")}</button>
        {#if writable}
          <button class="btn primary" disabled={!dirty || saving} on:click={save}>
            {saving ? $t("studio.saving") : $t("studio.save")}
          </button>
        {/if}
      </div>
    </footer>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 60;
  }
  .dialog {
    width: min(560px, 92vw);
    max-height: 86vh;
    display: flex;
    flex-direction: column;
    background: var(--surface);
    color: var(--text);
    border: 1px solid var(--dialog-border);
    border-radius: 10px;
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.35);
    padding: 16px 18px 14px;
    overflow: hidden;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-bottom: 12px;
    border-bottom: 1px solid var(--border);
  }
  h2 {
    margin: 0;
    font-size: 15px;
  }
  .x {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    border: 1px solid transparent;
    border-radius: var(--radius);
    background: transparent;
    color: var(--text);
    cursor: pointer;
  }
  .x:hover {
    background: var(--surface-alt);
    border-color: var(--border);
  }
  .filename {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 12px 0 8px;
    font-weight: 600;
    color: var(--text);
  }
  .ro-badge {
    margin-left: 6px;
    padding: 1px 7px;
    font-size: 11px;
    font-weight: 500;
    white-space: nowrap;
    border-radius: 999px;
    background: var(--surface-alt);
    color: var(--text-dim);
    border: 1px solid var(--border);
  }
  .muted {
    color: var(--text-dim);
    padding: 10px 2px;
  }
  .error {
    color: var(--danger);
    padding: 8px 2px;
  }
  /* Tab standard: accent top-bar + surface on the active tab; inactive as recessed chips. */
  .tabs {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin: 4px 0 10px;
    border-bottom: 1px solid var(--border);
  }
  .tab {
    flex: 0 0 auto;
    white-space: nowrap;
    padding: 7px 14px;
    font-size: 12.5px;
    cursor: pointer;
    color: var(--text-dim);
    background: var(--surface-alt);
    border: 1px solid var(--border);
    border-bottom: none;
    border-top: 2px solid transparent;
    border-radius: 7px 7px 0 0;
  }
  .tab.active {
    color: var(--text);
    background: var(--surface);
    border-top: 2px solid var(--accent);
  }
  .fields {
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 2px;
  }
  .row {
    display: grid;
    grid-template-columns: 150px 1fr;
    align-items: center;
    gap: 10px;
  }
  .k {
    color: var(--text-dim);
    font-size: 12.5px;
    text-align: right;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .v {
    font-size: 13px;
  }
  input.v {
    height: 30px;
    padding: 0 10px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--surface-alt);
    color: var(--text);
  }
  input.v:focus {
    outline: none;
    border-color: var(--accent);
  }
  .row.changed input.v {
    border-color: var(--accent);
  }
  .v.ro {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--text);
    opacity: 0.8;
  }
  footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-top: 12px;
    padding-top: 12px;
    border-top: 1px solid var(--border);
  }
  .left {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }
  .all {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12.5px;
    color: var(--text-dim);
  }
  .notice {
    font-size: 12px;
    color: var(--text-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .right {
    display: flex;
    gap: 8px;
  }
  .btn {
    height: 32px;
    padding: 0 16px;
    border-radius: var(--radius);
    border: 1px solid var(--border-strong);
    background: var(--surface-alt);
    color: var(--text);
    cursor: pointer;
  }
  .btn:hover:not(:disabled) {
    border-color: var(--accent);
  }
  .btn.primary {
    background: var(--accent);
    color: var(--accent-contrast, #fff);
    border-color: var(--accent);
  }
  .btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
