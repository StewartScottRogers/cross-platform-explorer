// CPE-1866 — the reset hook that makes session-per-shard safe.
//
// wdio.conf.ts now groups every spec in a shard into ONE nested `specs` entry, so WDIO creates a
// SINGLE session (one app launch) for the whole shard instead of one per spec file — see that file's
// header comment on `specs` for the measured reason (app-launch/session-create is ~30.4-32.6s per spec,
// essentially the WHOLE ~29.5s fixed overhead CPE-1858 measured; driver-process spawn is <0.3s and was
// never the lever). A shared app process means IN-MEMORY state that used to reset for free on every
// relaunch — current folder, selection, an open dialog/drawer, window size, and (CPE-1866's own red-proof
// below) synthetic Agent Watch sessions injected via the `__CPE_TEST_INGEST_*` test-mode hooks — now
// carries over between spec files unless something puts it back. This is that something: called once per
// spec file, from wdio.conf.ts's `handleRunnableStart` (see that file's comment for why it is
// `beforeTest`/`beforeHook`, not the config-level `beforeSuite`/`afterSuite` hooks the first version of
// this fix wrongly used), BEFORE that file's own `before()`/`it()`s run.
//
// What this does NOT do, and does not need to: touch anything on DISK. `onPrepare` seeds every fixture
// ONCE and `onComplete` cleans up ONCE at the very end of the whole run — on-disk state under the seeded
// tmpDir (and the app-data dir Tauri resolves for this build) was ALREADY shared across every spec
// file's own fresh app relaunch before this ticket; every "isolated in its own subfolder, never
// perturbs any other spec's fixtures" comment in wdio.conf.ts already assumes exactly that. The NEW
// risk surface this ticket introduces is narrowly in-memory state (UI + the Agent Watch test-mode
// stores), so that is all this resets.
//
// WHAT THIS DELIBERATELY DOES NOT COVER — named explicitly (reviewer finding) so the next contributor
// self-cleans the way `specs/preview-pane.smoke.ts` already does for theme/pane-width (its own
// `afterEach`, written BEFORE this ticket existed, defensively, for exactly this shared-session shape —
// the model to copy, not a coincidence): sort order/column config, view mode (list/grid/gallery),
// filter/search text, sidebar section expansion, the open-tab set (dual-pane / tab strip), and
// OS/system clipboard contents. (Scroll position WAS on this list until CPE-1866's own gauntlet caught
// it — see SCROLL_CONTAINER_SELECTOR's comment below; it is now reset, not merely named.) None of these
// has caused an observed failure as of this ticket — this suite happens not to have two specs that
// collide on them today — but
// none is reset here either, so a future spec that sets one and assumes a fresh-launch baseline should
// clean up after itself (`afterEach`) rather than rely on this file growing a matching reset for
// everything the app can accumulate. Backend/server-side state is a SEPARATE, larger gap: this file only
// ever touches the FRONTEND (WebDriver commands + test-mode hooks into the Svelte stores) — it has no
// mechanism to reset backend-resident state like `IndexService` (see `specs/instant-search.smoke.ts`'s
// own corrected header comment for the concrete, currently-dormant case this creates).
//
// CPE-1979 — THE ONE THAT WAS NOT ON THAT LIST, and cost the most. A LAYERED VIEW (the in-app archive
// browser, a smart folder, a saved structured search) renders a listing other than `currentPath`'s own
// while leaving `currentPath` itself untouched. `specs/archive-browse.smoke.ts` ends inside a `.tar.gz`
// and never leaves, so every following spec in shard 2 walked in with the archive still open — and step 4
// below could not clear it, because `NavToolbar.svelte#commit()` short-circuited on `value ===
// currentPath` and never dispatched the `navigate` that `loadPath` (the app's single chokepoint for
// dismissing all three views) hangs off. Measured over the 16h50m window 2026-08-28T00:21Z-17:11Z: of
// the 81 completed `gui-smoke (ubuntu-latest) shard 2` jobs in it, 77 have a retrievable log AND reached
// this transition, and 77 of those 77 threw `expected the breadcrumb to show "cpe-gui-smoke-XXXXXX"` out
// of `navigateTo` here, green jobs included — the trigger for every one of those 77
// `handleRunnableStart:resetFailedRestartingSession` lines, and (via the session restart's
// `DELETE /session/<id>`) for 11 of them spending a tauri-driver respawn. The other 4 jobs were all
// CANCELLED and were never inspected — 3 whose log 404s, 1 killed before the transition — so this says
// nothing about them, deliberately: the population is those 77, not "the window".
//
// The fix is in the APP, not here: `commit()` now takes `pathOverlaidByView` from App.svelte and lets the
// same-path submit through. That makes step 4 load-bearing on a real product behaviour — say so out loud,
// because it is a coupling a reader would not guess. A DELIBERATE non-fix, for the same reason: this file
// did NOT grow an archive-specific escape hatch (a Backspace, a first-crumb click), and
// `archive-browse.smoke.ts` did NOT grow an `afterEach` that exits the archive, even though this file's
// list above points at exactly that convention. Either would have made the reset pass while the app stayed
// broken for every real user who types a path to get out of an archive — and would have destroyed the only
// detector that found this at all. The harness driving the SAME primitive a user drives is the feature.
import { $$, browser } from "@wdio/globals";
import { navigateTo } from "./samplesNav.js";

