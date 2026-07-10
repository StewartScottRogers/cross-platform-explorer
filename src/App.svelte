<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { check } from "@tauri-apps/plugin-updater";
  import { relaunch } from "@tauri-apps/plugin-process";

  interface DirEntry {
    name: string;
    path: string;
    is_dir: boolean;
    size: number;
  }

  let currentPath = "";
  let entries: DirEntry[] = [];
  let error = "";
  let updateStatus = "";

  async function load(path: string) {
    error = "";
    try {
      entries = await invoke<DirEntry[]>("list_dir", { path });
      currentPath = path;
    } catch (e) {
      error = String(e);
    }
  }

  async function open(entry: DirEntry) {
    if (entry.is_dir) {
      await load(entry.path);
    }
  }

  async function goHome() {
    const home = await invoke<string>("home_dir");
    await load(home);
  }

  async function goUp() {
    try {
      const parent = await invoke<string | null>("parent_dir", {
        path: currentPath,
      });
      if (parent) await load(parent);
    } catch (e) {
      error = String(e);
    }
  }

  function formatSize(bytes: number): string {
    if (bytes === 0) return "";
    const units = ["B", "KB", "MB", "GB", "TB"];
    let i = 0;
    let n = bytes;
    while (n >= 1024 && i < units.length - 1) {
      n /= 1024;
      i++;
    }
    return `${n.toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
  }

  // Check for updates on launch. Silent no-op if none / dev build.
  async function checkForUpdates() {
    try {
      const update = await check();
      if (update) {
        updateStatus = `Update ${update.version} available — downloading…`;
        await update.downloadAndInstall();
        updateStatus = "Update installed. Restarting…";
        await relaunch();
      }
    } catch (e) {
      // Updater is not configured in dev; ignore.
      console.debug("update check skipped:", e);
    }
  }

  onMount(() => {
    goHome();
    checkForUpdates();
  });

  $: sorted = [...entries].sort((a, b) => {
    if (a.is_dir !== b.is_dir) return a.is_dir ? -1 : 1;
    return a.name.localeCompare(b.name);
  });
</script>

<div class="toolbar">
  <button on:click={goUp} title="Up one level">↑</button>
  <button on:click={goHome} title="Home">⌂</button>
  <span class="path">{currentPath}</span>
</div>

<div class="listing">
  {#each sorted as entry (entry.path)}
    <div class="entry" on:dblclick={() => open(entry)} role="button" tabindex="0">
      <span class="icon">{entry.is_dir ? "📁" : "📄"}</span>
      <span class="name">{entry.name}</span>
      <span class="size">{entry.is_dir ? "" : formatSize(entry.size)}</span>
    </div>
  {/each}
</div>

<div class="status">
  {#if error}
    <span class="error">{error}</span>
  {:else}
    {sorted.length} items{updateStatus ? ` — ${updateStatus}` : ""}
  {/if}
</div>
