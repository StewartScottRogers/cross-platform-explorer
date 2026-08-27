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
  // UAT round (2026-08-20): both notes previously led with "N entries…", so a fast skim could
  // momentarily read them as one count. Leads with a different word than `filteredHiddenText` now —
  // same fact, same words otherwise, just reordered so the two are distinct at a glance.
  $: unreadableText = unreadableCount === 1
    ? "Couldn't read 1 entry"
    : `Couldn't read ${unreadableCount} entries`;
  $: unreadableTitle = `${unreadableText} — the rest of the folder loaded successfully.`;

  // CPE-1833: neither note was ever announced to a screen reader — `.filtered-hidden`/`.unreadable`
  // above are `{#if}`-conditional, so each appears as a BRAND NEW element already holding its final
  // text, which is exactly the shape Chromium+Windows AT (WebView2 with NVDA/Narrator — this app)
  // routinely fails to announce even with `role="status"` on the span itself (the same lesson CPE-1816
  // recorded the same day: a live region must already exist in the accessibility tree BEFORE its
  // content changes, or the mutation can be missed). The fix is a SEPARATE, always-mounted announcer —
  // never conditionally rendered, never removed — decoupled from the two visible pills (which keep
  // their own colour/truncation/title exactly as before, see below). Its own text is a single reactive
  // string, so a screen reader observing it sees ONE full-sentence update, not two independent
  // insertions; `aria-atomic="true"` (below, in the markup) makes that a single coherent announcement of
  // the whole sentence even when both notes change in the same tick, rather than "two competing
  // sentences" (CPE-1833 AC).
  $: advisoryAnnouncement = [
    filteredHidden > 0 ? filteredHiddenText : null,
    unreadableCount > 0 ? unreadableText : null,
  ]
    .filter((s): s is string => s !== null)
    .join(". ");
</script>

