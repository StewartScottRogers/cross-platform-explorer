<script lang="ts">
  import { createEventDispatcher, onMount } from "svelte";
  import { invoke } from "../invoke";
  import Icon from "./Icon.svelte";
  import { t } from "../i18n";
  import { formatSize } from "../format";
  import { formatDate } from "../datetime";
  import { iconFor, typeName, categoryOf, isImage } from "../filetypes";
  import { checksumMatches } from "../checksum";
  import type { DirEntry } from "../types";

  /** The selected entries. One => full detail; many => aggregate summary. */
  export let entries: DirEntry[] = [];

  const dispatch = createEventDispatcher<{ close: void }>();

  interface Info {
    name: string;
    path: string;
    is_dir: boolean;
    size: number;
    modified: number | null;
    created: number | null;
    readonly: boolean;
    hidden: boolean;
  }

  let info: Info | null = null;
  let error = "";
  let folderSize: number | null = null;
  let sizing = false;
  let cancelled = false;

  // On-demand SHA-256 checksum (CPE-412) — hashing is I/O-bound, so it's opt-in, never automatic.
  let checksum = "";
  let hashing = false;
  let hashError = "";
  let copied = false;
  // Verify against a pasted expected digest (CPE-413). `null` verdict = neutral (nothing entered).
  let expected = "";
  $: verdict = checksum ? checksumMatches(checksum, expected) : null;

  // On-demand text stats (CPE-414) — offered only for text/code files; opt-in, like the checksum.
  interface Stats { lines: number; words: number; chars: number; bytes: number }
  let stats: Stats | null = null;
  let statting = false;
  let statError = "";
  $: isTextFile = !!single && !single.is_dir && ["text", "code"].includes(categoryOf(single));

  async function computeStats() {
    if (!single) return;
    statting = true;
    statError = "";
    stats = null;
    try {
      stats = await invoke<Stats>("text_stats", { path: single.path });
    } catch (e) {
      statError = String(e);
    } finally {
      statting = false;
    }
  }

  async function computeHash() {
    if (!single || single.is_dir) return;
    hashing = true;
    hashError = "";
    checksum = "";
    try {
      checksum = await invoke<string>("hash_file", { path: single.path });
    } catch (e) {
      hashError = String(e);
    } finally {
      hashing = false;
    }
  }

  async function copyHash() {
    try {
      await navigator.clipboard.writeText(checksum);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      /* clipboard unavailable — leave the digest on screen to copy manually */
    }
  }

  // Image dimensions + basic EXIF (CPE-659) — auto-loaded for a single image file, best-effort.
  interface ImageMeta {
    width: number | null;
    height: number | null;
    camera: string | null;
    lens: string | null;
    taken: string | null;
    iso: string | null;
    aperture: string | null;
    exposure: string | null;
    focal_length: string | null;
  }
  let imageMeta: ImageMeta | null = null;
  $: isImageFile = !!single && !single.is_dir && isImage(single.name);
  // Rows to render, in order, skipping fields with no data.
  $: imageRows = imageMeta
    ? ([
        [$t("prop.dimensions"), imageMeta.width && imageMeta.height ? `${imageMeta.width} × ${imageMeta.height}` : null],
        [$t("prop.camera"), imageMeta.camera],
        [$t("prop.lens"), imageMeta.lens],
        [$t("prop.dateTaken"), imageMeta.taken],
        [$t("prop.iso"), imageMeta.iso],
        [$t("prop.aperture"), imageMeta.aperture],
        [$t("prop.exposure"), imageMeta.exposure],
        [$t("prop.focalLength"), imageMeta.focal_length],
      ] as [string, string | null][]).filter(([, v]) => v)
    : [];

  $: single = entries.length === 1 ? entries[0] : null;
  $: totalSize = entries.reduce((n, e) => n + (e.is_dir ? 0 : e.size), 0);
  $: folderCount = entries.filter((e) => e.is_dir).length;
  $: fileCount = entries.length - folderCount;

  onMount(async () => {
    if (!single) return;
    try {
      info = await invoke<Info>("entry_info", { path: single.path });
    } catch (e) {
      error = String(e);
    }
    // Image metadata is best-effort — a failure just leaves the rows off, never blocks the dialog.
    if (!single.is_dir && isImage(single.name)) {
      try {
        imageMeta = await invoke<ImageMeta>("image_meta", { path: single.path });
      } catch {
        /* leave imageMeta null; the dialog just omits the media rows */
      }
    }
    // Folder sizes must be computed recursively, which can take a while on a
    // big tree — do it after the dialog is already showing, never blocking it.
    if (single.is_dir) {
      sizing = true;
      try {
        const n = await invoke<number>("dir_size", { path: single.path });
        if (!cancelled) folderSize = n;
      } catch {
        /* leave folderSize null; the dialog just omits it */
      } finally {
        sizing = false;
      }
    }
  });

  function close() {
    cancelled = true;
    dispatch("close");
  }
