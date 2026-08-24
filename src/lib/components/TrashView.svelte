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
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import { t } from "../i18n";
  import { formatDate } from "../datetime";
  import { formatSize } from "../format";
  import { iconFor } from "../filetypes";
  import { displaySafeName, displaySafePath } from "../filename";
  import type { Section } from "../sectionDocs";

  // CPE-1827: "help" is dispatched directly from the overflow menu's Docs row below (the same `Section`
  // payload `HelpButton` would have dispatched) — the titlebar overflow menu replaced the standalone
  // `HelpButton` chip, which didn't fit the `.row`/`MENUS.md` menu-item shape, so this component now owns
  // the dispatch itself instead of forwarding `HelpButton`'s own event verbatim.
  const dispatch = createEventDispatcher<{ close: void; help: Section }>();

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

  /** CPE-1827: the titlebar's "…" overflow menu (docs/design/MENUS.md dropdown pattern) — holds every
   *  toolbar action except Close, so the titlebar itself never needs more than two fixed-size controls
   *  (this trigger + the × ) regardless of listing state or locale. See the titlebar markup + its CSS
   *  comment for the full rationale. */
  let overflowOpen = false;
  /** The overflow trigger button, bound so `clampToAnchor` (below) can read its on-screen position when
   *  the menu opens. */
  let overflowBtnEl: HTMLButtonElement;

  /** CPE-1827: positions the overflow menu at `position: fixed` (MENUS.md's container spec), anchored
   *  to the trigger button's own on-screen rect and clamped fully into the viewport — the same
   *  clamp-on-open shape `AgentMenu.svelte`/`ContextMenu.svelte` use for their `.ctx` menus, adapted
   *  from "clamp around a cursor point" to "clamp around a trigger element". `position: fixed` is
   *  deliberate, not just MENUS.md-conformant: `.tv-panel` is `overflow: hidden`, and this ticket exists
   *  BECAUSE that rule silently clips anything that overflows it — a `position: absolute` menu (the
   *  `.menu-wrap`/`.menu` shape CommandBar/MenuBar use for their left-edge-anchored dropdowns) stays a
   *  normal descendant for clipping purposes and would still be cut off by `.tv-panel` at the 600px
   *  floor with a tall menu (Select all/Restore/Empty selected/Empty Trash/Refresh/Docs can add up to
   *  ~210px, taller than the panel's own ~368px height at the app's 600×400 minimum window). `position:
   *  fixed` escapes that clip (no transformed/filtered ancestor between here and the viewport establishes
   *  a new containing block for it), so clamping against `window.innerWidth/innerHeight` here is the
   *  real, sufficient guarantee — not `.tv-panel`'s bounds. Right-anchored (menu's right edge lines up
   *  with the trigger's right edge, growing leftward) because the trigger sits near the panel's own right
   *  edge, just left of Close; anchoring left, like CommandBar's dropdowns do, would grow the menu
   *  rightward off the panel at any width this ticket cares about. */
  function clampToAnchor(node: HTMLElement, anchor: HTMLElement) {
    const place = () => {
      const a = anchor.getBoundingClientRect();
      const pad = 6;
      let left = a.right - node.offsetWidth;
      let top = a.bottom + 4;
      left = Math.max(pad, Math.min(left, window.innerWidth - node.offsetWidth - pad));
      top = Math.max(pad, Math.min(top, window.innerHeight - node.offsetHeight - pad));
      node.style.left = `${left}px`;
      node.style.top = `${top}px`;
    };
    place();
    return { destroy() {} };
  }

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

