<script lang="ts">
  /**
   * FolderBrowser (CPE-1426): renders a highlighted DIRECTORY's contents one level down, inside the
   * PREVIEW PANE — a "peek" that lets the user walk a folder tree by clicking subfolders without the
   * main pane navigating first (Miller-columns / macOS Finder column-view feel). Wired in from
   * `PreviewPane.svelte`'s `folder` provider kind (`../preview/provider.ts`).
   *
   * Reuses the exact streaming listing command the main pane's `ExplorerPane.loadListing` calls
   * (`list_dir_stream`, collect-to-vec fallback `list_dir` — see docs/design/STREAMING.md) rather than
   * reinventing a fetch path, so a huge peeked folder still paints its first rows immediately. The main
   * pane's file-type filter never applies here — this always shows everything one level down, per the
   * ticket.
   *
   * Every row click (file or folder) fires `pick` with `{ parent: this folder's path, entry: the
   * clicked row }`. The CALLER (App.svelte, via PreviewPane's bare event-forwarding) drives the actual
   * descent: `pendingSelectPath = entry.path; navigate(parent);` — the exact mechanism
   * `revealFileInApp` already uses for search-hit reveals, reused rather than forked. A double-click on
   * a FILE row additionally fires `open` so the caller can hand it to the normal open flow (external
   * app / archive-enter / vault-unlock, whichever `open()` already does); folders don't get a separate
   * double-click behaviour since a single click already descends exactly one level (per the ticket).
   */
  import { onDestroy, createEventDispatcher } from "svelte";
  import { rawInvoke, createChannel } from "../invoke";
  import { sortEntries } from "../sort";
  import { iconFor } from "../filetypes";
  import { formatSize } from "../format";
  import Icon from "./Icon.svelte";
  import { t } from "../i18n";
  import { displaySafeName } from "../filename";
  import type { DirEntry } from "../types";

  /** The highlighted folder's path — this component peeks INTO it (lists its own contents). */
  export let path = "";

  const dispatch = createEventDispatcher<{
    pick: { parent: string; entry: DirEntry };
    open: { parent: string; entry: DirEntry };
  }>();

  /** Debounce (CPE-1426 default ~150ms): fast arrow-key scrolling through folders re-points `path`
   *  rapidly — only the SETTLED selection actually triggers a filesystem listing. */
  const DEBOUNCE_MS = 150;

  // A private stream-id space, well clear of any `<ExplorerPane>` instance's own small sequential
  // `loadGen` counter, so this component's `cancel_dir_stream` calls can never collide with (and
  // wrongly cancel) a live main-pane or pane-B listing stream — `cancel_dir_stream`'s registry is keyed
  // globally by the raw numeric id (STREAMING.md), not per-caller.
  const STREAM_ID_BASE = 1_000_000_000;

  let entries: DirEntry[] = [];
  let state: "loading" | "idle" | "error" = "idle";
  let loadedPath = "";
  let gen = 0;
  let debounceTimer: ReturnType<typeof setTimeout> | undefined;

  $: if (path && path !== loadedPath) scheduleLoad(path);
  $: if (!path) {
    loadedPath = "";
    entries = [];
    state = "idle";
  }

  function scheduleLoad(p: string): void {
    loadedPath = p;
    if (debounceTimer !== undefined) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => {
      debounceTimer = undefined;
      void load(p);
    }, DEBOUNCE_MS);
  }

  async function load(p: string): Promise<void> {
    const mine = ++gen;
    // Cancel our own previous in-flight walk (if any) — mirrors `ExplorerPane.loadListing`'s own
    // gen-bump cancel, so navigating the peek away from a huge folder stops that walk instead of
    // letting it run to completion unread.
    if (mine > 1) rawInvoke("cancel_dir_stream", { streamId: STREAM_ID_BASE + mine - 1 }).catch(() => {});
    entries = [];
    state = "loading";
    try {
      const channel = createChannel<DirEntry[]>();
      channel.onmessage = (batch) => {
        if (mine !== gen) return; // superseded — drop stale rows
        entries = entries.concat(batch);
        state = "idle"; // first rows are in — reveal them (STREAMING.md convention)
      };
      await rawInvoke("list_dir_stream", { path: p, streamId: STREAM_ID_BASE + mine, onEntry: channel });
      if (mine !== gen) return;
      state = "idle";
    } catch {
      if (mine !== gen) return;
      entries = [];
      state = "error"; // shown as a plain note below — never an error dialog (ticket AC)
    }
  }

  onDestroy(() => {
    if (debounceTimer !== undefined) clearTimeout(debounceTimer);
    if (gen > 0) rawInvoke("cancel_dir_stream", { streamId: STREAM_ID_BASE + gen }).catch(() => {});
  });

  $: sorted = sortEntries(entries, "name", "asc", true);

  function onRowClick(entry: DirEntry): void {
    dispatch("pick", { parent: path, entry });
  }
  function onRowDblClick(entry: DirEntry): void {
    if (entry.is_dir) return; // a single click already descends one level into it
    dispatch("open", { parent: path, entry });
  }
  function onRowKeydown(e: KeyboardEvent, entry: DirEntry): void {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onRowClick(entry);
    }
  }
</script>

<div class="folder-browser">
  {#if state === "loading" && sorted.length === 0}
    <p class="fb-note">{$t("pv.loading")}</p>
  {:else if state === "error"}
    <p class="fb-note">{$t("pv.folder.cantOpen")}</p>
  {:else if sorted.length === 0}
    <p class="fb-note">{$t("fl.empty")}</p>
  {:else}
    <div class="fb-rows">
      {#each sorted as entry (entry.path)}
        <!-- svelte-ignore a11y-click-events-have-key-events -->
        <div
          class="fb-row"
          role="button"
          tabindex="0"
          on:click={() => onRowClick(entry)}
          on:dblclick={() => onRowDblClick(entry)}
          on:keydown={(e) => onRowKeydown(e, entry)}
        >
          <Icon name={iconFor(entry)} size={16} />
          <span class="fb-name">{displaySafeName(entry.name)}</span>
          {#if !entry.is_dir}<span class="fb-size">{formatSize(entry.size)}</span>{/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .folder-browser {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: auto;
  }
  .fb-rows {
    display: flex;
    flex-direction: column;
    padding: 4px 0;
  }
  .fb-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 12px;
    font-size: 12px;
    color: var(--text);
    cursor: pointer;
    white-space: nowrap;
  }
  .fb-row:hover,
  .fb-row:focus-visible {
    background: var(--surface-alt);
    outline: none;
  }
  .fb-name {
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 1 1 auto;
    min-width: 0;
  }
  .fb-size {
    color: var(--text-dim);
    flex: 0 0 auto;
    font-variant-numeric: tabular-nums;
  }
  .fb-note {
    margin: auto;
    color: var(--text-faint);
    padding: 12px;
  }
</style>