/** `src-tauri/src/lib.rs`'s own `.inner_size(1000.0, 700.0)` — the window's real default. Restored here
 *  because `specs/terminal-panel.smoke.ts` deliberately widens the window ("scoped to this spec/session
 *  only", by comment, written when every spec DID get its own session) to reach a command-bar button
 *  past 1000px; under a shared session that resize would otherwise leak into every spec that runs after
 *  it in the same shard, and several of them (archive-browse/archive-password/shred-dialog, per the
 *  CPE-1249 fold-position incident this file's fixture-ordering comments already document) are
 *  sensitive to exactly this kind of layout shift. */
const DEFAULT_WINDOW_WIDTH = 1000;
const DEFAULT_WINDOW_HEIGHT = 700;

/** Matches EVERY explicit close button in this app, dialog or not — a convention independently followed
 *  by 34+ `src/lib/components/*.svelte` files (`title="Close"` literal, or `title={$t("common.close")}`,
 *  which renders identically since `"common.close": "Close"` in the default English locale this harness
 *  runs under), each wired to `on:click={() => dispatch("close")}`. Escape alone (below) is NOT enough:
 *  it closes the ~49 `*Dialog.svelte` components (verified — they all reference it, the app-wide
 *  MENUS.md convention), but `AgentTimeline.svelte` (the Agent Watch drawer) is not one of them — it has
 *  NO Escape handling at all, only its own `.tl-close` button, which this selector also matches. Found
 *  the hard way: CPE-1866's first real CI run under session-per-shard cascaded 13 of 14 specs in one
 *  shard red, root-caused to exactly this — `checkpoint-restore.smoke.ts` opened the drawer, Escape
 *  never closed it, and the NEXT spec's click on the drawer's own OWN toggle button
 *  (`.agent-log-btn`) closed it again instead of opening it fresh, which is not what that spec's test
 *  expected (see this ticket's Work Log for the pasted evidence and the fix). */
const CLOSE_BUTTON_SELECTOR = 'button[title="Close"], button[aria-label="Close"]';