<!-- CPE-1827: no Escape handler existed before this ticket — once the toolbar's overflow-heavy layout
     clipped the × below ~684px, the ONLY way out was the ~2vw backdrop sliver `.tv-overlay`'s
     `on:click|self` catches (a modal trap). Matches the other full-panel overlays' own convention
     (DiskSpaceView.svelte, ArchiveSafetyDialog.svelte, RepairLinkDialog.svelte: a bare
     `<svelte:window on:keydown={(e) => e.key === "Escape" && dispatch("close")} />`), with two additions
     of this component's own:
       - the overflow menu (below) takes Escape FIRST when open — matching MENUS.md's "Escape closes [the
         menu]" convention for every other dropdown in the app (CommandBar/MenuBar) — rather than closing
         the whole view out from under an open menu.
       - `confirmEmpty` (the nested `ConfirmDialog`, rendered as a sibling below) owns Escape while IT is
         open: that dialog already has its own `<svelte:window on:keydown>` (ConfirmDialog.svelte) that
         dispatches `cancel` — without this guard, the same keypress would ALSO bubble into this handler
         and close the entire Trash view out from under the confirm, which is a bigger, more surprising
         jump than the "quiet no-op, dialog handles its own Escape" every other host of a nested
         ConfirmDialog in this app relies on.
     Click-outside-closes-the-menu (MENUS.md) reuses the same `overflowOpen` state via the window click
     listener; the trigger button and the menu itself both `stopPropagation` so opening/clicking inside
     never immediately re-closes it (same shape as CommandBar's `.menu`). -->
<svelte:window
  on:click={() => (overflowOpen = false)}
  on:keydown={(e) => {
    if (e.key !== "Escape") return;
    if (overflowOpen) {
      overflowOpen = false;
      return;
    }
    if (confirmEmpty) return; // ConfirmDialog is open — it owns this Escape via its own listener
    dispatch("close");
  }}
/>

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
             a mid-stream selection to go unacknowledged here just because the total is still unknown.
             CPE-1816 review round 3 (nit): `&nbsp;·` instead of a plain leading space before the
             separator — Svelte trims leading whitespace immediately inside an `{#if}` block's static
             text, so the naive `{#if selected.size > 0} · {selectedCountLabel}{/if}` silently lost its
             leading space and read as "3 items· 1 selected" (audible to a screen reader, not just a
             visual nit — pre-existing on the resolved-count branch, now also fixed here rather than
             duplicated). `&nbsp;` is a literal entity, not collapsible ASCII whitespace, so it survives. -->
        <span class="tv-count" role="status">
          {#if !loading && !error}
            {#if !degraded && complete}
              {itemCountLabel}{#if selected.size > 0}&nbsp;· {selectedCountLabel}{/if}
            {:else if !complete && entries.length > 0}
              <span class="tv-count-loading">{$t("trash.stillLoading")}</span>{#if selected.size > 0}&nbsp;· {selectedCountLabel}{/if}
            {:else if selected.size > 0}
              {selectedCountLabel}
            {/if}
          {/if}
        </span>
      </span>
      <!-- CPE-1827: the whole action set collapses into ONE overflow menu (the Visual Critic's
           recommendation over the two alternatives — icon-only buttons below a breakpoint, or letting
           the bar wrap — because it's the only option that keeps the × on the first line at EVERY
           supported width, not just above some breakpoint). `.tv-tools` now holds exactly two
           fixed-size controls regardless of listing state, selection, or locale: this trigger and ×. -->
      <div class="tv-tools">
        <button
          class="tv-btn tv-overflow-btn"
          bind:this={overflowBtnEl}
          title={$t("trash.moreActions")}
          aria-label={$t("trash.moreActions")}
          aria-haspopup="menu"
          aria-expanded={overflowOpen}
          on:click|stopPropagation={() => (overflowOpen = !overflowOpen)}
        >
          <Icon name="more" size={15} />
        </button>
        {#if overflowOpen}
          <!-- svelte-ignore a11y-no-noninteractive-element-interactions a11y-click-events-have-key-events -->
          <div
            class="menu tv-overflow-menu"
            role="menu"
            tabindex="-1"
            use:clampToAnchor={overflowBtnEl}
            on:click|stopPropagation
          >
            {#if entries.length > 0}
              <button role="menuitem" on:click={() => { toggleSelectAll(); overflowOpen = false; }}>
                <Icon name="check" size={15} />
                {allSelected ? $t("trash.deselectAll") : $t("trash.selectAll")}
              </button>
              <button
                role="menuitem"
                disabled={selected.size === 0}
                on:click={() => { restoreSelected(); overflowOpen = false; }}
              >
                <Icon name="back" size={15} />
                {$t("trash.restoreSelected")}
              </button>
              <button
                role="menuitem"
                disabled={selected.size === 0}
                on:click={() => { requestEmpty("selected"); overflowOpen = false; }}
              >
                <Icon name="delete" size={15} />
                {$t("trash.emptySelected")}
              </button>
              <!-- Wording conveys destructiveness; text stays var(--text) via the shared `.menu button`
                   rule per MENUS.md — never red (the one place red belongs is ConfirmDialog's own
                   primary button, reached via `requestEmpty` below). -->
              <button role="menuitem" on:click={() => { requestEmpty("all"); overflowOpen = false; }}>
                <Icon name="delete" size={15} />
                {$t("trash.emptyAll")}
              </button>
              <div class="menu-sep" role="separator" />
            {/if}
            <button role="menuitem" on:click={() => { load(); overflowOpen = false; }}>
              <Icon name="refresh" size={15} />
              {$t("trash.refresh")}
            </button>
            <button role="menuitem" on:click={() => { dispatch("help", "trash"); overflowOpen = false; }}>
              <Icon name="book" size={15} />
              {$t("trash.docs")}
            </button>
          </div>
        {/if}
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
             `!complete` here — `loading` is still true and the branch above already renders first.

             CPE-1816 review round 3, BLOCKING 2 (a11y): `role="status"` on this span, not just the
             title-bar slot. The title bar's own "Still loading…" only shows while `entries.length > 0`
             (see its own guard), so when Restore/Empty drains the list to zero mid-stream, THIS is the
             only place the caveat is visible at all — without a live region here, a screen reader that
             heard "Still loading…" from the title bar a moment ago hears nothing when that text moves
             here, and nothing again once it resolves either way. Net effect: best-effort announcement
             instead of guaranteed silence — strictly better than before this fix, never worse.
             CPE-1816 review round 4 (a11y correction, tracked as a follow-up rather than fixed here):
             this node is freshly mounted WITH its text already present, which is the WEAKER of the two
             live-region shapes, not the correct one — a node inserted already containing its content is
             frequently not announced at all (WebView2 + Windows AT is a particularly weak combination
             for it), unlike the title-bar slot's persistent-node shape (content changes IN PLACE inside
             an already-mounted node across states), which IS the reliable pattern and is what CPE-1816's
             actual scope — the mid-stream caveat — correctly uses. An earlier version of this comment
             claimed the opposite (that fresh-mount was "the correct... shape for genuinely NEW
             information"), inverting the accepted guidance and contradicting the reasoning applied
             correctly to the title-bar slot above; that claim was wrong and is corrected here. The
             mechanism (this `role="status"`) is left as-is regardless — still strictly better than no
             live region at all — pending a proper fix in the follow-up ticket. -->
        <div class="tv-empty">
          <span class="tv-degraded-note" role="status">{noticeMessage}</span>
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
                 and worth the (smaller, one-time) layout cost of surfacing prominently.

                 CPE-1816 review round 3, BLOCKING 2 (a11y): `role="status"` here closes the gap the
                 Critic measured — the title-bar slot goes "Still loading…" -> "" the instant this
                 branch's condition becomes true (no `degraded`-aware branch in that slot's `{#if}` chain
                 shows anything once `complete && degraded`, other than a possible selection count), so
                 without a live region on THIS element a screen reader hears the pass finish and then
                 hears nothing — not a count, not "unreadable", nothing. Best-effort announcement instead
                 of guaranteed silence — see the empty-pane note above for the same reasoning, INCLUDING
                 the round-4 correction: this node mounts fresh already containing its text, which is the
                 weaker, not-guaranteed live-region shape (unlike the title-bar slot's reliable
                 persistent-node shape), tracked as a follow-up rather than fixed here. -->
            <div class="tv-degraded-banner">
              <span class="tv-degraded-note" role="status">{degradedMessage}</span>
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
  /* CPE-1827 (supersedes the CPE-1816 round 2/3/4 history that used to sit here — same finding-5
     symptom, now actually fixed rather than shuffled between the title and the toolbar): the real
     cause was density, not a `min-width` value on either side, per the ticket. The toolbar-density
     decision round 4 deferred is made here — an overflow "…" menu holds every action except Close, so
     `.tv-tools` (below) is now a FIXED two-control width (trigger + ×) at every listing state and every
     locale, never the wide, content-driven row that forced this fight in the first place. That frees
     `.tv-title` to take the make-or-break variable-width role safely: `flex: 1 1 auto; min-width: 0`
     lets it shrink below its own content width (the thing round 2's bare `min-width: 0` got right),
     and `flex-wrap: wrap` (replacing the old `white-space: nowrap; overflow: hidden`, which pinned
     `.tv-title`'s own min-content width to its full rendered width and reintroduced the exact silent
     clip this ticket exists to fix) lets its three children — icon, "Trash" text, the `.tv-count` span
     — wrap onto their own line inside `.tv-title` when they don't fit, growing `.tv-titlebar`'s height
     rather than ever being cut off. `.tv-tools` stays `flex: 0 0 auto` so it never participates in the
     shrink/wrap at all — Close must never move. */
  .tv-title {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 4px 8px;
    font-weight: 600;
    flex: 1 1 auto;
    min-width: 0;
  }
  .tv-count { font-size: 12px; font-weight: 400; opacity: 0.7; }
  /* CPE-1816: the mid-stream caveat, sharing the title bar's item-count slot rather than a separate
     banner (round-2 Visual Critic decision — see `noticeMessage`'s doc comment). Italic distinguishes
     "provisional status text" from the plain count/selection text this same slot shows once resolved,
     without a second colour (the slot is already dim via `.tv-count`'s opacity). */
  .tv-count-loading { font-style: italic; }
  /* CPE-1827: fixed-width and non-shrinking — see `.tv-title`'s comment above. Holds exactly the
     overflow-menu trigger and Close, in that order, so Close is always the last, rightmost, and never
     the widest-varying element in the titlebar. */
  .tv-tools { display: flex; align-items: center; gap: 8px; flex: 0 0 auto; }
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
  /* CPE-1827: the overflow trigger is a square icon button (reuses `.tv-btn`'s chrome, just narrower
     padding so the 15px "more" glyph isn't crowded by 12px of horizontal padding meant for text). */
  .tv-overflow-btn { padding: 0 7px; }
  /* CPE-1827: `position: fixed` (not the global `.menu-wrap`/`.menu`'s `position: absolute`) — see
     `clampToAnchor`'s doc comment above for why. Reuses the global `.menu` class for its chrome
     (background/border/radius/shadow/padding, `app.css`) and only overrides positioning; `left`/`top`
     are set per-open by `clampToAnchor`, computed and clamped fully into the viewport, so the static
     `left: 0` / `top: calc(100% + 4px)` the global class ships (meant for a `position: relative`
     `.menu-wrap` ancestor, which this menu deliberately has none of) never applies. `z-index: 100`
     matches MENUS.md's container spec (and `AgentMenu`/`ContextMenu`'s `.ctx`) rather than the global
     `.menu`'s `z-index: 20`, which was tuned for menus still nested inside their trigger's own stacking
     context. */
  .tv-overflow-menu { position: fixed; left: 0; top: 0; z-index: 100; }
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
     hard-failure state `trash.error` represents. Also deliberately NOT `var(--warn, <hex>)` — at the time
     `--warn` was never defined as a real token anywhere in `src/`, so that fallback always resolved to the
     literal hex (AgentTimeline.svelte's `.hd-unclean-note` comment called this out by name as an
     "older ... fallback idiom" to avoid) — a fixed hex would render identically, and least legibly, in the
     dark theme, which is exactly what the WCAG contrast guard exists to catch. CPE-1810 has since given
     `--warn` a real, always-defined value in every theme, but this box-treatment choice stays: it was
     never just a workaround for the missing token, it also keeps "degraded" visually distinct from
     "empty" (plain dim text, no box) and "error" (red text, no box) via the bordered/filled box rather
     than hue, so switching to `--warn` here would be a scope-creeping redesign, not this ticket's fix. */
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
