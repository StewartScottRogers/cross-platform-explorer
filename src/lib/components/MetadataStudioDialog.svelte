<script lang="ts">
  // Metadata Studio (CPE-1041, epic CPE-725): an editable, tabbed metadata inspector spanning the read
  // codecs (ID3/Vorbis audio, EXIF image, PDF doc, MP4 video). Editable formats (mp3/flac today) can be
  // changed and saved back via `metadata_write`; the rest are shown read-only until their write codecs
  // land. Follows the dialog convention (visible border) and the tab standard (accent active tab).
  // CPE-1325: `save()` takes a best-effort `checkpointCreate` of the target folder ONCE before the write
  // (batch-safe: single file and applyToAll both checkpoint once, before the loop). A checkpoint failure
  // never blocks the write — it's a bonus safety net, not a gate — and success is surfaced only as a
  // subtle suffix on the existing "Saved" notice, never a blocking modal (mirrors SimilarImagesDialog's
  // pre-move checkpoint).
  import { createEventDispatcher, onMount } from "svelte";
  import { unwrap } from "../invoke";
  import { commands, type MetaField } from "../bindings.gen";
  import { joinFieldKey, buildMetaEdits, stageFieldEdits, isFieldDirty, revertFieldEdit } from "../metaEdits";
  import { parentDir } from "../contentSearch";
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
    iptc: "studio.tabIptc",
    xmp: "studio.tabXmp",
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

  // `e` defaults to the outer `edited` but MUST also be passed explicitly at every call site in the
  // template (`currentValue(f, edited)`, not `currentValue(f)`): Svelte's reactive dependency tracking
  // for a template expression is a static scan of the identifiers written directly in that expression —
  // it doesn't see into a called function's body — so a call that only mentions `f` would never be
  // recognized as depending on `edited` and the bound DOM value would go stale the moment `edited` is
  // set from anywhere other than that same input's own `on:input` (which sets the DOM value natively via
  // the browser, masking the gap). CPE-1326's batch ops are the first callers that mutate `edited`
  // programmatically for fields the user never typed into, which is what surfaced this.
  function currentValue(f: MetaField, e: Record<string, string> = edited): string {
    const k = ekey(f);
    return k in e ? e[k] : f.value;
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

  // NB: must reference `writable` directly (not just via the `editable(f)` closure) — Svelte's reactive
  // statement dependency tracking is a static textual scan, and `load()` assigns `fields` and `writable`
  // in two separate awaited steps, so a statement that only mentions `fields` would compute against a
  // stale `writable` and never re-run when it flips true a tick later.
  $: editableFields = writable ? fields.filter((f) => f.editable) : [];

  /** "Strip editable metadata" (CPE-1326): stage every editable field to empty, which `buildMetaEdits`
   *  turns into a `clear` edit — reusing the existing clear path, not a new mechanism. This only stages
   *  pending edits; it applies to whatever `targets` `save()` would use (primary alone, or the whole
   *  selection when "Apply to all" is checked), so the CPE-1325 pre-save checkpoint still fires and the
   *  user still has to hit Save. */
  function stripEditable() {
    if (!primary || saving || editableFields.length === 0) return;
    edited = { ...edited, ...stageFieldEdits(editableFields, () => "") };
  }

  /** "Copy from first" (CPE-1326): stage the primary file's own editable values as edits, and force
   *  "Apply to all" on — copying the primary onto itself is a no-op, the point is pushing its values onto
   *  the OTHER selected files. Writability is still respected: only fields this dialog already treats as
   *  editable for the primary are staged, and the unchanged `metadataWrite` call validates every target
   *  file/format on save exactly as the existing "Apply to all" path already does. */
  function copyFromFirst() {
    if (!primary || saving || editableFields.length === 0 || files.length < 2) return;
    edited = { ...edited, ...stageFieldEdits(editableFields, (f) => f.value) };
    applyToAll = true;
  }

  /** Per-field revert (CPE-1327): discard just this field's pending edit, restoring the value loaded from
   *  disk. Mirrors CPE-1326's reactivity fix — reassign `edited` to a new record (via `revertFieldEdit`)
   *  rather than mutating it, so the bound input picks up the change via `currentValue(f, edited)`. This
   *  never writes to disk and never touches the CPE-1325 checkpoint — it's a pure client-side discard. */
  function revertField(f: MetaField) {
    if (saving) return;
    edited = revertFieldEdit(f, edited);
  }

  /** "Reset all edits" (CPE-1327): discard every pending edit — across the whole selection when
   *  applyToAll is on, since edits are staged once in `edited` and applied uniformly to `targets` at save
   *  time (the same shared state CPE-1326's batch ops stage into). No disk write, no checkpoint — this
   *  only clears client-side staged state back to the loaded-from-disk values. */
  function resetAllEdits() {
    if (saving || !dirty) return;
    edited = {};
  }

  async function save() {
    if (!primary || !dirty || saving) return;
    const edits = buildEdits();
    const targets = applyToAll ? files : [primary];
    saving = true;
    notice = "";
    // Best-effort checkpoint of the target folder BEFORE this mutates any file (CPE-1325): metadata
    // write-back has no undo, so a bad edit should still be recoverable. Taken ONCE before the whole
    // batch (single file or applyToAll) begins — never per-field, never per-file. A checkpoint failure
    // must never block the write; it's a bonus safety net, not a gate — log it and proceed.
    let checkpointed = false;
    try {
      // `unwrap` throws on a `{status:"error"}` envelope too, not just a thrown `Error` (CPE-1328): a
      // Rust-side `Err(String)` rejects the raw promise with a plain string, which the generated
      // binding only rethrows when it's an `Error` instance — otherwise it resolves to
      // `{status:"error"}` WITHOUT throwing. Without unwrapping, that resolved-but-failed case would
      // fall straight through to `checkpointed = true` and the UI would claim a checkpoint that never
      // happened.
      unwrap(await commands.checkpointCreate(parentDir(primary.path), "Before metadata edit"));
      checkpointed = true;
    } catch (e) {
      console.error("Metadata Studio: pre-save checkpoint failed (proceeding with write)", e);
    }
    try {
      for (const f of targets) {
        const res = unwrap(await commands.metadataWrite(f.path, edits));
        if (f.path === primary.path) fields = res;
      }
      edited = {};
      const saved = targets.length > 1 ? $t("studio.savedN", { n: targets.length }) : $t("studio.saved");
      notice = checkpointed ? `${saved} ${$t("studio.checkpointed")}` : saved;
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
  <div class="dialog" role="dialog" aria-modal="true" aria-label={$t("studio.title")} on:click|stopPropagation>
    <header>
      <h2>{$t("studio.title")}</h2>
      <button class="x" data-testid="ms-close-btn" title={$t("common.close")} on:click={close}><Icon name="close" size={14} /></button>
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

        <div class="fields" data-testid="ms-fields">
          {#each shown as f (ekey(f))}
            <div class="row" data-testid="ms-field-row" class:changed={isFieldDirty(f, edited)}>
              <label class="k" for={`mf-${ekey(f)}`}>{f.key}</label>
              {#if editable(f)}
                <div class="vwrap">
                  <input
                    id={`mf-${ekey(f)}`}
                    class="v"
                    data-testid="ms-field-input"
                    value={currentValue(f, edited)}
                    on:input={(e) => onEdit(f, e.currentTarget.value)}
                  />
                  {#if isFieldDirty(f, edited)}
                    <button
                      type="button"
                      class="revert"
                      title={$t("studio.revertFieldHint")}
                      aria-label={$t("studio.revertFieldAria", { field: f.key })}
                      on:click={() => revertField(f)}
                    >
                      <Icon name="refresh" size={13} />
                    </button>
                  {/if}
                </div>
              {:else}
                <span class="v ro" title={writable ? $t("studio.fieldReadonly") : $t("studio.viewOnly")}>
                  {currentValue(f, edited) || "—"}
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
          {#if editableFields.length > 0}
            <button
              class="btn ghost"
              disabled={saving}
              title={$t("studio.stripEditableHint")}
              on:click={stripEditable}
            >
              {$t("studio.stripEditable")}
            </button>
            <button
              class="btn ghost"
              disabled={saving}
              title={$t("studio.copyFromFirstHint")}
              on:click={copyFromFirst}
            >
              {$t("studio.copyFromFirst")}
            </button>
          {/if}
        {/if}
        {#if writable && dirty}
          <button
            class="btn ghost"
            disabled={saving}
            title={$t("studio.resetAllHint")}
            on:click={resetAllEdits}
          >
            {$t("studio.resetAll")}
          </button>
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
  /* Wraps the input + its per-field revert button (CPE-1327) so the revert control sits inline without
     disturbing the row's 150px/1fr grid — the wrapper fills the 1fr column exactly as the input alone
     used to, and lays its two children out in a row internally. */
  .vwrap {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  input.v {
    flex: 1 1 auto;
    min-width: 0;
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
  /* Per-field revert (CPE-1327): only rendered while the field is dirty, so no layout reserved for it
     on untouched rows. Small icon-only ghost button, consistent with the other inline affordances. */
  .revert {
    flex: 0 0 auto;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    padding: 0;
    border: 1px solid transparent;
    border-radius: var(--radius);
    background: transparent;
    color: var(--text-dim);
    cursor: pointer;
  }
  .revert:hover {
    color: var(--text);
    border-color: var(--border);
    background: var(--surface-alt);
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
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 10px;
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
  /* Compact secondary action for the batch ops (CPE-1326) — sits inline with the "Apply to all"
     checkbox rather than competing with the primary Close/Save pair on the right. */
  .btn.ghost {
    height: auto;
    padding: 2px 8px;
    font-size: 12px;
    border-color: transparent;
    background: transparent;
    color: var(--text-dim);
  }
  .btn.ghost:hover:not(:disabled) {
    color: var(--text);
    border-color: var(--border);
  }
</style>
