<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { formatSize, formatDiskFree } from "../format";
  import { displaySafeName } from "../filename";

  /** Begin an OS-window resize from the bottom-right corner when the grip is pressed (CPE-842).
      Guarded so it's a harmless no-op outside Tauri (e.g. the jsdom test harness). */
  async function startCornerResize(e: MouseEvent) {
    if (e.button !== 0) return; // left button only
    e.preventDefault();
    try {
      await getCurrentWindow().startResizeDragging("SouthEast");
    } catch {
      /* not running under Tauri, or the window can't be resized — ignore */
    }
  }

  /** Git sync status of the current folder (CPE-462), or null when it isn't a repo. Shape mirrors
      the host `forge_repo_status` command: { is_repo, branch, upstream, ahead, behind, dirty, ... }. */
  export let git: { is_repo?: boolean; branch?: string; upstream?: string; ahead?: number; behind?: number; dirty?: boolean; conflicted?: boolean } | null = null;
  const dispatch = createEventDispatcher<{ pull: void; push: void; sync: void; resolve: void }>();

  export let itemCount = 0;
  /** The folder's total item count before filtering; when it exceeds itemCount the status
      reads "X of Y items" so the filter's effect is visible (CPE-407). */
  export let totalCount = 0;
  export let selectedCount = 0;
  export let selectedSize = 0;

  $: isFiltered = totalCount > itemCount;
  export let hiddenShown = false;
  /** How many entries the current remote listing left out because their name could not be shown safely
   *  (CPE-1708) — 0 for every ordinary (local, or unfiltered-remote) folder, which is the overwhelming
   *  majority of listings, so this renders NOTHING at all in that case (an always-on note nobody reads
   *  is exactly how a real warning gets tuned out). Deliberately worded to say the LISTING succeeded —
   *  "N entries were hidden" reads as "here's what happened to some entries", not "your folder failed to
   *  load" — since the alternative (silence) is the actual bug this exists to fix: CPE-1704 built a real,
   *  trustworthy, non-spoofable count for exactly this a `RemoteListing::filtered` `usize` computed
   *  in-process from what the provider's own listing pass genuinely had to refuse — but it stopped at the
   *  Tauri boundary as a developer-only `eprintln!`, so a user with a hidden key saw a listing that
   *  looked complete. A synthetic "⚠ N keys hidden" ROW was tried and rejected (PR 890, round 2): it was
   *  spoofable by a real object sharing the marker's name, its `is_dir`/`size` fields were dishonest, and
   *  deleting it "succeeded" without deleting anything. This status-bar note is why: a plain status-bar
   *  line is data ABOUT the listing, never a fake ROW IN it — same convention as `hiddenShown` above,
   *  the closest existing precedent (a folder-scoped fact about what's/isn't currently shown). */
  export let filteredHidden = 0;
  /** How many of the current listing's entries could NOT BE READ (CPE-1780) — a `metadata()`/`readdir`
   *  failure the local walk hit mid-listing, always 0 for a remote listing. DELIBERATELY a separate prop
   *  from `filteredHidden` above and never added to it: `filteredHidden` means "a name could not be
   *  shown safely" (the row was never even seen — a REMOTE keyspace-rule refusal); this means "the row
   *  was seen but the walk could not stat it" (a LOCAL read failure) — different facts needing different
   *  words, so this renders its own note rather than folding into `filteredHidden`'s sentence. 0 for the
   *  overwhelming majority of listings, in which case (same convention as `filteredHidden`) the status
   *  bar renders nothing for it at all. */
  export let unreadableCount = 0;
  // CPE-1798: `notice` is fed raw backend error strings from 35 `showNotice(String(e), true)` call
  // sites in `App.svelte` (plus one hand-built "Sync failed: " + e.message), and a Rust error routinely
  // embeds the offending filesystem path — so escaping on arrival, same leaf-escapes-what-it-renders
  // model CPE-1790 established for ConfirmDialog/PasswordPromptDialog, is required here too, not just
  // for genuinely static UI copy. `displaySafeName` only replaces the twelve bidi/format control
  // characters and is idempotent, so escaping the WHOLE message is a safe no-op on ordinary prose.
  export let notice = "";
  export let noticeIsError = false;
  /** Free / total bytes on the current drive (CPE-403); null ⇒ unknown (Home/archive/error). */
  export let diskFree: number | null = null;
  export let diskTotal: number | null = null;

  $: diskLabel =
    diskFree !== null && diskTotal !== null ? formatDiskFree(diskFree, diskTotal) : "";

  // CPE-1708 (Foreman F2): hoisted so the SAME sentence backs both the visible (possibly
  // ellipsis-truncated, see `.filtered-hidden` below) text and the `title` tooltip — the tooltip
  // additionally appends the "loaded successfully" reassurance, so truncation at a narrow window
  // never hides either the actual count or the reassurance that the listing didn't fail.
  $: filteredHiddenText = filteredHidden === 1
    ? "1 entry was hidden because its name could not be shown safely"
    : `${filteredHidden} entries were hidden because their names could not be shown safely`;
  $: filteredHiddenTitle = `${filteredHiddenText} — the folder itself loaded successfully.`;

  // CPE-1780: the `unreadableCount` twin of the hoisting above — same "one sentence backs both the
  // (possibly truncated) visible text and the title tooltip" reasoning, DIFFERENT wording from
  // `filteredHiddenText` (see `unreadableCount`'s doc for why the two facts must never share a sentence).
  $: unreadableText = unreadableCount === 1
    ? "1 entry could not be read"
    : `${unreadableCount} entries could not be read`;
  $: unreadableTitle = `${unreadableText} — the rest of the folder loaded successfully.`;
