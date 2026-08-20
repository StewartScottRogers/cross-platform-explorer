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
  /** CPE-1803: true when the last completed listing pass couldn't fully read the OS trash rather than
   *  the Trash genuinely being empty — set from the `degraded` flag on `list_trash_stream`'s resolved
   *  summary, never inferred from `entries` being empty (a healthy empty Trash also has zero entries,
   *  and the two must render differently).
   *
   *  CPE-1804/CPE-1805: this no longer implies `entries` is empty. It covers both the caught-panic route
   *  (which does wipe the pass) and per-item non-UTF-8 skips (which don't), so it must be read strictly
   *  as "what follows is not the whole truth", never as "there is nothing to show". */
  let degraded = false;
  /** CPE-1804: how many items the last pass dropped because a field wasn't valid UTF-8 — the backend's
   *  own count, straight off the summary. `0` with `degraded` true means the pass failed wholesale and
   *  has no per-item count to give, which is why the message below picks its wording from this rather
   *  than always claiming a number. */
  let skipped = 0;
  /** CPE-1816: true once the current pass has fully resolved (summary received, or the invoke threw) —
   *  false for the whole window between the first batch landing (which flips `loading` off, per
   *  STREAMING.md) and that resolution. `degraded`/`skipped` ride on the summary, which by construction
   *  arrives last, so during that window the app genuinely does not yet know whether this pass will turn
   *  out degraded.
   *
   *  MUST be reset to `false` at the top of every `load()` call (CPE-1816 review round 2): without that
   *  reset, a Refresh keeps the PREVIOUS pass's resolved `true` while the new stream is still in flight,
   *  which reintroduces this ticket's exact bug on the second load — see the reset test.
   *
   *  Gates the title-bar count slot (in place of a number, mid-stream, since the real total isn't final)
   *  and, together with `degraded`, the empty-pane note when `entries` drains to zero mid-stream (CPE-1816
   *  review round 2: reachable via Restore/Empty draining the last visible row before the summary
   *  resolves, not just via an untouched empty Trash) — so neither a growing count nor "Trash is empty"
   *  can assert something this pass doesn't yet know. Does NOT gate the degraded-with-rows banner above
   *  the list, which stays keyed on `degraded` alone (CPE-1816 review round 2 / Visual Critic: moving the
   *  mid-stream caveat into the title bar instead of that banner avoids a ~55px row-jump on every clean
   *  load and a one-frame flash on a fast trash — see `noticeMessage` below). */
  let complete = false;
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
    skipped = 0;
    complete = false;
    loading = true;
    try {
      const channel = createChannel<TrashEntry[]>();
      channel.onmessage = (batch) => {
        if (gen !== loadGen) return; // superseded by a newer refresh — drop stale rows
        entries = entries.concat(batch);
        loading = false; // first real rows are in — reveal them
      };
      // CPE-1803: `list_trash_stream` resolves with a `{ count, degraded, skipped }` summary once every
      // batch has flushed — `degraded` is the backend's own signal, never inferred here from an empty
      // `entries`, since a genuinely empty Trash looks identical over the channel (zero batches sent
      // either way). CPE-1804: `skipped` counts the items the backend dropped for an undecodable field;
      // that route DOES send batches, so nothing about the arriving rows reveals it either.
      const summary = await rawInvoke<TrashStreamSummary>("list_trash_stream", { onEntry: channel });
      if (gen === loadGen) {
        degraded = summary.degraded;
        skipped = summary.skipped;
        complete = true; // CPE-1816: only now is the verdict in — safe to assert a count or its absence
      }
    } catch (e) {
      if (gen === loadGen) {
        error = e instanceof Error ? e.message : String(e);
        // Defensive only, not currently test-observable: `error` already gates every rendered branch
        // that would otherwise consult `complete` (the title-bar slot and the empty-pane note both check
        // `!error` first), so nothing visibly changes if this line is deleted. Kept because a future
        // change to those conditions should not silently reintroduce a stuck-incomplete state on a failed
        // pass -- correctness-by-construction rather than a covered path. Reviewer review (round 2).
        complete = true;
      }
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
  /** CPE-1804: one message for the one `degraded` state, worded from what the pass actually knows.
   *  A per-item skip has a real count, and "3 items couldn't be shown" tells the user how much is
   *  missing — enough to decide whether to go looking at the OS level — where an unqualified warning
   *  only tells them that *something* is (the CPE-1704 counting-contract precedent). A caught panic has
   *  no count to give (it lost the whole pass, not n items), so it keeps CPE-1803's wording, which was
   *  written for exactly that: an unknown quantity of unseen entries. */
  $: degradedMessage =
    skipped > 0
      ? skipped === 1
        ? $t("trash.skippedOne")
        : $t("trash.skippedMany", { count: skipped })
      : $t("trash.degraded");
  /** CPE-1816: the one caveat shared by two very different unfinished-listing states — `degraded`
   *  (resolved, known incomplete) and mid-stream (`!complete`, not yet resolved, not yet known either
   *  way). `degraded` is authoritative once it's known (`complete` is true by the time it's ever true),
   *  so it's checked first; otherwise the honest answer is "not sure yet", not "no problem so far".
   *
   *  Used in the title-bar count slot (mid-stream only — see the markup) and in the empty-pane note when
   *  `entries` is empty AND either state applies. Deliberately NOT used for the rows-above-the-list
   *  banner when rows are present and mid-stream: CPE-1816 review round 2 (Visual Critic) measured that
   *  putting a *second* caveat location there caused a ~55px layout jump on every ordinary clean load (the
   *  banner appearing then vanishing) and could flash for a single frame on a fast trash where the whole
   *  pass resolves in one batch. The title bar's count slot is already empty in this state and toggling
   *  its text moves nothing, so that's where the mid-stream caveat lives instead; the rows banner stays
   *  keyed on `degraded` alone, exactly as CPE-1805 left it. */
  $: noticeMessage = degraded ? degradedMessage : $t("trash.stillLoading");
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
        <!-- CPE-1803 review: a degraded pass's entry count is NOT a known fact — showing "0 items" here
             would sit right next to trash.degraded's whole point, which is that the true count is
             unknown. Suppress the count entirely rather than assert a number.
             CPE-1804 keeps that rule unchanged now that a degraded pass can also have rows: "5 items"
             would still be a claim about the Trash's contents that this pass can't back, since the
             skipped items are in the Trash and not in that number. The count the app DOES know — how
             many it dropped — is in the notice instead (the empty-pane note / rows banner below).
             CPE-1816: `degraded`/`skipped` themselves don't exist yet until the stream's summary
             resolves, so the same suppression now also applies for the whole window between the first
             batch landing and that resolution — `complete` is what tracks that. Rather than leaving this
             slot blank in that window (or duplicating a caveat into a rows banner that would then jump
             the list around, per the round-2 Visual Critic review), it shows `trash.stillLoading` here —
             the slot is already reserved and already empty in exactly this state, so swapping its text
             costs no layout.
             `role="status"` (round-2 a11y finding): this slot is the ONLY place the final item count is
             ever announced, so it doubles as the live region for both halves of the problem this ticket
             exists to fix — a screen reader hears "Still loading…" while the pass is in flight, then
             hears the real count (or nothing, if degraded or the pane has drained to zero mid-stream,
             since the empty-pane note / rows banner carries that message instead and would otherwise
             double up with this slot — see the `entries.length > 0` guard below) once it resolves. Kept
             as one persistent node — not toggled in and out of the DOM — since a `role="status"` region
             only reliably announces content CHANGES within an already-mounted node.
             CPE-1816 review round 2 (nit): the selection count rides alongside `trash.stillLoading` too,
             same as it already does alongside the resolved item count — the app genuinely knows how many
             rows are checked regardless of whether the pass itself has finished, so there's no reason for
             a mid-stream selection to go unacknowledged here just because the total is still unknown. -->
        <span class="tv-count" role="status">
          {#if !loading && !error}
            {#if !degraded && complete}
              {itemCountLabel}{#if selected.size > 0} · {selectedCountLabel}{/if}
            {:else if !complete && entries.length > 0}
              <span class="tv-count-loading">{$t("trash.stillLoading")}</span>{#if selected.size > 0} · {selectedCountLabel}{/if}
            {:else if selected.size > 0}
              {selectedCountLabel}
            {/if}
          {/if}
        </span>
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
      {:else if (degraded || !complete) && entries.length === 0}
        <!-- CPE-1803: a degraded listing must never render as "trash.empty" — an unreadable trash is
             not the same claim as a genuinely empty one, and telling the user "empty" here would make
             them stop looking for files that are still sitting in the trash. It must also read as its
             OWN state rather than borrowing the hard-failure "error" treatment (CPE-1803 review):
             restore still works, entries may still be there — this isn't a crash, it's a caution.

             CPE-1816 review round 2: `!complete` joins `degraded` here, not just `degraded` alone. This
             branch is reachable with `!complete` — Restore or Empty can drain the last VISIBLE row to
             zero while the stream is still in flight (the summary hasn't resolved, so more rows could
             still be coming), and before this fix that drained state fell through to the plain "Trash is
             empty" branch below. Safe for a genuinely empty Trash: `complete` and `loading` are set in
             the same `finally` tick as each other, so the healthy empty-Trash case never observes
             `!complete` here — `loading` is still true and the branch above already renders first. -->
        <div class="tv-empty">
          <span class="tv-degraded-note">{noticeMessage}</span>
        </div>
      {:else if entries.length === 0}
        <div class="tv-empty">{$t("trash.empty")}</div>
      {:else}
        <!-- CPE-1816 review round 2 (finding 3): the degraded banner and the head row below are both
             meant to stay pinned to the top of the scrolling `.tv-body`. Giving them EACH their own
             independent `position: sticky; top: 0` made them occupy the same sticky slot once the list
             scrolled — the taller one (the banner) fully covered the header, including its Select-all
             checkbox, and no `z-index` fixes that since they're competing for one slot rather than
             stacked one above the other. Wrapping both in a single `.tv-sticky-stack` container makes
             them lay out in normal flow INSIDE it, and only the container itself is sticky, so the pair
             sticks as one unit and never overlaps — with no hard-coded pixel offset that would break the
             moment either message's text reflows to a different height. -->
        <div class="tv-sticky-stack">
          {#if degraded}
            <!-- CPE-1805: degraded-with-entries. Before CPE-1804 this branch was unreachable — the only
                 degradation route (a caught `list()` panic) wiped the pass to zero entries, so the
                 empty-only special case above was correct purely by accident, resting on an unstated
                 backend invariant. CPE-1804 makes per-item skips a second route, and that route leaves
                 the surviving entries in place: a partial list is now the ORDINARY shape of an
                 incomplete listing. Rendering it as a plain list would ship the same lie in a new place
                 — the user sees rows, assumes that's everything, and stops looking. The notice is
                 therefore driven by `degraded` ALONE; `entries.length` only chooses where it sits, never
                 whether it appears.

                 CPE-1816 review round 2 (Visual Critic): deliberately NOT also keyed on `!complete` any
                 more. An earlier version of this fix showed this same banner for the mid-stream case
                 too, and measurement caught two real problems: on the ordinary clean path the banner's
                 appear-then-vanish caused a ~55px jump in every row's position the instant the stream
                 resolved (rows are click targets — the row under the pointer changes mid-reach), and on
                 a fast local trash (one batch, near-instant summary) the banner's entire lifetime could
                 be a single rendered frame — a flash worse than no notice. The mid-stream caveat now
                 lives in the title bar's item-count slot instead (see the markup above), which is
                 already empty in that state, so swapping its text moves nothing below it. This banner is
                 reserved for `degraded`, a state that's rarer, already resolved by the time it shows,
                 and worth the (smaller, one-time) layout cost of surfacing prominently. -->
            <div class="tv-degraded-banner">
              <span class="tv-degraded-note">{degradedMessage}</span>
            </div>
          {/if}
          <div class="tv-head-row">
            <span class="tv-cell tv-check">
              <input type="checkbox" checked={allSelected} on:change={toggleSelectAll} aria-label={$t("trash.selectAll")} />
            </span>
            <span class="tv-cell tv-name">{$t("trash.columnsName")}</span>
            <span class="tv-cell tv-path">{$t("trash.columnsOriginalPath")}</span>
            <span class="tv-cell tv-date">{$t("trash.columnsDeleted")}</span>
          </div>
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
  /* CPE-1816 review round 2 (finding 5): a bare flex child grows to its content width and can force the
     row to wrap / the toolbar to jump at a narrow window width. `min-width: 0` lets `.tv-title` shrink
     below that content width inside `.tv-titlebar`'s flex row instead of pushing `.tv-tools`; the
     overflow/nowrap/ellipsis trio keeps a long title on one line, truncated, rather than wrapping the
     titlebar onto two. */
  .tv-title {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 600;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .tv-count { font-size: 12px; font-weight: 400; opacity: 0.7; }
  /* CPE-1816: the mid-stream caveat, sharing the title bar's item-count slot rather than a separate
     banner (round-2 Visual Critic decision — see `noticeMessage`'s doc comment). Italic distinguishes
     "provisional status text" from the plain count/selection text this same slot shows once resolved,
     without a second colour (the slot is already dim via `.tv-count`'s opacity). */
  .tv-count-loading { font-style: italic; }
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
  /* CPE-1803 review: deliberately NOT `.tv-edge.error`'s red/`--danger` treatment — a degraded listing
     is a caution (the listing came back thin; restore still works, entries may still be there), not the
     hard-failure state `trash.error` represents. Also deliberately NOT `var(--warn, <hex>)` — `--warn`
     is never defined as a real token anywhere in `src/`, so that fallback always resolves to the literal
     hex (AgentTimeline.svelte's `.hd-unclean-note` comment calls this out by name as an "older ... fallback
     idiom" to avoid) — a fixed hex would render identically, and least legibly, in the dark theme, which
     is exactly what the WCAG contrast guard exists to catch. Uses the same real, always-defined semantic
     tokens `.hd-unclean-note` uses instead (`--border-strong`/`--surface-alt` + `--text`), so distinctness
     from "empty" (plain dim text, no box) and "error" (red text, no box) comes from the bordered/filled
     box, not from hue — no ratchet growth, no fixed-hex contrast risk. */
  .tv-degraded-note {
    border: 1px solid var(--border-strong, var(--border));
    background: var(--surface-alt, transparent);
    color: var(--text);
    border-radius: var(--radius);
    padding: 8px 14px;
    font-size: 12px;
  }
  /* CPE-1805: the same note, sitting ABOVE a partial list instead of centred in an empty pane. Only the
     placement differs — the note keeps its own `.tv-degraded-note` box so "incomplete" reads identically
     whether or not any rows survived. Visibility while scrolling is now handled by the `.tv-sticky-stack`
     wrapper (below) rather than by this element being sticky itself — see that rule's comment. */
  .tv-degraded-banner {
    display: flex;
    justify-content: center;
    padding: 10px 14px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }
  /* CPE-1816 review round 2 (finding 3): `.tv-degraded-banner` and `.tv-head-row` previously each had
     their own independent `position: sticky; top: 0`, which put them in the SAME sticky slot once the
     list scrolled — the banner, being taller, fully covered the header (including its Select-all
     checkbox) underneath it, and no `z-index` ordering fixes two elements competing for one slot. Both
     are now plain, non-sticky children of THIS wrapper, which is the only sticky element; they stack in
     normal document flow inside it (banner above head row, when the banner is present at all), so the
     wrapper's total height already accounts for both and nothing overlaps. No hard-coded pixel offset —
     it survives either message's text reflowing to a different height, or the banner being absent
     entirely (a plain listing's head row is the wrapper's only child, at the same `top: 0`). */
  .tv-sticky-stack {
    position: sticky;
    top: 0;
    z-index: 1;
    background: var(--surface);
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