<div class="statusbar">
  <span class="item-count">
    {#if isFiltered}
      {itemCount} of {totalCount} items
    {:else}
      {itemCount} item{itemCount === 1 ? "" : "s"}
    {/if}
  </span>

  {#if selectedCount > 0}
    <span class="selected-count">
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
         truncating the visible text (see `.filtered-hidden` below) never loses either. CPE-1833:
         `tabindex="0"` plus the `:focus-visible` rule below make the full sentence reachable without a
         mouse — `title` alone is hover-only. The element's own text content is already the FULL
         sentence (CSS `text-overflow: ellipsis` only clips what's painted, never the DOM text), so no
         separate `aria-label` is needed for the accessible name; screen-reader announcement of the
         mid-session change is handled by the separate persistent live region below, not by this span. -->
    <!-- svelte-ignore a11y-no-noninteractive-tabindex -- deliberate: a plain focusable static-text span
         is the WCAG "reveal truncated content on focus" technique (mirrors LogPreview.svelte's
         `.log-body` SCR29 precedent for the same lint rule). Without this the text is permanently
         truncated for any keyboard user with no mouse to hover the `title` with.
         CPE-1883: `data-reveal` duplicates this span's own text for the `:focus-visible::after` overlay
         (see that rule's own comment for why the reveal is a SEPARATE generated-content box rather than
         resizing this span itself) — `content: attr(...)` can only read a plain attribute, not this
         span's text-node children, so the same string has to exist both places. -->
    <span class="filtered-hidden" title={filteredHiddenTitle} tabindex="0" data-reveal={filteredHiddenText}>
      {filteredHiddenText}
    </span>
  {/if}

  {#if unreadableCount > 0}
    <!-- CPE-1780: a separate NOTE from `.filtered-hidden` above, worded for a different fact (a read
         failure, not a name refused) — see `unreadableCount`'s doc. `--warn` (not `--accent`) because this
         one IS a real read failure, not just an intentional/successful name filter. CPE-1833:
         `tabindex="0"` — see the comment on `.filtered-hidden` above; same reasoning applies here. -->
    <!-- svelte-ignore a11y-no-noninteractive-tabindex -- see the comment on `.filtered-hidden` above. -->
    <span class="unreadable" title={unreadableTitle} tabindex="0" data-reveal={unreadableText}>
      {unreadableText}
    </span>
  {/if}

  <!-- CPE-1833: the announcer. ALWAYS mounted — never `{#if}`-gated — so it is already present in the
       accessibility tree before either note's text changes; only its text content mutates. `aria-atomic`
       re-announces the WHOLE region on any change, so when both notes change in the same tick they read
       as one coherent sentence instead of two competing ones. Visually hidden (`.sr-only`, below) because
       the two pills above already carry the same facts for sighted users — this exists purely so the
       change is announced at all, per the AC's "verify the mid-change announcement actually happens". -->
  <div class="advisory-live sr-only" role="status" aria-live="polite" aria-atomic="true">
    {advisoryAnnouncement}
  </div>

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
  /* CPE-1780 (Visual Critic, round 3 — corrects round 2's comment, which was wrong): in a fixed-height
     single-row `.statusbar` (no `flex-wrap`) there is NO SUCH THING as a child that "never truncates" —
     round 2 gave `.item-count`/`.selected-count`/`.dim` `min-width: 0; white-space: nowrap;` with NO
     `overflow: hidden`, on the theory that they'd just overflow their own box harmlessly at an extreme.
     Measured reality: with `min-width: 0` removing their content-width floor and nothing clipping the
     result, their text shrinks its BOX but keeps painting at full width — it visually overlaps the NEXT
     element instead of wrapping ("42 item12 selected", genuinely illegible). Worse than the wrap it
     replaced. The honest semantic is SHRINK PRIORITY, not immunity: every child below now gets `min-width:
     0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;` — so nothing can ever overlap its
     neighbour or wrap the bar taller — and a `flex-shrink` weight encodes which group gives up space
     first. `--priority-shrink: 10` (defined once below, referenced by every "shrinks first" rule) is
     picked LARGE relative to `--priority-stay: 1` specifically so the flexbox shrink-then-freeze algorithm
     drains the whole "shrinks first" group toward its own floor (0, courtesy of `min-width: 0`) before the
     "shrinks last" group gives up any meaningful width — not just proportionally-less, but LAST in
     practice for any deficit this bar will realistically see. Once a "shrinks last" element genuinely runs
     out of room (the "shrinks first" group already at/near zero), it ellipses too — it is never immune,
     only lowest priority. Two groups, in flex order:
       1. SHRINKS LAST (`--priority-stay`) — `.item-count`, `.selected-count`, `.dim` (so "Hidden files
          shown"): short, load-bearing facts, kept whole as long as there is ANY room to spare elsewhere.
       2. SHRINKS FIRST (`--priority-shrink`), in this sub-order when even THAT group runs out of shared
          room: `.filtered-hidden`/`.unreadable`/`.notice` (variable-length prose, but already-established
          status), then `.git`/`.git-branch` (a branch name can be long; `.git`'s counts/dirty-dot/buttons
          stay `flex: 0 0 auto` — genuinely never shrink, since shrinking a clickable button is worse than
          truncating a name), then `.disk` (free-space is the least essential fact on the bar). Every
          element's full text stays reachable via its own `title` tooltip regardless of which group is
          currently truncating. */
  .statusbar {
    --priority-stay: 1;
    --priority-shrink: 10;
  }
  .dim {
    color: var(--text-faint);
    flex: 0 var(--priority-stay) auto;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* The item/selection counts (unclassed `<span>`s in markup before CPE-1780) are the FIRST children of
     `.statusbar`, so before round 2's fix they absorbed the wrap-of-last-resort: once every LATER sibling
     could shrink safely, the narrow-width deficit had to land SOMEWHERE and wrapped "42 items" onto two
     lines. `--priority-stay` (see the ordering comment above `.dim`) keeps them lowest-priority to shrink
     — but, per that same comment, never immune: `overflow: hidden; text-overflow: ellipsis;` so if they
     DO ever have to give up width, they clip cleanly instead of painting over `.selected-count`/`.dim`. */
  .item-count, .selected-count {
    flex: 0 var(--priority-stay) auto;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* CPE-1708: `--accent` is this app's INFO tone (app.css: "ERROR/INFO reuse --danger/--accent") — never
     `--danger`, which would read as "this folder failed to load" when it didn't. Same overflow strategy
     as `.notice` below (CPE-1660): truncate to an ellipsis, with the SAME sentence (`filteredHiddenText`
     above) plus the "loaded successfully" reassurance always readable via the `title` tooltip
     (`filteredHiddenTitle`) — so a narrow window that ellipsis-truncates the visible text never loses
     either the count or the reassurance, and the status bar's fixed height never grows for a long count.
     `--priority-shrink` (see the ordering comment above `.dim`) — this is a SHRINKS-FIRST element.
     `position: relative` unconditionally (not just on focus): CPE-1883's `:focus-visible::after` reveal
     needs a STABLE containing block that is never itself resized by focus — see that rule's own comment
     for why. */
  .filtered-hidden {
    position: relative;
    color: var(--accent);
    max-width: 45%;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 0 var(--priority-shrink) auto;
  }

  /* CPE-1780: same overflow/truncation strategy as `.filtered-hidden` above (same reasoning — a narrow
     window must still show the count via `title`), but `--warn` instead of `--accent`: an unreadable row
     is a genuine read FAILURE for that one row, distinct from a successful, intentional name filter.
     `position: relative` — see the identical note on `.filtered-hidden` above. */
  .unreadable {
    position: relative;
    color: var(--warn);
    max-width: 45%;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 0 var(--priority-shrink) auto;
  }

  /* CPE-1833: the full sentence, reachable without a mouse. `title` (kept above, on both pills, for
     hover) is not exposed to keyboard-only or screen-reader users, so `tabindex="0"` on `.filtered-hidden`
     / `.unreadable` makes each Tab-reachable, and this rule reveals the whole sentence on focus for a
     sighted keyboard user who has no mouse to hover with — the visual truncation these pills otherwise
     apply is a rendering choice, not a loss of the underlying text, so focus simply turns that clipping
     back off. `position: relative` (kept on the base rule below, unconditionally — not just on focus) so
     the reveal has a stable anchor: see the `::after` rule's own comment for why the reveal is generated
     content on a pseudo-element rather than a resize of this span itself.
     CPE-1883 diagnosis: `max-width` above was never the whole story. The FIRST fix tried was resizing
     THIS span directly on `:focus-visible` (`overflow: visible; white-space: normal; max-width: ...`) —
     it never worked, for two DIFFERENT reasons tried in sequence:
       1. As shipped (no flex override): the base rule's `flex: 0 var(--priority-shrink) auto` (a
          SHRINKS-FIRST item, see the ordering comment above `.dim`) still applied, so the flex algorithm
          kept squeezing the item toward its shrink-allocated share of the row regardless of `max-width`
          sitting unreached above it — and once `white-space: normal` legalised wrapping, that squeeze
          bottomed out at the single longest WORD's min-content width, producing the ticket's tall
          one-word-per-line column instead of a wide box.
       2. Tried `flex: 0 0 auto` (stop shrinking) to fix #1 — it DOES stop the column, but by giving the
          box real layout width inside the flex row at the moment it's needed most (a crowded busy row),
          which squeezes `.git`/`.disk` (both SHRINKS-FIRST too, later in priority) toward ZERO width
          while a note is focused — measured, not assumed: at 600px busy `.git`/`.disk` both collapsed to
          `width: 0`. Fixing this ticket's bug by breaking a neighbour is exactly what this component's
          rules forbid. Also tried `position: absolute` directly on the span (removing it from flex
          layout entirely, so it can't squeeze siblings) — that solves the squeeze but trades it for a
          WORSE regression: Chromium's static-position computation for an absolutely-positioned FLEX
          CHILD does not reliably reproduce its in-flow location (a documented cross-browser rough edge,
          not a mistaken assumption on this codebase's part) — measured jumping to `left: 0` (the row's
          own padding edge) regardless of how many earlier siblings (`.item-count`, `.selected-count`,
          `.dim`) it should have sat after, covering them instead of the neighbours to its right the
          original design intended to overlay.
     Both attempts share the same root problem: THIS element's own box is a flex item whose size/position
     depends on its siblings, so restyling it directly can never cleanly detach it from the row without
     one of the above trade-offs. The actual fix moves the reveal to a `::after` PSEUDO-element instead
     (below) — its containing block is this span (via `position: relative`, kept always-on so the
     anchor never changes across DOM updates), and that span's own box is NEVER resized by focus, so it
     never re-enters the flex-shrink negotiation and `.git`/`.disk` never lose anything; the pseudo is
     `position: absolute; left: 0; top: 0;` relative to that STABLE anchor (a plain relative-parent
     absolute-child, not a flex-child static position — no ambiguity), so it starts exactly where the
     narrow pill visually sits and grows to the right, over whatever neighbour it would otherwise be
     clipped against — the original design's own intent, now actually achieved. */
  /* CPE-1883: the reveal itself. `content: attr(data-reveal)` reads the SAME sentence the markup already
     puts in this span's own title/text (see the markup comment on `data-reveal` for why a duplicate
     attribute is needed — generated content can only read a plain attribute, never text-node children).
     `overflow: visible` on the BASE rule's sibling above would defeat its own `text-overflow: ellipsis`
     (which requires `overflow: hidden` to do anything), so it is set HERE, scoped to focus, and only
     matters for letting this pseudo-element's box paint past its (unmoved) parent's clip region — the
     parent's own text stays correctly ellipsis-clipped underneath, invisible only because this opaque
     box paints after it (`::after` = last-painted) and fully covers it (same anchor corner, one string,
     so their extents coincide).
     `width: max-content` turned out to be load-bearing, not decorative — its absence was a THIRD way to
     reproduce this exact ticket, discovered measuring this very attempt: `position: absolute; left: 0;`
     with `width` left at its default `auto` still computes a "shrink-to-fit" width CONSTRAINED BY THE
     CONTAINING BLOCK's own remaining space (CSS2.1 §10.3.7) — and the containing block here is this
     narrow, still flex-shrunk SPAN (58px at 600px busy), so the pseudo dutifully wrapped to fit inside
     ITS PARENT's 58px, reproducing the identical one-word-per-line column one level down. `width:
     max-content` bypasses that: it is not `auto`, so the shrink-to-fit-within-containing-block algorithm
     never runs, and the box sizes to its own max-content width instead — `max-width` then still clamps
     that, exactly as intended, independent of the narrow parent.
     Measured before/after via `scripts/dev-harness/layout-guard` (statusbar-focus-reveal case, which
     measures this `::after` box via `getComputedStyle` — pseudo-elements have no
     `getBoundingClientRect()`) and the ticket's own work log: 600px compound-busy went 63.9x148px (the
     span itself, pre-fix) -> 367.3x16px (this `::after` box, a single readable line); 900px went
     157x52px -> the same 367.3x16px — both with `.git`/`.disk`/`.item-count`/`.selected-count` measured
     BYTE-FOR-BYTE identical to their unfocused rects (the span's own box, and therefore the row's flex
     layout, is never touched by any of this).
     ROUND 2 (Visual Critic UAT, real Chrome re-render of the shipped CSS at 600x26/900x26, both
     themes) caught four more defects `npm run check`/jsdom structurally cannot see, all fixed here:
       1. `left: 0` anchored the box's LEFT edge to the pill, so it grew RIGHTWARD — at the app's own
          600px width floor WITH the full compound-busy row, that ran the box ~100px past the viewport
          edge, and `body { overflow: hidden }` (app.css) silently clipped the SENTENCE'S TAIL — no
          ellipsis, no scroll, just gone. Measured: "…their names could" visible, "not be shown safely"
          not. That is WORSE than the original bug: the one-word-per-line column was ugly but showed
          every word. Fixed by anchoring the opposite edge instead — `right: 0; left: auto` — so the box
          grows LEFTWARD from the pill's right edge. Unconditional (not just a 600px media query): the
          pill always sits in the right half of a busy row (after item-count/selected-count/hidden-shown,
          before .git/.disk), so growing leftward runs over those ALREADY-TRUNCATED counts rather than
          off the right edge of the window — see the ticket's work log for the re-measured on-screen
          rects at both widths.
       2. Vertical alignment: `top: 0` anchors to the pill's 16px text box, then this rule's own 2px
          padding + the 1px ring pushed the visible box 5px down inside the 26px bar — measured
          bottom-flush with the bar's own edge (0.5px clear below, 4.5px above), shadow dying into the
          window boundary. `top: 50%; transform: translateY(-50%);` centres it in the bar instead,
          matching how a deliberate popover reads rather than a mis-anchored one.
       3. Dark-theme contrast: no `color` here means this inherits the PILL's `var(--accent)` — dark
          `#0078e0` on `--surface #2b2b2b` measures 3.21:1 for a full sentence of 12px body text, under
          the 4.5:1 AA floor (light is 5.5:1, fine). `--accent` is correctly a NON-text accent by design
          (focus rings, icons — see `app.css.dark-contrast.test.ts:270`, which only asserts >=3:1 on
          purpose), so that guard has no reason to catch a full sentence of body text using it — its
          blind spot, same family as CPE-1919/CPE-1921 (also guard-blind color-as-text uses), not a bug
          in the guard itself. The pill underneath KEEPS `--accent`/`--warn` (unaffected, still correct
          for a short truncated label); only the reveal — precisely the low-vision affordance this whole
          ticket is about — gets `color: var(--text)` instead.
       4. `::after` is part of its originating element for hit-testing, so while focused this box (up to
          367px wide) can paint over `.git`'s Pull/Push/Sync buttons and swallow their first click (it
          blurs the note instead of pressing the button). `pointer-events: none` removes that entirely —
          a hover/focus reveal has no interactive content of its own, so nothing is lost by letting
          clicks fall through to whatever it's covering.
     NOT changed, confirmed correct by the same UAT: `width: max-content` (still load-bearing, see
     above), the 1px `--border-strong` ring — do NOT "simplify" that away, it is carrying ALL of the
     visual separation in dark theme, where the box's `--surface` fill is only two steps off
     `.statusbar`'s own `--surface` background and would otherwise read as barely-there. */
  /* CPE-1883 round 2 fix, found capturing evidence for THIS round: `overflow: visible` here lets the
     span's OWN raw text (still `white-space: nowrap`, unaffected by anything else in this rule) paint
     unclipped past its own narrow box — with the original `left: 0` anchor that coincided with where the
     `::after` box also grew (both rightward from the same point), so the opaque overlay happened to
     cover it. `right: 0` (the round-2 fix above) grows the OVERLAY leftward while the raw underlying
     text still flows rightward as always — the two no longer occupy the same horizontal range, so the
     span's own unclipped text bleeds out to the right of the overlay, visible as a second, ellipsis-less
     copy of the sentence in the pill's own colour. `color: transparent` removes it from view entirely
     (safe for a11y the same way the rest of this component already treats colour-only hiding: the DOM
     text node, the accessible name computed from it, and the separate always-mounted `.sr-only` live
     region are all untouched by `color`) rather than trying to keep the overlay's geometry chasing the
     raw text's, which — given the raw text has no width constraint of its own — cannot be made to work
     in general. */
  .filtered-hidden:focus-visible,
  .unreadable:focus-visible {
    overflow: visible;
    color: transparent;
  }
  .filtered-hidden:focus-visible::after,
  .unreadable:focus-visible::after {
    content: attr(data-reveal);
    position: absolute;
    right: 0;
    left: auto;
    top: 50%;
    transform: translateY(-50%);
    z-index: 1;
    display: inline-block;
    white-space: normal;
    width: max-content;
    max-width: min(90vw, 420px);
    background: var(--surface);
    color: var(--text);
    border-radius: 4px;
    /* Do not drop this ring — see the comment above: in dark theme it is the ONLY thing separating this
       box from the bar behind it. */
    box-shadow: 0 0 0 1px var(--border-strong), 0 2px 6px rgba(0, 0, 0, 0.35);
    padding: 2px 4px;
    /* CPE-1883 round 2: a hover/focus reveal has no interactive content of its own — without this, the
       box (up to 367px wide) can sit over `.git`'s Pull/Push/Sync buttons while focused and swallow
       their first click (it blurs the note instead of pressing the button underneath). Verified via a
       real-Chrome hit-test sweep (`document.elementsFromPoint` over all three `.git-btn` centres at
       900px busy): every button is topmost/reachable with this in place, none are shadowed by the
       reveal.
       Open question, not a regression, left unexamined rather than silently assumed either way: `::after`
       generated content is not a text node, so it cannot be drag-selected, and the real `<span>`'s own
       text sits UNDER this opaque overlay — whether a sighted mouse user can still select the sentence
       by dragging across it is browser-dependent and untested here. Not a regression from this fix's own
       baseline: pre-fix there was no readable box to select from in the first place (a one-word-per-line
       column), so this doesn't take away a capability that existed before CPE-1883. */
    pointer-events: none;
  }

  /* CPE-1833: the persistent announcer for both advisory notes. ALWAYS mounted (see the markup comment
     above it) — a live region that appears/disappears with its content is exactly the shape that goes
     unannounced. Visually hidden with the standard "clip, don't display:none" technique: `display: none`
     / `visibility: hidden` remove a node from the accessibility tree in most browsers, which would defeat
     the whole point, whereas clipping a 1x1 box keeps it in the tree and reachable by AT while invisible
     to sighted users (who already see the same facts in the coloured pills above). */
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
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
    flex: 0 var(--priority-shrink) auto;
  }
  /* Git sync + free space sit at the far right, away from the item/selection counts. `.git` itself
     (CPE-1780) gets `min-width: 0;` plus the SHRINKS-FIRST priority (see the ordering comment above
     `.dim`) so the WHOLE block can shrink as a unit rather than forcing the row wider or wrapping — a
     `display: flex` container has no `white-space` of its own, so what actually needs to shrink is its
     variable-length child, `.git-branch`, below. */
  /* CPE-1836: at exactly the 600px floor, in the compound scenario (both notes + a busy row + a long
     branch), `.git`'s pinned `flex: 0 0 auto` children (the counts, dirty dot, buttons — intentionally
     never shrink, since shrinking a clickable button is worse than truncating a name, see the comment on
     `.git-branch` below) collectively exceed `.git`'s own shrunk box by ~16-33px. `.git` had no
     `overflow: hidden` of its own, so that overage painted OUTSIDE its box and into `.disk`'s — the
     "text painted over text" failure this whole file's ordering model exists to prevent (see the
     ordering comment above `.dim`: "every child needs overflow: hidden so that running out of room
     produces an ellipsis rather than text painted over text"). Fixed the same way as every other child:
     `overflow: hidden` on `.git` itself. This clips the LAST pinned child (the rightmost button) rather
     than bleeding into the next sibling — acceptable here because reaching this state at all requires the
     compound, sub-600px-floor-only scenario the ticket documents as "everything realistic is clean";
     letting the pinned children participate in the shrink (the alternative the ticket offered) was
     rejected because it would make a git action button partially unclickable while still fully visible,
     which is worse than a clean edge-clip. `.git-branch` still shrinks first (below), so the buttons are
     only ever touched once the branch name is already fully collapsed. */
  .git {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-left: auto;
    min-width: 0;
    overflow: hidden;
    flex: 0 var(--priority-shrink) auto;
  }
  /* A branch name can be arbitrarily long (CPE-1780): shrinks + truncates to an ellipsis like the other
     SHRINKS-FIRST elements (see the ordering comment above `.dim`), full name still in `title` via the
     parent `.git` span. Its OWN `flex-shrink` value only matters relative to ITS git-only siblings below
     (counts/dot/buttons), which are all `flex: 0 0 auto` — genuinely never shrink, since shrinking a
     clickable button is worse than truncating a name — so `.git-branch` absorbs 100% of whatever `.git`
     as a whole gives up. */
  .git-branch {
    opacity: 0.85;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 0 var(--priority-shrink) auto;
  }
  .git-ct { font-variant-numeric: tabular-nums; opacity: 0.8; flex: 0 0 auto; }
  .git-dirty { color: var(--warn); flex: 0 0 auto; }
  .git-conflict { color: var(--warn); font-weight: 600; flex: 0 0 auto; }
  .git-btn.resolve { border-color: var(--warn); }
  .git-btn { font-size: 11px; padding: 1px 7px; cursor: pointer; border: 1px solid var(--border-strong, #555);
             background: transparent; color: inherit; border-radius: 4px; flex: 0 0 auto; }
  .git-btn:hover { background: var(--selection, rgba(128,128,128,0.2)); }
  /* CPE-1780 Visual Critic finding (round 1): `.disk` had NO overflow strategy at all (unlike
     `.filtered-hidden`/`.unreadable`/`.notice` above), so it had never been forced to shrink hard enough
     to wrap — until both new notes on screen at once, at the app's own 600px floor (`.min_inner_size`,
     `src-tauri/src/lib.rs`), left it no room and its text wrapped to a second line, spilling OUTSIDE the
     statusbar's fixed 26px box. Same fix as `.notice`, now also SHRINKS-FIRST priority (see the ordering
     comment above `.dim`) — free-space is the least essential fact on the bar, so it's last in the
     shrinks-first sub-order (git-branch and the notes above give up room before `.disk` does). */
  /* CPE-1859: `.disk` ANCHORS ITSELF to the right edge. It used to carry only `margin-left: 12px` and
     sat right purely because `.git` precedes it carrying the row's one `margin-left: auto` — so with no
     repo in the current folder (`{#if git && git.is_repo}` removes the chip outright) the free-space
     text rendered next to the ITEM COUNT. That is not a rare race: it is the steady state of every
     non-repo folder. Measured in real Chrome via scripts/dev-harness/statusbar-notice at w=900 —
     `.disk` landed at left=84.9 / right=216.0, i.e. 670.0px short of the bar's right padding edge and
     26.0px from `.item-count` (the bar's 14px `gap` + this 12px margin).

     `margin-left: auto` alone — the obvious fix, and the one the ticket proposed — is WRONG, and this
     was measured rather than reasoned: flexbox distributes positive free space EQUALLY among all
     main-axis auto margins, so with `.git` also carrying one the chip stopped anchoring and parked
     mid-row, moving from left=637.3 to left=361.1 (276.2px) with both readouts present.

     Hence the pair. `.disk` owns the anchor by default; when `.git` actually precedes it, `.git`'s auto
     margin is the anchor and `.disk` reverts to the plain 12px separator it has always had — so the
     both-present layout is byte-identical to before (measured: `.git` left=637.3, `.disk` right=886.0
     flush with the content edge, in both the pre- and post-fix renders). The sibling rule is 0-2-0 and
     the base rule 0-1-0, so order in this stylesheet is not what decides it. */
  .disk {
    margin-left: auto;
    flex: 0 var(--priority-shrink) auto;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .git ~ .disk {
    margin-left: 12px;
  }

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
