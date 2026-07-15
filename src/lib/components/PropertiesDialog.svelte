<script lang="ts">
  import { createEventDispatcher, onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Icon from "./Icon.svelte";
  import { formatSize } from "../format";
  import { formatDate } from "../datetime";
  import { iconFor, typeName } from "../filetypes";
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
      <h2>Properties</h2>
      <button class="x" title="Close" on:click={close}><Icon name="close" size={14} /></button>
    </header>

    {#if error}
      <p class="error">{error}</p>
    {:else if single}
      <div class="hero">
        <Icon name={iconFor(single)} size={48} />
        <span class="fname">{single.name}</span>
      </div>
      <dl>
        <div><dt>Type</dt><dd>{typeName(single)}</dd></div>
        <div><dt>Location</dt><dd class="path">{single.path}</dd></div>
        {#if single.is_dir}
          <div>
            <dt>Size</dt>
            <dd>
              {#if sizing}Calculating…
              {:else if folderSize !== null}{formatSize(folderSize) || "0 B"} ({folderSize.toLocaleString()} bytes)
              {:else}Unavailable{/if}
            </dd>
          </div>
        {:else}
          <div>
            <dt>Size</dt>
            <dd>{formatSize(single.size) || "0 B"} ({single.size.toLocaleString()} bytes)</dd>
          </div>
        {/if}
        {#if info}
          <div><dt>Created</dt><dd>{formatDate(info.created) || "—"}</dd></div>
          <div><dt>Modified</dt><dd>{formatDate(info.modified) || "—"}</dd></div>
          <div>
            <dt>Attributes</dt>
            <dd>
              {[info.readonly ? "Read-only" : null, info.hidden ? "Hidden" : null]
                .filter(Boolean)
                .join(", ") || "None"}
            </dd>
          </div>
        {/if}
        {#if !single.is_dir}
          <div>
            <dt>SHA-256</dt>
            <dd class="checksum">
              {#if checksum}
                <code class="hash">{checksum}</code>
                <button class="mini" on:click={copyHash} title="Copy checksum to clipboard">
                  <Icon name={copied ? "check" : "copy"} size={13} />
                  {copied ? "Copied" : "Copy"}
                </button>
              {:else if hashing}
                <span class="dim">Computing…</span>
              {:else if hashError}
                <span class="err-inline">{hashError}</span>
              {:else}
                <button class="mini" on:click={computeHash}>Compute</button>
              {/if}
            </dd>
          </div>
        {/if}
      </dl>
    {:else}
      <div class="hero">
        <Icon name="folder" size={48} />
        <span class="fname">{entries.length} items selected</span>
      </div>
      <dl>
        <div><dt>Folders</dt><dd>{folderCount}</dd></div>
        <div><dt>Files</dt><dd>{fileCount}</dd></div>
        <div>
          <dt>Size of files</dt>
          <dd>{formatSize(totalSize) || "0 B"} ({totalSize.toLocaleString()} bytes)</dd>
        </div>
        {#if folderCount > 0}
          <div><dt>Note</dt><dd class="dim">Folder contents are not included in the total.</dd></div>
        {/if}
      </dl>
    {/if}

    <div class="actions">
      <button class="btn primary" on:click={close}>Close</button>
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
  .actions { display: flex; justify-content: flex-end; padding-top: 8px; }
  .btn { height: 32px; padding: 0 16px; border-radius: var(--radius);
         border: 1px solid var(--border-strong); background: var(--surface-alt); }
  .btn.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
  .btn.primary:hover { background: var(--accent-hover); }
  .error { color: #c42b1c; padding: 12px 0; }
</style>
