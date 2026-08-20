<script lang="ts">
  // Browsable Trash (CPE-1560, epic CPE-1486 final slice) — lists what `list_trash_stream` finds sitting
  // in the OS Recycle Bin / Trash and offers Restore / Empty over it. Windows/Linux only: the Sidebar
  // gates the entry that opens this view on `canBrowseTrash` (mirrors `can_restore_from_trash`), so this
  // component only ever mounts where the backend commands exist.
  //
  // Streaming (STREAMING.md): consumes `list_trash_stream` over a raw channel via `rawInvoke`/
  // `createChannel` (never the busy-tracking `commands.listTrashStream`, which would hold the wait cursor
  // for the whole listing — see BUSY-CURSOR.md) so a large Trash paints progressively instead of blocking
  // on one big `Vec`. No virtualized DOM windowing here: per `listTrashStream`'s own doc comment, a
  // listing is bounded by what's literally sitting in the Recycle Bin — nowhere near the size of an
  // arbitrary directory tree — so the added complexity of `../virtualize` isn't worth it for this view.
  import { createEventDispatcher, onMount } from "svelte";
  import { createChannel, rawInvoke, unwrap } from "../invoke";
  import { commands } from "../bindings.gen";
  import type { TrashEntry, TrashStreamSummary } from "../bindings.gen";
  import Icon from "./Icon.svelte";
  import HelpButton from "./HelpButton.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import { t } from "../i18n";
  import { formatDate } from "../datetime";
  import { formatSize } from "../format";
  import { iconFor } from "../filetypes";
  import { displaySafeName, displaySafePath } from "../filename";

  // "help" isn't in this dispatcher type — it's never dispatched directly, only forwarded verbatim from
  // HelpButton's own typed `Section` event via the bare `on:help` in the markup below (same convention
  // as WorkbenchView.svelte).
  const dispatch = createEventDispatcher<{ close: void }>();

  let entries: TrashEntry[] = [];
  let loading = true;
  let error = "";
  /** CPE-1803: true when the last completed listing pass couldn't fully read the OS trash and degraded
   *  to no entries (CPE-1791) rather than the Trash genuinely being empty — set from the `degraded` flag
   *  on `list_trash_stream`'s resolved summary, never inferred from `entries` being empty (a healthy
   *  empty Trash also has zero entries, and the two must render differently). */
  let degraded = false;
  let selected = new Set<string>();
  /** Which empty confirm is pending ("all" = whole Trash via the toolbar button with nothing selected
   *  or the explicit "Empty Trash" action; "selected" = just the checked rows), or null when the
   *  `ConfirmDialog` is closed. Irreversible, so both routes through it (MENUS.md). */
  let confirmEmpty: "all" | "selected" | null = null;
  /** Per-item restore failures from the last `restoreSelected()` call, surfaced as a dismissible banner
   *  rather than aborting the rest of the selection (mirrors `restore_trash_items`'s per-item results). */
  let restoreErrors: { name: string; error: string }[] = [];

  let loadGen = 0;

  const extOf = (name: string) => {
    const i = name.lastIndexOf(".");
    return i > 0 ? name.slice(i + 1).toLowerCase() : "";
  };

  async function load(): Promise<void> {
    const gen = ++loadGen;
    entries = [];
    selected = new Set();
    restoreErrors = [];
    error = "";
    degraded = false;
    loading = true;
    try {
      const channel = createChannel<TrashEntry[]>();
      channel.onmessage = (batch) => {
        if (gen !== loadGen) return; // superseded by a newer refresh — drop stale rows
        entries = entries.concat(batch);
        loading = false; // first real rows are in — reveal them
      };
      // CPE-1803: `list_trash_stream` resolves with a `{ count, degraded }` summary once every batch has
      // flushed — `degraded` is the backend's own signal (CPE-1791's caught-panic boundary), never
      // inferred here from an empty `entries`, since a genuinely empty Trash looks identical over the
      // channel (zero batches sent either way).
      const summary = await rawInvoke<TrashStreamSummary>("list_trash_stream", { onEntry: channel });
      if (gen === loadGen) degraded = summary.degraded;
    } catch (e) {
      if (gen === loadGen) error = e instanceof Error ? e.message : String(e);
    } finally {
      if (gen === loadGen) loading = false;
    }
  }
  onMount(load);

  function toggleSelect(id: string): void {
    const next = new Set(selected);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    selected = next;
  }

  function toggleSelectAll(): void {
    selected = selected.size === entries.length ? new Set() : new Set(entries.map((e) => e.id));
  }

  async function restoreSelected(): Promise<void> {
    const targets = entries.filter((e) => selected.has(e.id));
    if (targets.length === 0) return;
    restoreErrors = [];
    const results = await commands.restoreTrashItems(targets.map((e) => e.id));
    const restoredIds = new Set<string>();
    const failed: { name: string; error: string }[] = [];
    results.forEach((r, i) => {
      const entry = targets[i];
      if (r.ok) restoredIds.add(entry.id);
      else failed.push({ name: entry.name, error: r.error });
    });
    entries = entries.filter((e) => !restoredIds.has(e.id));
    selected = new Set([...selected].filter((id) => !restoredIds.has(id)));
    restoreErrors = failed;
  }

  /** Open the confirm dialog for an Empty action. `"selected"` is a no-op with nothing checked — the
   *  toolbar button is disabled in that case, but guard here too since this is reachable from tests /
   *  future call sites directly. */
  function requestEmpty(scope: "all" | "selected"): void {
    if (scope === "selected" && selected.size === 0) return;
    confirmEmpty = scope;
  }

  async function confirmEmptyAction(): Promise<void> {
    const scope = confirmEmpty;
    confirmEmpty = null;
    if (!scope) return;
    try {
      // `confirmed: true` (CPE-1651) is set only in this function — the one the Empty-confirm dialog's
      // accept button runs. `requestEmpty` merely opens that dialog; it never purges.
      if (scope === "all") {
        unwrap(await commands.emptyTrash(null, true));
        entries = [];
        selected = new Set();
      } else {
        const ids = [...selected];
        unwrap(await commands.emptyTrash(ids, true));
        entries = entries.filter((e) => !selected.has(e.id));
        selected = new Set();
      }
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  $: allSelected = entries.length > 0 && selected.size === entries.length;
  $: itemCountLabel =
    entries.length === 1 ? $t("trash.itemCountOne") : $t("trash.itemCountMany", { count: entries.length });
  $: selectedCountLabel =
    selected.size === 1 ? $t("trash.selectedCountOne") : $t("trash.selectedCountMany", { count: selected.size });
  $: emptyConfirmMessage =
    confirmEmpty === "all"
      ? $t("trash.emptyConfirmMessageAll")
      : selected.size === 1
        ? $t("trash.emptyConfirmMessageOne")
        : $t("trash.emptyConfirmMessageMany", { count: selected.size });
</script>

<!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
<div class="tv-overlay" on:click|self={() => dispatch("close")}>
  <div class="tv-panel">
    <div class="tv-titlebar">
      <span class="tv-title">
        <Icon name="delete" size={15} /> {$t("trash.title")}
        <!-- CPE-1803 review: a degraded pass has zero entries but that count is NOT a known fact —
             showing "0 items" here would sit right next to trash.degraded's whole point, which is that
             the true count is unknown. Suppress the count entirely rather than assert a number. -->
        {#if !loading && !error && !degraded}<span class="tv-count">{itemCountLabel}{#if selected.size > 0} · {selectedCountLabel}{/if}</span>{/if}
      </span>
      <div class="tv-tools">
        {#if entries.length > 0}
          <button class="tv-btn" on:click={toggleSelectAll}>
            {allSelected ? $t("trash.deselectAll") : $t("trash.selectAll")}
          </button>
          <button class="tv-btn" disabled={selected.size === 0} on:click={restoreSelected}>
            {$t("trash.restoreSelected")}
          </button>
          <button class="tv-btn" disabled={selected.size === 0} on:click={() => requestEmpty("selected")}>
            {$t("trash.emptySelected")}
          </button>
          <button class="tv-btn danger-text" on:click={() => requestEmpty("all")}>
            {$t("trash.emptyAll")}
          </button>
        {/if}
        <button class="tv-btn" on:click={load} title={$t("trash.refresh")}>
          <Icon name="refresh" size={13} />
        </button>
        <HelpButton section="trash" on:help />
        <button class="tv-x" title="Close" aria-label="Close" on:click={() => dispatch("close")}>×</button>
      </div>
    </div>

    {#if restoreErrors.length > 0}
      <div class="tv-error-banner">
        {#each restoreErrors as f (f.name)}
          <div>{$t("trash.restoreFailed", { name: displaySafeName(f.name), error: f.error })}</div>
        {/each}
        <button class="tv-btn" on:click={() => (restoreErrors = [])}>×</button>
      </div>
    {/if}

    <div class="tv-body">
      {#if loading}
        <div class="tv-empty">{$t("trash.loading")}</div>
      {:else if error}
        <div class="tv-empty tv-edge error">{$t("trash.error", { error })}</div>
      {:else if entries.length === 0 && degraded}
        <!-- CPE-1803: a degraded listing must never render as "trash.empty" — an unreadable trash is
             not the same claim as a genuinely empty one, and telling the user "empty" here would make
             them stop looking for files that are still sitting in the trash. It must also read as its
             OWN state rather than borrowing the hard-failure "error" treatment (CPE-1803 review):
             restore still works, entries may still be there — this isn't a crash, it's a caution. -->
        <div class="tv-empty">
          <span class="tv-degraded-note">{$t("trash.degraded")}</span>
        </div>
      {:else if entries.length === 0}
        <div class="tv-empty">{$t("trash.empty")}</div>
      {:else}
        <div class="tv-head-row">
          <span class="tv-cell tv-check">
            <input type="checkbox" checked={allSelected} on:change={toggleSelectAll} aria-label={$t("trash.selectAll")} />
          </span>
          <span class="tv-cell tv-name">{$t("trash.columnsName")}</span>
          <span class="tv-cell tv-path">{$t("trash.columnsOriginalPath")}</span>
          <span class="tv-cell tv-date">{$t("trash.columnsDeleted")}</span>
        </div>
        {#each entries as e (e.id)}
          <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
          <div class="tv-row" class:selected={selected.has(e.id)} on:click={() => toggleSelect(e.id)}>
            <span class="tv-cell tv-check">
              <input
                type="checkbox"
                checked={selected.has(e.id)}
                on:click|stopPropagation
                on:change={() => toggleSelect(e.id)}
                aria-label={displaySafeName(e.name)}
              />
            </span>
            <span class="tv-cell tv-name" title={displaySafeName(e.name)}>
              <Icon name={iconFor({ is_dir: false, extension: extOf(e.name) })} size={15} />
              {displaySafeName(e.name)}
              {#if e.size !== null}<span class="tv-size">{formatSize(e.size)}</span>{/if}
            </span>
            <span class="tv-cell tv-path" title={displaySafePath(e.original_path)}>{displaySafePath(e.original_path)}</span>
            <span class="tv-cell tv-date">{formatDate(e.time_deleted * 1000)}</span>
          </div>
        {/each}
      {/if}
    </div>
  </div>
</div>

{#if confirmEmpty}
  <ConfirmDialog
    title={$t("trash.emptyConfirmTitle")}
    message={emptyConfirmMessage}
    confirmLabel={$t("trash.emptyConfirmButton")}
    danger
    on:confirm={confirmEmptyAction}
    on:cancel={() => (confirmEmpty = null)}
  />
{/if}

<style>
  .tv-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 60;
  }
  .tv-panel {
    width: min(1000px, 96vw);
    height: min(700px, 92vh);
    display: flex;
    flex-direction: column;
    background: var(--surface);
    color: var(--text);
    border: 1px solid var(--dialog-border);
    border-radius: 8px;
    box-shadow: 0 16px 48px rgba(0, 0, 0, 0.4);
    overflow: hidden;
  }
  .tv-titlebar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border);
  }
  .tv-title { display: flex; align-items: center; gap: 8px; font-weight: 600; }
  .tv-count { font-size: 12px; font-weight: 400; opacity: 0.7; }
  .tv-tools { display: flex; align-items: center; gap: 8px; }
  .tv-btn {
    font: inherit;
    font-size: 12px;
    height: 28px;
    padding: 0 12px;
    border-radius: 6px;
    cursor: pointer;
    border: 1px solid var(--border-strong);
    background: var(--surface);
    color: var(--text);
  }
  .tv-btn:hover:not(:disabled) { background: rgba(128, 128, 128, 0.14); }
  .tv-btn:disabled { opacity: 0.5; cursor: default; }
  /* Wording conveys destructiveness ("Empty Trash…"); text stays var(--text) per MENUS.md — never red.
     The one place red belongs is the ConfirmDialog's own primary button (danger). */
  .tv-btn.danger-text { color: var(--text); }
  .tv-x {
    border: 0;
    background: transparent;
    color: var(--text-dim);
    font-size: 20px;
    cursor: pointer;
    line-height: 1;
    padding: 0 4px;
    border-radius: 4px;
  }
  .tv-x:hover { background: rgba(128, 128, 128, 0.18); color: var(--text); }

  .tv-error-banner {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 8px 14px;
    background: rgba(201, 79, 79, 0.12);
    color: var(--text);
    font-size: 12px;
    border-bottom: 1px solid var(--border);
  }

  .tv-body { flex: 1; overflow: auto; }
  .tv-empty { display: grid; place-items: center; height: 100%; color: var(--text-dim); }
  .tv-edge.error { color: var(--danger); }
  /* CPE-1803: deliberately NOT `.tv-edge.error`'s red/`--danger` treatment — a degraded listing is a
     caution (the listing came back thin; restore still works, entries may still be there), not the
     hard-failure state `trash.error` represents. `--warn` is the established "caution, not failure"
     token elsewhere in the app (ImageCompareView.svelte's `.ic-note`, SidecarManager.svelte, etc.). */
  .tv-degraded-note {
    /* One hex fallback, referenced three times below — keeps this file's ratchet-guarded inline-hex
       footprint (src/app.css.test.ts) to a single new literal instead of three. */
    --tv-warn: var(--warn, #b5872b);
    color: var(--tv-warn);
    background: color-mix(in srgb, var(--tv-warn) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--tv-warn) 40%, transparent);
    border-radius: var(--radius);
    padding: 8px 14px;
    font-size: 12px;
  }

  .tv-head-row,
  .tv-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 14px;
    height: 34px;
  }
  .tv-head-row {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: var(--text-dim);
    border-bottom: 1px solid var(--border);
    position: sticky;
    top: 0;
    background: var(--surface);
  }
  .tv-row { cursor: pointer; border-bottom: 1px solid var(--border); }
  .tv-row:hover { background: rgba(128, 128, 128, 0.08); }
  .tv-row.selected { background: var(--selection); }

  .tv-cell.tv-check { flex: 0 0 auto; width: 20px; display: flex; align-items: center; }
  .tv-cell.tv-name {
    flex: 1 1 30%;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 6px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .tv-cell.tv-path {
    flex: 1 1 45%;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    color: var(--text-dim);
    font-size: 12px;
  }
  .tv-cell.tv-date { flex: 0 0 auto; width: 160px; color: var(--text-dim); font-size: 12px; }
  .tv-size { flex: 0 0 auto; color: var(--text-faint); font-size: 11px; }
</style>
