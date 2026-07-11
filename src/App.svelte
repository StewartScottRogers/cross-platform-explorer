<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { check } from "@tauri-apps/plugin-updater";
  import { relaunch } from "@tauri-apps/plugin-process";
  import { openPath } from "@tauri-apps/plugin-opener";
  import { formatSize, friendlyError, splitPath } from "./lib/format";

  interface DirEntry {
    name: string;
    path: string;
    is_dir: boolean;
    size: number;
  }

  let currentPath = "";
  let entries: DirEntry[] = [];
  let error = "";
  let loading = false;
  let updateStatus = "";
  // Transient message shown in the status bar (e.g. failure to open a file).
  // Unlike `error`, this does not blank the listing.
  let notice = "";
  let noticeTimer: ReturnType<typeof setTimeout> | undefined;

  function showNotice(message: string) {
    notice = message;
    clearTimeout(noticeTimer);
    noticeTimer = setTimeout(() => (notice = ""), 4000);
  }

  // Index of the keyboard-selected row in `sorted`, or -1 for no selection.
  let selected = -1;
  let rowEls: HTMLElement[] = [];

  async function load(path: string) {
    error = "";
    loading = true;
    selected = -1; // reset selection whenever the directory changes
    try {
      entries = await invoke<DirEntry[]>("list_dir", { path });
      currentPath = path;
    } catch (e) {
      // Clear the stale listing and surface a friendly state, but still update
      // currentPath so the user can navigate back out of the failed folder.
      entries = [];
      currentPath = path;
      error = friendlyError(String(e));
    } finally {
      loading = false;
    }
  }

  async function open(entry: DirEntry) {
    if (entry.is_dir) {
      await load(entry.path);
      return;
    }
    // Files: hand off to the OS default application.
    try {
      await openPath(entry.path);
    } catch (e) {
      console.debug("openPath failed:", e);
      showNotice(`Can't open "${entry.name}" — no app is associated with this file type.`);
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

  // Keyboard navigation: arrows move the selection, Enter opens, Backspace goes up.
  function handleKeydown(event: KeyboardEvent) {
    // Don't hijack keys while the user is typing in an input.
    const target = event.target as HTMLElement | null;
    if (target && ["INPUT", "TEXTAREA"].includes(target.tagName)) return;

    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        if (sorted.length > 0) {
          selected = Math.min(selected + 1, sorted.length - 1);
        }
        break;
      case "ArrowUp":
        event.preventDefault();
        if (sorted.length > 0) {
          selected = selected <= 0 ? 0 : selected - 1;
        }
        break;
      case "Enter":
        // If a row itself has focus, its own keydown handler opens it —
        // bail out here so we don't open the same entry twice.
        if (target?.closest(".entry")) return;
        event.preventDefault();
        if (selected >= 0 && selected < sorted.length) {
          open(sorted[selected]);
        }
        break;
      case "Backspace":
        event.preventDefault();
        goUp();
        break;
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

  $: crumbs = splitPath(currentPath);

  // Keep the keyboard selection scrolled into view.
  $: if (selected >= 0 && rowEls[selected]) {
    rowEls[selected].scrollIntoView({ block: "nearest" });
  }
</script>

<svelte:window on:keydown={handleKeydown} />

<div class="toolbar">
  <button on:click={goUp} title="Up one level (Backspace)">↑</button>
  <button on:click={goHome} title="Home">⌂</button>
  <nav class="breadcrumbs" aria-label="Current path">
    {#each crumbs as crumb, i (crumb.path)}
      {#if i === crumbs.length - 1}
        <span class="crumb current" aria-current="page">{crumb.name}</span>
      {:else}
        <button class="crumb" on:click={() => load(crumb.path)}>{crumb.name}</button>
        <span class="crumb-sep" aria-hidden="true">›</span>
      {/if}
    {/each}
  </nav>
</div>

<div class="listing">
  {#if error}
    <div class="empty-state">
      <div class="empty-icon">🚫</div>
      <p>{error}</p>
    </div>
  {:else if loading}
    <div class="empty-state">
      <p>Loading…</p>
    </div>
  {:else if sorted.length === 0}
    <div class="empty-state">
      <div class="empty-icon">📂</div>
      <p>This folder is empty</p>
    </div>
  {:else}
    {#each sorted as entry, i (entry.path)}
      <div
        class="entry"
        class:selected={i === selected}
        bind:this={rowEls[i]}
        on:click={() => (selected = i)}
        on:dblclick={() => open(entry)}
        on:keydown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            selected = i;
            open(entry);
          }
        }}
        role="button"
        tabindex="0"
      >
        <span class="icon">{entry.is_dir ? "📁" : "📄"}</span>
        <span class="name">{entry.name}</span>
        <span class="size">{entry.is_dir ? "" : formatSize(entry.size)}</span>
      </div>
    {/each}
  {/if}
</div>

<div class="status">
  {#if error}
    <span class="error">{error}</span>
  {:else if notice}
    <span class="error">{notice}</span>
  {:else}
    {sorted.length} items{updateStatus ? ` — ${updateStatus}` : ""}
  {/if}
</div>
