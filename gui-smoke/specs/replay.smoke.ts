// CPE-1135 — burn down the Replay-scrubber visual residual (CPE-1094, epic CPE-728): drives the real
// built app to the Agent Watch drawer's Replay tab and asserts the scrubber transport/slider
// (`.rp-transport`/`.rp-slider`) actually render, and that the reconstructed folder listing
// (`.rp-recon`) loaded from the on-disk journal+baseline fixture seeded by
// wdio.conf.ts#seedReplayFixture — non-error, not just a silently-empty tab.
//
// Opening the drawer needs a "watched" agent session whose `cwd` matches the current folder, exactly
// like cost-history.smoke.ts (CPE-1130) — see that file's header comment for the full rationale. This
// suite reuses the same `window.__CPE_TEST_INGEST_SESSION__` hook to seed a SYNTHETIC session
// announcement, with `cwd` set to the SAME tmpDir `--open=<tmpDir>` already navigated into
// (open-dir.smoke.ts asserts that navigation).
//
// The Replay tab's transport/slider render is gated on `sliderRange(entries)` being non-null
// (agentReplay.ts), which needs >=2 entries in the LIVE `agentTimeline` store — a store fed only by
// real `ai-console://fs-activity` events in production. This harness never watches a real folder, so
// there is no other way to populate it; CPE-1135 adds the minimum test-mode seam,
// `window.__CPE_TEST_INGEST_ACTIVITY__` (App.svelte, gated behind the same `--test-mode` /
// `window.__CPE_TEST_MODE__` global as every other test hook here — zero cost/attack-surface outside
// `--test-mode`), to seed that store directly. This is INDEPENDENT of the Replay tab's *reconstructed
// listing*, which instead comes from the real `replay_load` IPC call reading the on-disk
// journal+baseline fixture straight off disk (seeded by wdio.conf.ts#seedReplayFixture) — so this spec
// exercises both seams together, mirroring how a real watched-then-replayed session combines a live
// timeline with a durable journal.
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, $$, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");

// Duplicated literals rather than importing from wdio.conf.ts — matches this harness's existing
// convention (see open-dir.smoke.ts's FIXTURE_NAME comment) of not reaching across the
// runner/worker process boundary.
const REPLAY_SESSION_ID = "gui-smoke-replay";
const REPLAY_CREATED_NAME = "CPE-1135-created.txt";