</script>

<div class="statusbar">
  <span>
    {#if isFiltered}
      {itemCount} of {totalCount} items
    {:else}
      {itemCount} item{itemCount === 1 ? "" : "s"}
    {/if}
  </span>

  {#if selectedCount > 0}
    <span>
      {selectedCount} selected{selectedSize > 0 ? ` — ${formatSize(selectedSize)}` : ""}
    </span>
  {/if}

  {#if hiddenShown}
    <span class="dim">Hidden files shown</span>
  {/if}

  {#if filteredHidden > 0}
    <!-- CPE-1708: only ever a status-bar NOTE about the listing, never a synthetic ROW in it (see
         `filteredHidden`'s doc above for why that distinction is the whole point). `title` carries
         the SAME sentence (see `filteredHiddenTitle` above) plus the reassurance, so a narrow window
         truncating the visible text (see `.filtered-hidden` below) never loses either. -->
    <span class="filtered-hidden" title={filteredHiddenTitle}>
      {filteredHiddenText}
    </span>
  {/if}

  {#if unreadableCount > 0}
    <!-- CPE-1780: a separate NOTE from `.filtered-hidden` above, worded for a different fact (a read
         failure, not a name refused) — see `unreadableCount`'s doc. `--warn` (not `--accent`) because this
         one IS a real read failure, not just an intentional/successful name filter. -->
    <span class="unreadable" title={unreadableTitle}>
      {unreadableText}
    </span>
  {/if}

  {#if notice}
    <span class="notice" class:error={noticeIsError} title={displaySafeName(notice)}>{displaySafeName(notice)}</span>
  {/if}

  {#if git && git.is_repo}
    <span class="git" title={git.upstream ? `Tracking ${git.upstream}` : "No upstream branch"}>
      <span class="git-branch">⎇ {git.branch || "detached"}</span>
      {#if git.behind}<span class="git-ct" title="{git.behind} behind">↓{git.behind}</span>{/if}
      {#if git.ahead}<span class="git-ct" title="{git.ahead} ahead">↑{git.ahead}</span>{/if}
      {#if git.dirty}<span class="git-dirty" title="Uncommitted changes">●</span>{/if}
      {#if git.conflicted}
        <span class="git-conflict" title="Unmerged files from a merge/rebase">conflicts</span>
        <button class="git-btn resolve" on:click={() => dispatch("resolve")} title="Resolve merge/rebase conflicts in-app">Resolve…</button>
      {:else}
        {#if git.behind}<button class="git-btn" on:click={() => dispatch("pull")} title="Fast-forward pull from the remote">Pull</button>{/if}
        {#if git.ahead}<button class="git-btn" on:click={() => dispatch("push")} title="Push local commits to the remote">Push</button>{/if}
        <button class="git-btn" on:click={() => dispatch("sync")} title="Two-way sync: preview the plan, set the on-diverge policy, then run">Sync…</button>
      {/if}
    </span>
  {/if}

  {#if diskLabel}
    <span class="dim disk" title="Free space on this drive">{diskLabel}</span>
  {/if}

  <!-- Bottom-right sizing grip: drag to resize the window (CPE-842). A pure mouse-drag affordance
       (the OS window edges remain keyboard/other-input resizable), so the a11y interaction rule is
       intentionally suppressed. -->
  <!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
  <div
    class="resize-grip"
    role="separator"
    aria-label="Resize window"
    title="Drag to resize the window"
    on:mousedown={startCornerResize}
  ></div>
</div>

<style>
  .dim { color: var(--text-faint); }

  /* CPE-1708: `--accent` is this app's INFO tone (app.css: "ERROR/INFO reuse --danger/--accent") — never
     `--danger`, which would read as "this folder failed to load" when it didn't. Same overflow strategy
     as `.notice` below (CPE-1660): truncate to an ellipsis, with the SAME sentence (`filteredHiddenText`
     above) plus the "loaded successfully" reassurance always readable via the `title` tooltip
     (`filteredHiddenTitle`) — so a narrow window that ellipsis-truncates the visible text never loses
     either the count or the reassurance, and the status bar's fixed height never grows for a long count. */
  .filtered-hidden {
    color: var(--accent);
    max-width: 45%;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 0 1 auto;
  }

  /* CPE-1780: same overflow/truncation strategy as `.filtered-hidden` above (same reasoning — a narrow
     window must still show the count via `title`), but `--warn` instead of `--accent`: an unreadable row
     is a genuine read FAILURE for that one row, distinct from a successful, intentional name filter. */
  .unreadable {
    color: var(--warn);
    max-width: 45%;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 0 1 auto;
  }

  /* The notice/toast text (CPE-1660): a plain <span> had no overflow strategy at all, so a notice
     longer than the window could hold wrapped to a second line and the fixed 26px `.statusbar` grew
     for the 5s it's shown. Option 1 from the ticket: truncate with an ellipsis instead of letting it
     reflow the chrome, with the full text still reachable via the `title` tooltip on the span above —
     keeps the bar's height a hard constant (this app's "predictable" tiebreaker) rather than option
     2's bounded-but-still-variable two-line clamp. `min-width: 0` opts the flex item out of its
     default min-content floor so it can actually shrink below its own text width instead of forcing
     the row wider or crushing its neighbours (git status, free space) at narrow widths; `max-width`
     additionally caps how much of the row a single notice may claim. Applies identically to the error
     variant (`.error` below only recolours — no separate overflow rule needed for it). */
  .notice {
    max-width: 45%;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 0 1 auto;
  }
  /* Git sync + free space sit at the far right, away from the item/selection counts. */
  .git { display: flex; align-items: center; gap: 6px; margin-left: auto; }
  .git-branch { opacity: 0.85; }
  .git-ct { font-variant-numeric: tabular-nums; opacity: 0.8; }
  .git-dirty { color: var(--warn); }
  .git-conflict { color: var(--warn); font-weight: 600; }
  .git-btn.resolve { border-color: var(--warn); }
  .git-btn { font-size: 11px; padding: 1px 7px; cursor: pointer; border: 1px solid var(--border-strong, #555);
             background: transparent; color: inherit; border-radius: 4px; }
  .git-btn:hover { background: var(--selection, rgba(128,128,128,0.2)); }
  .disk { margin-left: 12px; }

  /* Classic bottom-right sizing grip: three diagonal strokes in the corner, clipped to the
     lower-right triangle. Theme-variable coloured so it reads identically light/dark (CPE-842). */
  .resize-grip {
    position: absolute;
    right: 0;
    bottom: 0;
    width: 16px;
    height: 16px;
    cursor: nwse-resize;
    background-image: repeating-linear-gradient(
      -45deg,
      var(--text-faint) 0 1.5px,
      transparent 1.5px 4px
    );
    clip-path: polygon(100% 0, 100% 100%, 0 100%);
    opacity: 0.7;
  }
  .resize-grip:hover { opacity: 1; }
</style>
