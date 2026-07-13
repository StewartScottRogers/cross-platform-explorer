<script lang="ts">
  /**
   * The torn-off preview window (CPE-234). Runs the app in "float mode"
   * (index.html?float=1). It shows pinned file previews as tabs: the main window
   * emits `float:add` with a DirEntry; we add a tab. Each tab reuses the same
   * PreviewPane as the in-app pane. Tabs are close-only; closing the last one
   * closes the window.
   */
  import { onMount, onDestroy } from "svelte";
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { listen, emit } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import PreviewPane from "./PreviewPane.svelte";
  import DetailsPane from "./DetailsPane.svelte";
  import Icon from "./Icon.svelte";
  import { iconFor } from "../filetypes";
  import {
    loadPreviewText, loadArchiveEntries, loadPreviewInfo, loadImageData, savePreviewText,
  } from "../preview/loaders";
  import type { DirEntry } from "../types";

  interface Tab { id: number; entry: DirEntry }
  let tabs: Tab[] = [];
  let activeId = -1;
  let nextId = 1;

  function addTab(entry: DirEntry) {
    // Popping out a file already pinned just focuses its tab (no duplicates).
    const existing = tabs.find((t) => t.entry.path === entry.path);
    if (existing) { activeId = existing.id; return; }
    const tab = { id: nextId++, entry };
    tabs = [...tabs, tab];
    activeId = tab.id;
  }

  function closeTab(id: number) {
    const idx = tabs.findIndex((t) => t.id === id);
    tabs = tabs.filter((t) => t.id !== id);
    if (tabs.length === 0) { void getCurrentWindow().close(); return; }
    if (activeId === id) activeId = tabs[Math.max(0, idx - 1)].id;
  }

  $: active = tabs.find((t) => t.id === activeId) ?? null;

  let unlisten: (() => void) | undefined;
  onMount(async () => {
    // Register the listener BEFORE announcing readiness so the first file the
    // main window sends is never missed.
    unlisten = await listen<DirEntry>("float:add", (e) => addTab(e.payload));
    await emit("float:ready", {});
  });
  onDestroy(() => unlisten?.());
</script>

<div class="float-root">
  <div class="float-tabs" role="tablist" aria-label="Pinned previews">
    {#each tabs as t (t.id)}
      <div class="float-tab" class:active={t.id === activeId}>
        <span class="ft-icon"><Icon name={iconFor(t.entry)} size={14} /></span>
        <button class="ft-label" role="tab" aria-selected={t.id === activeId}
          on:click={() => (activeId = t.id)} title={t.entry.path}>{t.entry.name}</button>
        <button class="ft-close" title="Close tab" on:click={() => closeTab(t.id)}>
          <Icon name="close" size={12} />
        </button>
      </div>
    {/each}
  </div>

  <div class="float-body">
    {#if active}
      {#key active.id}
        <PreviewPane
          entry={active.entry}
          assetUrl={convertFileSrc}
          loadText={loadPreviewText}
          loadEntries={loadArchiveEntries}
          loadInfo={loadPreviewInfo}
          loadImageData={loadImageData}
          saveText={savePreviewText}
        >
          <DetailsPane selected={[active.entry]} folderName="" itemCount={0} />
        </PreviewPane>
      {/key}
    {:else}
      <div class="float-empty">No preview pinned.</div>
    {/if}
  </div>
</div>

<style>
  /* A framed panel inset from the window edge so the float reads as a finished
     card rather than bleeding to the OS chrome (CPE-245). */
  .float-root {
    height: 100vh;
    display: flex;
    flex-direction: column;
    background: var(--bg);
    padding: 8px;
    gap: 0;
  }
  .float-tabs {
    display: flex; gap: 2px; padding: 6px 6px 0;
    background: var(--surface-alt);
    border: 1px solid var(--border-strong);
    border-bottom: 1px solid var(--border);
    border-radius: var(--radius-lg) var(--radius-lg) 0 0;
    overflow-x: auto; flex: none;
  }
  .float-tab {
    display: flex; align-items: center; gap: 4px;
    height: 32px; padding: 0 4px 0 8px;
    border-radius: var(--radius) var(--radius) 0 0;
    color: var(--text-dim); max-width: 200px; flex: none;
  }
  .float-tab.active {
    background: var(--surface); color: var(--text);
    box-shadow: 0 -1px 0 var(--border), 1px 0 0 var(--border), -1px 0 0 var(--border);
  }
  .ft-icon { flex: none; display: grid; place-items: center; }
  .ft-label {
    flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    text-align: left; background: transparent; color: inherit;
  }
  .ft-close { width: 20px; height: 20px; display: grid; place-items: center; border-radius: 4px; flex: none; opacity: 0.7; }
  .ft-close:hover { background: var(--border-strong); opacity: 1; }
  .float-body {
    flex: 1; min-height: 0; display: flex;
    background: var(--surface);
    border: 1px solid var(--border-strong);
    border-top: none;
    border-radius: 0 0 var(--radius-lg) var(--radius-lg);
    overflow: hidden;
  }
  .float-body :global(.preview-pane),
  .float-body :global(.preview),
  .float-body :global(.details) { flex: 1; min-height: 0; }
  .float-empty { flex: 1; display: grid; place-items: center; color: var(--text-faint); }
</style>