/** The full-screen dialog scrim — `position: fixed; inset: 0; z-index: 200`, `on:click={() =>
 *  dispatch("close")}` — independently used by 60 `src/lib/components/*.svelte` dialogs (verified: every
 *  one greps for `class="backdrop"`). Checked and clicked BEFORE the Close-button loop below, not after:
 *  clicking the backdrop itself closes the dialog whether or not its own Close button can be located, so
 *  it also recovers from an ORPHANED backdrop — one whose dialog content already unmounted (e.g. the
 *  Close-button click handler ran) but whose backdrop element, for whatever reason, did not — which would
 *  otherwise present as a full-viewport click-interceptor with no discoverable button inside it at all,
 *  reproducible CI evidence of exactly that shape being this ticket's Work Log (`element click
 *  intercepted`, deterministic across reruns, on the spec file immediately following one that opens a
 *  backdrop-style dialog and explicitly closes it before its test function returns). */
const BACKDROP_SELECTOR = ".backdrop";

/** Bounded — a genuinely stuck overlay must not hang the reset forever; 5 rounds is generous headroom
 *  over the deepest nesting any spec in this suite produces (a dialog opened from within a drawer, at
 *  most two levels). */
const MAX_CLOSE_ROUNDS = 5;

/** The Operations panel (`TransferPanel.svelte`) is idle-hidden — `{#if $transfers.length > 0}` — and
 *  every row's own dismiss/cancel button is `title="Dismiss"`/`title="Cancel"`, NOT `title="Close"`, so
 *  {@link CLOSE_BUTTON_SELECTOR} does not reach it (deliberately kept separate rather than widened to
 *  match "Cancel" everywhere — a generic "Cancel" can mean something else entirely elsewhere in the app,
 *  e.g. a wizard step, and conflating the two would make this loop's intent unclear). Found the same way
 *  as the Agent Watch drawer leak: `specs/transfer-panel.smoke.ts` line 130 already asserted
 *  `expect(await $(".ops").isExisting()).to.equal(false)` with the comment "The panel must not already
 *  show a leftover row from an earlier spec" — written when every spec DID get its own session, so it
 *  was always true for free; under a shared session it is exactly the assertion that catches THIS leak,
 *  and did, in CI, on the second real run under session-per-shard (this ticket's Work Log). Both button
 *  kinds are handled the same way here: a finished transfer is dismissed, and a still-running one
 *  (unlikely at reset time, but not impossible) is cancelled — either is the correct "get back to no
 *  leftover rows" outcome for the next spec. */
const OPS_PANEL_SELECTOR = ".ops";
const OPS_ROW_BUTTON_SELECTOR = ".ops .x";

/** `FileList.svelte`'s own scroll container — `scrollEl = rowsEl.closest(".filelist-pane")`. Reset
 *  explicitly here because CPE-1866's own gauntlet caught a real leak `navigateTo(rootDir)` (step 5
 *  below) does NOT reliably clear: `NavToolbar.svelte#commit()` reads `if (!value || value ===
 *  currentPath) return;` BEFORE ever dispatching a `navigate` event — so when the app is ALREADY at
 *  `rootDir` (the common case: most specs in a shard operate at or near the seeded root, so this is
 *  true more often than not), `navigateTo`'s whole Ctrl+L/type/Enter sequence is a no-op from the
 *  listing's own perspective — no re-fetch, no re-mount, no scroll reset — and whatever scrollTop an
 *  EARLIER spec left on `.filelist-pane` carries straight through into the next file. Confirmed, not
 *  guessed: a real CI run's `document.elementFromPoint` diagnostic (`specs/open-dir.smoke.ts`) named
 *  the file pane's own `.toolbar` as the topmost element at a fixture row's click point
 *  (`topSameAsRow: false`) — exactly the shape a scrolled list produces (a row's computed rect lands
 *  where the toolbar sits, because the row itself scrolled up underneath it), not an overlay/backdrop
 *  leak (the element chain was ordinary pane furniture, no dialog/drawer anywhere in it). */
const SCROLL_CONTAINER_SELECTOR = ".filelist-pane";

