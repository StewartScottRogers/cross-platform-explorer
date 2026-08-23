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
// spec file, from wdio.conf.ts's `beforeSuite` hook, BEFORE that file's own `before()`/`it()`s run.
//
// What this does NOT do, and does not need to: touch anything on DISK. `onPrepare` seeds every fixture
// ONCE and `onComplete` cleans up ONCE at the very end of the whole run — on-disk state under the seeded
// tmpDir (and the app-data dir Tauri resolves for this build) was ALREADY shared across every spec
// file's own fresh app relaunch before this ticket; every "isolated in its own subfolder, never
// perturbs any other spec's fixtures" comment in wdio.conf.ts already assumes exactly that. The NEW
// risk surface this ticket introduces is narrowly in-memory state (UI + the Agent Watch test-mode
// stores), so that is all this resets.
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

/** Bounded — a genuinely stuck overlay must not hang the reset forever; 5 rounds is generous headroom
 *  over the deepest nesting any spec in this suite produces (a dialog opened from within a drawer, at
 *  most two levels). */
const MAX_CLOSE_ROUNDS = 5;

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
    const closeButtons = await $$(CLOSE_BUTTON_SELECTOR);
    if ((await closeButtons.length) === 0) return;
    try {
      await closeButtons[0]!.click();
    } catch {
      // best-effort — see this function's doc comment.
    }
  }
}

/** Restores the shared session to the same starting point a fresh app launch used to provide, before
 *  the next spec file's tests run:
 *   1. Window size back to the app's real default — see the constants' comment above.
 *   2. `__CPE_TEST_CLEAR_AGENT_SESSIONS__` — a test-mode-only hook (App.svelte, CPE-1866, mirroring the
 *      existing `__CPE_TEST_INGEST_SESSION__`/`__CPE_TEST_INGEST_ACTIVITY__`/`__CPE_TEST_INGEST_COST__`
 *      convention) that wipes every synthetic Agent Watch session those hooks seed. Needed because
 *      `checkpoint-restore.smoke.ts`/`cost-history.smoke.ts`/`cost-ledger.smoke.ts`/`radar.smoke.ts`/
 *      `replay.smoke.ts` all inject a synthetic session and none of them ever tears it down — every one
 *      was written assuming (correctly, before this ticket) that the session would die with the app
 *      process at the end of its OWN spec file. Clearing `$agentSessions` also closes the drawer as a
 *      side effect, via App.svelte's own `$: if (!activeWatchCwd) showTimeline = false;` — the SAME
 *      real lifecycle path a genuine session-end event drives, not a special case invented here.
 *   3. {@link closeAnyOpenOverlay} — Escape + explicit Close-button clicks for whatever that missed.
 *   4. `navigateTo(rootDir)` — the SAME address-bar navigation primitive `samples.smoke.ts` already uses
 *      to hop between dozens of folders in one continuous session (CPE-1358), re-used here to return to
 *      the seeded tmpDir root and reconfirm via the breadcrumb that navigation actually landed, rather
 *      than assuming steps 2-3 alone left the app somewhere sane. */
export async function resetAppState(rootDir: string): Promise<void> {
  await browser.setWindowSize(DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT);
  await browser.execute(() => {
    const hook = (window as unknown as { __CPE_TEST_CLEAR_AGENT_SESSIONS__?: () => void })
      .__CPE_TEST_CLEAR_AGENT_SESSIONS__;
    hook?.();
  });
  await closeAnyOpenOverlay();
  await navigateTo(rootDir);
}