</script>

<svelte:window on:keydown={(e) => e.key === "Escape" && close()} />

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="backdrop" on:click={close}>
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions a11y-no-noninteractive-element-interactions -->
  <div class="dialog" role="dialog" aria-modal="true" on:click|stopPropagation>
    <header>
      <h2>{$t("prop.title")}</h2>
      <button class="x" title={$t("common.close")} on:click={close}><Icon name="close" size={14} /></button>
    </header>

    {#if error}
      <p class="error">{error}</p>
    {:else if single}
      <div class="hero">
        <Icon name={iconFor(single)} size={48} />
        <span class="fname">{single.name}</span>
      </div>
      <dl>
        <div><dt>{$t("prop.type")}</dt><dd>{typeName(single)}</dd></div>
        <div><dt>{$t("prop.location")}</dt><dd class="path">{single.path}</dd></div>
        {#if single.is_dir}
          <div>
            <dt>{$t("prop.size")}</dt>
            <dd>
              {#if sizing}{$t("prop.calculating")}
              {:else if folderSize !== null}{$t("prop.sizeBytes", { size: formatSize(folderSize) || "0 B", bytes: folderSize.toLocaleString() })}
              {:else}{$t("prop.unavailable")}{/if}
            </dd>
          </div>
        {:else}
          <div>
            <dt>{$t("prop.size")}</dt>
            <dd>{$t("prop.sizeBytes", { size: formatSize(single.size) || "0 B", bytes: single.size.toLocaleString() })}</dd>
          </div>
        {/if}
        {#if info}
          <div><dt>{$t("prop.created")}</dt><dd>{formatDate(info.created) || "—"}</dd></div>
          <div><dt>{$t("prop.modified")}</dt><dd>{formatDate(info.modified) || "—"}</dd></div>
          <div>
            <dt>{$t("prop.attributes")}</dt>
            <dd>
              {[info.readonly ? $t("prop.readonly") : null, info.hidden ? $t("prop.hidden") : null]
                .filter(Boolean)
                .join(", ") || $t("prop.none")}
            </dd>
          </div>
        {/if}
        {#if isImageFile && imageRows.length}
          {#each imageRows as [label, value]}
            <div><dt>{label}</dt><dd>{value}</dd></div>
          {/each}
        {/if}
        {#if !single.is_dir}
          <div>
            <dt>SHA-256</dt>
            <dd class="checksum">
              {#if checksum}
                <code class="hash">{checksum}</code>
                <button class="mini" on:click={copyHash} title={$t("prop.copyChecksumTip")}>
                  <Icon name={copied ? "check" : "copy"} size={13} />
                  {copied ? $t("prop.copied") : $t("prop.copy")}
                </button>
                <div class="verify">
                  <input
                    class="verify-in"
                    placeholder={$t("prop.pasteExpected")}
                    bind:value={expected}
                    spellcheck="false"
                    autocomplete="off"
                  />
                  {#if verdict === true}
                    <span class="match" title={$t("prop.matchTip")}>{$t("prop.match")}</span>
                  {:else if verdict === false}
                    <span class="nomatch" title={$t("prop.noMatchTip")}>{$t("prop.noMatch")}</span>
                  {/if}
                </div>
              {:else if hashing}
                <span class="dim">{$t("prop.computing")}</span>
              {:else if hashError}
                <span class="err-inline">{hashError}</span>
              {:else}
                <button class="mini" on:click={computeHash}>{$t("prop.compute")}</button>
              {/if}
            </dd>
          </div>
        {/if}
        {#if isTextFile}
          <div>
            <dt>{$t("prop.contents")}</dt>
            <dd class="checksum">
              {#if stats}
                <span>{$t("prop.contentStats", { lines: stats.lines.toLocaleString(), words: stats.words.toLocaleString(), chars: stats.chars.toLocaleString() })}</span>
              {:else if statting}
                <span class="dim">{$t("prop.counting")}</span>
              {:else if statError}
                <span class="err-inline">{statError}</span>
              {:else}
                <button class="mini" on:click={computeStats}>{$t("prop.count")}</button>
              {/if}
            </dd>
          </div>
        {/if}
      </dl>
    {:else}
      <div class="hero">
        <Icon name="folder" size={48} />
        <span class="fname">{$t("prop.itemsSelected", { count: entries.length })}</span>
      </div>
      <dl>
        <div><dt>{$t("prop.folders")}</dt><dd>{folderCount}</dd></div>
        <div><dt>{$t("prop.files")}</dt><dd>{fileCount}</dd></div>
        <div>
          <dt>{$t("prop.sizeOfFiles")}</dt>
          <dd>{$t("prop.sizeBytes", { size: formatSize(totalSize) || "0 B", bytes: totalSize.toLocaleString() })}</dd>
        </div>
        {#if folderCount > 0}
          <div><dt>{$t("prop.note")}</dt><dd class="dim">{$t("prop.folderNote")}</dd></div>
        {/if}
      </dl>
    {/if}

    <div class="actions">
      <button class="btn primary" on:click={close}>{$t("common.close")}</button>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed; inset: 0; background: rgba(0, 0, 0, 0.25);
    display: grid; place-items: center; z-index: 200;
  }
  .dialog {
    width: 460px; max-width: 92vw;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-radius: 10px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.25);
    padding: 16px 20px 20px;
  }
  header { display: flex; align-items: center; margin-bottom: 12px; }
  h2 { font-size: 16px; flex: 1; }
  .x { width: 28px; height: 28px; display: grid; place-items: center; }
  .hero {
    display: flex; align-items: center; gap: 12px;
    padding-bottom: 14px; border-bottom: 1px solid var(--border);
  }
  .fname { font-size: 15px; font-weight: 600; overflow-wrap: anywhere; }
  dl { padding: 12px 0; display: grid; gap: 8px; }
  dl > div { display: flex; gap: 12px; font-size: 13px; }
  dt { color: var(--text-dim); width: 110px; flex: none; }
  dd { flex: 1; overflow-wrap: anywhere; }
  dd.path { font-family: ui-monospace, monospace; font-size: 12px; }
  dd.dim { color: var(--text-faint); }
  dd.checksum { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .hash { font-family: ui-monospace, monospace; font-size: 11px; overflow-wrap: anywhere; }
  .err-inline { color: #c42b1c; }
  .dim { color: var(--text-faint); }
  .mini {
    display: inline-flex; align-items: center; gap: 5px;
    height: 24px; padding: 0 10px; border-radius: var(--radius);
    border: 1px solid var(--border-strong); background: var(--surface-alt); font-size: 12px;
  }
  .mini:hover { background: var(--surface); }
  .verify { display: flex; align-items: center; gap: 8px; flex-basis: 100%; margin-top: 2px; }
  .verify-in {
    flex: 1; min-width: 140px; height: 26px; padding: 0 8px;
    border: 1px solid var(--border-strong); border-radius: var(--radius);
    background: var(--surface-alt); font-family: ui-monospace, monospace; font-size: 11px;
  }
  .match { color: #1a7f37; font-weight: 600; white-space: nowrap; }
  .nomatch { color: #c42b1c; font-weight: 600; white-space: nowrap; }
  .actions { display: flex; justify-content: flex-end; padding-top: 8px; }
  .btn { height: 32px; padding: 0 16px; border-radius: var(--radius);
         border: 1px solid var(--border-strong); background: var(--surface-alt); }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
  .btn.primary:hover { background: var(--accent-hover); }
  .error { color: #c42b1c; padding: 12px 0; }
</style>