/** Presses Escape, then clicks the first visible explicit "Close" button if one remains, up to
 *  {@link MAX_CLOSE_ROUNDS} times — closing whatever a prior spec left open regardless of whether it
 *  happens to be a `*Dialog.svelte` (closes on Escape) or a drawer/panel like `AgentTimeline.svelte`
 *  (does not — see {@link CLOSE_BUTTON_SELECTOR}'s comment). Defensive: a click that fails (mid-
 *  animation, element gone between the query and the click) is swallowed rather than aborting the whole
 *  reset — the goal is to get as close to clean as realistically possible, not to guarantee zero
 *  overlays on every possible failure shape; the independence evidence in this ticket's Work Log is what
 *  actually certifies this works in practice. */
async function closeAnyOpenOverlay(): Promise<void> {
  for (let round = 0; round < MAX_CLOSE_ROUNDS; round++) {
    await browser.keys(["Escape"]);
    const backdrops = await $$(BACKDROP_SELECTOR);
    if ((await backdrops.length) > 0) {
      try {
        await backdrops[0]!.click();
      } catch {
        // best-effort — see this function's doc comment.
      }
      continue;
    }
    const closeButtons = await $$(CLOSE_BUTTON_SELECTOR);
    if ((await closeButtons.length) === 0) return;
    try {
      await closeButtons[0]!.click();
    } catch {
      // best-effort — see this function's doc comment.
    }
  }
}

/** Dismisses/cancels every row in the Operations panel (see {@link OPS_PANEL_SELECTOR}'s comment for
 *  why this needs its own loop, separate from {@link closeAnyOpenOverlay}), bounded the same way and for
 *  the same reason. */
async function clearOperationsPanel(): Promise<void> {
  for (let round = 0; round < MAX_CLOSE_ROUNDS; round++) {
    const panel = await $$(OPS_PANEL_SELECTOR);
    if ((await panel.length) === 0) return;
    const rowButtons = await $$(OPS_ROW_BUTTON_SELECTOR);
    if ((await rowButtons.length) === 0) return;
    try {
      await rowButtons[0]!.click();
    } catch {
      // best-effort — see closeAnyOpenOverlay's doc comment for the same reasoning.
    }
  }
}

/** Zeroes {@link SCROLL_CONTAINER_SELECTOR}'s `scrollTop` directly via `browser.execute` (not via
 *  navigation — see that constant's comment for why `navigateTo` cannot be trusted to do this itself),
 *  logging the before/after value both times so a real CI run's log carries the evidence rather than
 *  asking a reader to trust it. A no-op (still logged) if the pane isn't in the DOM at all, which
 *  should not happen this late in the reset (`navigateTo` above already confirmed the breadcrumb) but
 *  is handled rather than assumed. */
async function resetFileListScroll(): Promise<void> {
  const result = await browser.execute((sel) => {
    const el = document.querySelector(sel) as HTMLElement | null;
    if (!el) return { found: false, before: null, after: null };
    const before = el.scrollTop;
    el.scrollTop = 0;
    return { found: true, before, after: el.scrollTop };
  }, SCROLL_CONTAINER_SELECTOR);
  // eslint-disable-next-line no-console
  console.log(`[gui-smoke][resetFileListScroll] ${JSON.stringify(result)}`);
}

