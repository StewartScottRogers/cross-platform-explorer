// CPE-1255 — burn down the Agent-Watch Radar tab's visual residual (CPE-1100, epic CPE-1148): drives
// the real built app to the Radar tab and asserts an activity-OVERLAP row (`.rd-item`) with two
// `.rd-pill` actor chips actually renders — the last live-IPC-fed Agent-Watch drawer tab that had no
// `gui-smoke` render pin (MANUAL-TEST-BURNDOWN.md row CPE-1100).
//
// A read-only spike (2026-08-02, this ticket) confirmed the Radar tab is headlessly seedable exactly
// like CPE-1173 (cost-ledger) / CPE-1135 (replay) / CPE-1130 (cost-history) before it: it renders
// purely from `$: overlaps = foldOverlaps(entries)` (agentConflicts.ts) over the LIVE
// `agentTimeline` store — no new listener/timer, no on-disk fixture. `foldOverlaps` folds an
// "overlap" whenever a single path is touched by >=2 DISTINCT `actor` values within
// `OVERLAP_WINDOW_MS` (5000ms).
//
// Opening the drawer needs a "watched" agent session whose `cwd` matches the current folder, exactly
// like cost-ledger.smoke.ts / replay.smoke.ts (see those files' header comments for the full
// rationale) — seeded here via the SAME `window.__CPE_TEST_INGEST_SESSION__` test-mode hook.
//
// The overlap itself is seeded via the EXISTING `window.__CPE_TEST_INGEST_ACTIVITY__` hook
// (App.svelte, CPE-1135) — no new hook needed for this ticket, unlike CPE-1173's cost seam. Two
// SEPARATE calls (not one two-item batch) for the SAME path, each with a distinct `actor` and an
// explicit `at` within the 5s overlap window, so the entries land as genuinely distinct, ordered
// touches by two different actors rather than racing the same millisecond or reusing one shared
// `now`. The path itself is never actually written to disk — the Radar tab reads only from the live
// timeline store, not the real filesystem, so a synthetic path under tmpDir is enough (same reasoning
// replay.smoke.ts documents for its own seeded entries feeding the LIVE store vs. the separate
// on-disk journal fixture it also seeds).
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, $$, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");

const SESSION_ID = "gui-smoke-cpe-1255";
const ACTOR_A = "gui-smoke-agent-a";
const ACTOR_B = "gui-smoke-agent-b";
const SHARED_FILE_NAME = "CPE-1255-shared.txt";