describe("CPE-1135 — headless GUI smoke: Replay scrubber renders from a seeded journal", () => {
  let tmpDir = "";

  before(() => {
    // Written by wdio.conf.ts#onPrepare in the main process before this session started.
    ({ tmpDir } = JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir: string });
  });

  // CPE-1149: on a failing run, leave a shot of the state it failed in (`replay-tab-fail.png`) — the
  // inline `snap("replay-tab")` below is only reached on a pass. Non-arrow fn so Mocha binds `this`;
  // `snapFailure` is a no-op on a pass and swallows its own errors.
  afterEach(async function () {
    await snapFailure(this.currentTest, "replay-tab");
  });

  it("opens the Agent Watch drawer's Replay tab and renders the scrubber + reconstruction", async () => {
    // Wait for the initial `--open=<tmpDir>` navigation (also asserted in open-dir.smoke.ts) so
    // `currentPath` is settled before seeding a session anchored to it.
    const crumb = await $('[aria-current="page"]');
    await crumb.waitForExist({ timeout: 30_000 });

    // Seed the synthetic "started" session announcement — same wire shape a real sidecar emits over
    // the `ai-console://session` Tauri event (agentSessions.ts#ingestSessionState decodes it). Same
    // session id the on-disk journal/baseline fixture was written under (seedReplayFixture).
    await browser.execute((cwd: string, sessionId: string) => {
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
    }, tmpDir, REPLAY_SESSION_ID);

    // Seed two LIVE timeline entries (CPE-1135's new `__CPE_TEST_INGEST_ACTIVITY__` hook) — the same
    // path + timestamps the on-disk journal fixture uses, 30s apart, so `sliderRange(entries)`
    // (agentReplay.ts) is a non-null, non-degenerate range and the transport/slider actually render.
    // Two separate calls (not one two-item batch) so the entries land distinct `at` timestamps rather
    // than sharing one batch-wide `now`.
    const createdPath = path.join(tmpDir, REPLAY_CREATED_NAME);
    const startedAt = Date.now() - 60_000;
    for (const [kind, at] of [
      ["created", startedAt],
      ["modified", startedAt + 30_000],
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
        JSON.stringify([{ kind, path: createdPath, actor: REPLAY_SESSION_ID }]),
        at,
      );
    }

    // `.agent-log-btn` (ExplorerPane.svelte) only renders once `activeWatchCwd` is truthy — i.e. once
    // the seeded session above is folded in and its cwd matches the current folder.
    const openBtn = await $(".agent-log-btn");
    await openBtn.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected .agent-log-btn to appear once a watched session is active",
    });
    await openBtn.waitForClickable({ timeout: 10_000 });
    await openBtn.click();

    // The drawer mounts (and its tab strip renders) a beat after the click above resolves — wait for
    // the real DOM condition (all 5 tabs present) rather than assuming it's already there (same rule
    // cost-history.smoke.ts follows).
    await browser.waitUntil(async () => (await $$(".tl-tabbar .tab").length) === 5, {
      timeout: 10_000,
      timeoutMsg: "expected the Agent Watch drawer's 5-tab strip to render",
    });

    // Switch to the Replay tab. Text-matched over `.tl-tabbar .tab` rather than a `$('=text')`
    // exact-text locator — see cost-history.smoke.ts's identical comment for why (wry/classic
    // WebDriver doesn't reliably resolve script-injected text matching here).
    const tabs = $$(".tl-tabbar .tab");
    let replayTab: WebdriverIO.Element | undefined;
    for await (const tab of tabs) {
      const html = await tab.getHTML({ includeSelectorTag: false });
      if (html.includes("Replay")) {
        replayTab = tab;
        break;
      }
    }
    expect(replayTab, 'expected a tab in .tl-tabbar labelled "Replay"').to.not.equal(undefined);
    await replayTab!.waitForClickable({ timeout: 10_000 });
    await replayTab!.click();

    // Core assertion (CPE-1135): the scrubber transport actually rendered — the FALSIFIABLE check the
    // ticket asks for. If the seeded live-timeline entries above were missing/malformed (or the
    // `__CPE_TEST_INGEST_ACTIVITY__` seam broke), `sliderRange(entries)` stays null and the tab shows
    // its "not enough activity" empty state instead — NO `.rp-transport` exists, so this fails loudly
    // rather than silently passing on an empty view.
    await browser.waitUntil(async () => (await $(".rp-transport")).isExisting(), {
      timeout: 15_000,
      timeoutMsg: "expected .rp-transport to render from the seeded live timeline entries",
    });
    const transport = await $(".rp-transport");
    expect(await transport.isExisting(), "expected .rp-transport to render").to.equal(true);
    const btns = await $$(".rp-transport .rp-btn");
    expect(btns.length, "expected >=1 .rp-btn in the transport").to.be.greaterThan(0);

    // The slider itself (`<input type="range">`) — not disabled, since the two seeded entries carry
    // distinct timestamps (a non-degenerate range).
    const slider = await $(".rp-slider");
    expect(await slider.isExisting(), "expected .rp-slider to render").to.equal(true);
    expect(await slider.getTagName(), "expected .rp-slider to be an <input>").to.equal("input");
    expect(await slider.isEnabled(), "expected .rp-slider to be enabled for a non-degenerate range").to.equal(true);

    // The reconstructed-listing surface (CPE-1111/CPE-1110): renders once `replay_load` resolves
    // against the on-disk journal+baseline fixture (wdio.conf.ts#seedReplayFixture). This is the
    // second, INDEPENDENT falsifiable check: if the fixture's on-disk shape ever drifted from what
    // `replay_session::load_replay`/`replay_baseline::read_baseline` actually read, `replayData` would
    // stay null forever and `.rp-recon` would never appear (the tab falls back silently to the frozen
    // event list per AgentTimeline.svelte's own comment — it would NOT show an error string).
    await browser.waitUntil(async () => (await $(".rp-recon")).isExisting(), {
      timeout: 15_000,
      timeoutMsg: "expected .rp-recon to render once replay_load resolves the seeded journal fixture",
    });

    // Stronger than just "the container rendered": the seeded fixture's baseline + created/modified
    // events are rooted under this exact tmpDir, so the reconstruction at the current (latest) scrub
    // moment must actually list the created file by name — not just show an empty-folder state.
    const reconList = await $(".rp-recon-list");
    await reconList.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected .rp-recon-list to render a non-empty reconstruction from the seeded fixture",
    });
    const reconHtml = await reconList.getHTML({ includeSelectorTag: false });
    expect(reconHtml).to.include(REPLAY_CREATED_NAME);

    // CPE-1148 Part A: capture the Replay tab (transport/slider + reconstruction), after the
    // assertions above. On a FAILING run this line is never reached — the `afterEach` hook above
    // captures `replay-tab-fail.png` of the failure state instead (CPE-1149).
    await snap("replay-tab");
  });
});