/** Restores the shared session to the same starting point a fresh app launch used to provide, before
 *  the next spec file's tests run:
 *   1. Window size back to the app's real default — see the constants' comment above.
 *   2. `__CPE_TEST_CLEAR_AGENT_SESSIONS__` — a test-mode-only hook (App.svelte, CPE-1866, mirroring the
 *      existing `__CPE_TEST_INGEST_SESSION__`/`__CPE_TEST_INGEST_ACTIVITY__`/`__CPE_TEST_INGEST_COST__`
 *      convention). PRECISELY what it does (reviewer correction — the previous wording overstated this):
 *      it calls `clearAgentSessions()`, which empties `$agentSessions` DIRECTLY and SYNCHRONOUSLY
 *      (`store.set([])`). It does NOT directly touch the activity (`$fsActivity`)/cost (`$agentCost`)
 *      stores those OTHER two ingest hooks feed — those clear INDIRECTLY, as a side effect of
 *      App.svelte's existing `$: reconcileAgentWatch($agentSessions, currentPath);` reactive statement
 *      noticing `$agentSessions` is now empty and tearing down the same way a real session's `ended`
 *      event would (the exact mechanism `closeAllConsoles()` already relies on for the same cleanup, so
 *      the PATTERN is trusted — it is not a new invention here). Clearing `$agentSessions` also closes
 *      the Agent Watch drawer as a side effect, via `$: if (!activeWatchCwd) showTimeline = false;` —
 *      again the same real lifecycle path a genuine session-end event drives. What this function does
 *      NOT do: explicitly AWAIT that reactive teardown settling. Svelte's own reactivity flush and the
 *      `reconcileAgentWatch` chain it triggers are not exposed as an awaitable promise from here, so
 *      there is no direct handle to wait on; the WebDriver round trips `clearOperationsPanel`/
 *      `closeAnyOpenOverlay`/`navigateTo` make immediately afterward are the de facto settle time in
 *      practice, but this is not a guarantee. Needed because `checkpoint-restore.smoke.ts`/
 *      `cost-history.smoke.ts`/`cost-ledger.smoke.ts`/`radar.smoke.ts`/`replay.smoke.ts` all inject a
 *      synthetic session and none of them ever tears it down — every one was written assuming
 *      (correctly, before this ticket) that the session would die with the app process at the end of
 *      its OWN spec file.
 *   3. {@link closeAnyOpenOverlay} — Escape + explicit Close-button clicks for whatever that missed.
 *   4. `navigateTo(rootDir)` — the SAME address-bar navigation primitive `samples.smoke.ts` already uses
 *      to hop between dozens of folders in one continuous session (CPE-1358), re-used here to return to
 *      the seeded tmpDir root and reconfirm via the breadcrumb that navigation actually landed, rather
 *      than assuming steps 2-3 alone left the app somewhere sane.
 *   5. {@link resetFileListScroll} — zeroes the file list's own scroll position directly. Deliberately
 *      AFTER `navigateTo`, not before: `navigateTo` can itself trigger a real navigation (when the app
 *      was NOT already at `rootDir`), which re-fetches/re-mounts the listing and would reset scroll on
 *      its own — but when it does NOT (the app already at `rootDir`, the common case — see
 *      `SCROLL_CONTAINER_SELECTOR`'s comment), nothing else in this function touches scroll at all, so
 *      this step is not redundant insurance, it is load-bearing on its own.
 *
 *  See this file's header comment for what this function deliberately does NOT reset (sort order, view
 *  mode, filter text, sidebar expansion, tab set, clipboard, and any backend-resident state such as
 *  `IndexService`) — scroll position used to be on that list; it no longer is, see step 5 above. */
export async function resetAppState(rootDir: string): Promise<void> {
  // CPE-1866: the scrollTop this spec file's PREDECESSOR actually left behind — captured before ANY
  // reset step runs, so this number is the direct answer to "was the list scrolled walking in", not
  // conflated with whatever `navigateTo` below may or may not have already changed by the time
  // `resetFileListScroll` runs its own before/after pair.
  const enteringScrollTop = await browser.execute((sel) => {
    const el = document.querySelector(sel) as HTMLElement | null;
    return el ? el.scrollTop : null;
  }, SCROLL_CONTAINER_SELECTOR);
  // eslint-disable-next-line no-console
  console.log(`[gui-smoke][resetAppState] entering scrollTop=${JSON.stringify(enteringScrollTop)}`);
  await browser.setWindowSize(DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT);
  await browser.execute(() => {
    const hook = (window as unknown as { __CPE_TEST_CLEAR_AGENT_SESSIONS__?: () => void })
      .__CPE_TEST_CLEAR_AGENT_SESSIONS__;
    hook?.();
  });
  await clearOperationsPanel();
  await closeAnyOpenOverlay();
  await navigateTo(rootDir);
  await resetFileListScroll();
}