describe("CPE-1255 — headless GUI smoke: Radar tab renders a two-actor overlap", () => {
  let tmpDir = "";

  before(() => {
    // Written by wdio.conf.ts#onPrepare in the main process before this session started.
    ({ tmpDir } = JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir: string });
  });

  // CPE-1149: on a failing run, leave a shot of the state it failed in (`radar-fail.png`) — the
  // inline `snap("radar")` below is only reached on a pass. Non-arrow fn so Mocha binds `this`;
  // `snapFailure` is a no-op on a pass and swallows its own errors.
  afterEach(async function () {
    await snapFailure(this.currentTest, "radar");
  });

  it("opens the Agent Watch drawer's Radar tab and renders a two-actor overlap row", async () => {
    // Wait for the initial `--open=<tmpDir>` navigation (also asserted in open-dir.smoke.ts) so
    // `currentPath` is settled before seeding a session anchored to it.
    const crumb = await $('[aria-current="page"]');
    await crumb.waitForExist({ timeout: 30_000 });

    // Seed the synthetic "started" session announcement — same wire shape a real sidecar emits over
    // the `ai-console://session` Tauri event (agentSessions.ts#ingestSessionState decodes it).
    await browser.execute(
      (cwd: string, sessionId: string) => {
        const hook = (window as unknown as { __CPE_TEST_INGEST_SESSION__?: (s: string) => void })
          .__CPE_TEST_INGEST_SESSION__;
        if (!hook) {
          throw new Error(
            "window.__CPE_TEST_INGEST_SESSION__ is missing — is the app running with --test-mode?",
          );
        }
        hook(
          `session:${JSON.stringify({
            event: "started",
            sessionId,
            agentId: "claude",
            agentName: "Claude Code",
            provider: "openrouter",
            model: "sonnet",
            cwd,
          })}`,
        );
      },
      tmpDir,
      SESSION_ID,
    );

    // `.agent-log-btn` (ExplorerPane.svelte) only renders once `activeWatchCwd` is truthy — i.e.
    // once the seeded session above is folded in and its cwd matches the current folder.
    const openBtn = await $(".agent-log-btn");
    await openBtn.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected .agent-log-btn to appear once a watched session is active",
    });
    await openBtn.waitForClickable({ timeout: 10_000 });
    await openBtn.click();

    // The drawer mounts (and its tab strip renders) a beat after the click above resolves — wait for
    // the real DOM condition (all 5 tabs present) rather than assuming it's already there, the same
    // "wait on a real condition, not a sleep" rule the rest of this harness follows.
    await browser.waitUntil(async () => (await $$(".tl-tabbar .tab").length) === 5, {
      timeout: 10_000,
      timeoutMsg: "expected the Agent Watch drawer's 5-tab strip to render",
    });

    // Switch to the Radar tab. Text-matched over `.tl-tabbar .tab` rather than a `$('=text')`
    // exact-text locator — the latter relies on script-injected matching that doesn't reliably
    // resolve against wry's webview under the classic WebDriver protocol this harness forces (same
    // reasoning as cost-ledger.smoke.ts / replay.smoke.ts).
    const tabs = $$(".tl-tabbar .tab");
    let radarTab: WebdriverIO.Element | undefined;
    for await (const tab of tabs) {
      const html = await tab.getHTML({ includeSelectorTag: false });
      if (html.includes("Radar")) {
        radarTab = tab;
        break;
      }
    }
    expect(radarTab, 'expected a tab in .tl-tabbar labelled "Radar"').to.not.equal(undefined);
    await radarTab!.waitForClickable({ timeout: 10_000 });
    await radarTab!.click();

    // Seed two LIVE timeline entries for the SAME path, two DISTINCT actors, via the existing
    // `__CPE_TEST_INGEST_ACTIVITY__` hook (App.svelte, CPE-1135) — the same wire shape a real
    // `ai-console://fs-activity` batch decodes to via `agentActivity.ts#ingestActivity` /
    // `sidecar.ts#normalizeFsActivity`. Two separate calls (not one two-item batch) with explicit,
    // distinct `at` timestamps 200ms apart — well inside `OVERLAP_WINDOW_MS` (5000ms,
    // agentConflicts.ts) — so `foldOverlaps` sees two genuinely ordered, distinct-actor touches of
    // one path rather than two entries racing the same batch-wide `now`.
    const sharedPath = path.join(tmpDir, SHARED_FILE_NAME);
    const startedAt = Date.now();
    for (const [actor, at] of [
      [ACTOR_A, startedAt],
      [ACTOR_B, startedAt + 200],
    ] as const) {
      await browser.execute(
        (payload: string, ts: number) => {
          const hook = (
            window as unknown as { __CPE_TEST_INGEST_ACTIVITY__?: (p: string, at?: number) => void }
          ).__CPE_TEST_INGEST_ACTIVITY__;
          if (!hook) {
            throw new Error(
              "window.__CPE_TEST_INGEST_ACTIVITY__ is missing — is the app running with --test-mode?",
            );
          }
          hook(payload, ts);
        },
        JSON.stringify([{ kind: "modified", path: sharedPath, actor }]),
        at,
      );
    }

    // Core assertion (CPE-1255): the radar actually folded the two-actor overlap and rendered it —
    // the FALSIFIABLE check this ticket asks for. If the seeded entries above were missing/malformed,
    // shared the same actor, or landed outside the overlap window (or the `__CPE_TEST_INGEST_ACTIVITY__`
    // seam broke), `foldOverlaps` stays empty and the tab shows its "No overlapping activity" empty
    // state instead — NO `.rd-list`/`.rd-item` exists, so this fails loudly rather than silently
    // passing on an empty view.
    await browser.waitUntil(async () => (await $(".rd-list")).isExisting(), {
      timeout: 15_000,
      timeoutMsg: "expected .rd-list to render from the seeded two-actor overlap",
    });
    const list = await $(".rd-list");
    expect(await list.isExisting(), "expected .rd-list to render").to.equal(true);

    const items = await $$(".rd-item");
    expect(items.length, "expected >=1 .rd-item in the radar list").to.be.greaterThan(0);

    // The seeded overlap is the only activity in this run, so its `.rd-item` is the whole list — find
    // the one for our shared path (matched by its title attribute, the full path) and assert its
    // `.rd-actors` shows exactly two `.rd-pill` chips, one per seeded actor.
    let overlapItem: WebdriverIO.Element | undefined;
    for await (const item of items) {
      const html = await item.getHTML({ includeSelectorTag: false });
      if (html.includes(SHARED_FILE_NAME)) {
        overlapItem = item;
        break;
      }
    }
    expect(overlapItem, `expected a .rd-item for ${SHARED_FILE_NAME}`).to.not.equal(undefined);

    const pills = await overlapItem!.$$(".rd-actors .rd-pill");
    expect(pills.length, "expected exactly two .rd-pill actor chips").to.equal(2);

    // CPE-1148 Part A: capture the Radar tab, after the assertions above. On a FAILING run this line
    // is never reached — the `afterEach` hook above captures `radar-fail.png` of the failure state
    // instead (CPE-1149).
    await snap("radar");
  });
});
