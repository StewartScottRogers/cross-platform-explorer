// CPE-1130 — burn down the cost-History visual residual (CPE-1114, epic CPE-731 slice c — final
// slice): drives the real built app to the Agent Watch drawer's History tab and asserts the
// `.hd-bar`/`.hd-*` rollup actually renders from the `history.jsonl` fixture seeded by
// wdio.conf.ts#seedHistoryFixture.
//
// Opening the drawer needs a "watched" agent session whose `cwd` matches the current folder
// (App.svelte: `activeWatchCwd = watchTargetFor($agentSessions, currentPath)` — the drawer's own
// open button, `.agent-log-btn` in ExplorerPane.svelte, only renders once that's truthy). This
// harness never launches a real agent/sidecar to announce one, so this suite seeds a SYNTHETIC
// session announcement through a test-mode-only hook, `window.__CPE_TEST_INGEST_SESSION__`
// (App.svelte, gated behind the existing `--test-mode` / `window.__CPE_TEST_MODE__` global — mirrors
// the `__CPE_OPEN_DIR__` convention this harness already relies on; zero cost/attack-surface outside
// `--test-mode`). The seeded session's `cwd` is the SAME tmpDir `--open=<tmpDir>` already navigated
// into (open-dir.smoke.ts asserts that navigation), so `watchTargetFor` matches it immediately with
// no further navigation. The History tab's own DATA — what actually renders — is unrelated to this
// synthetic session: it comes from the real `commands.metricsHistory()` IPC call reading the seeded
// `history.jsonl` journal file straight off disk.
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, $$, browser } from "@wdio/globals";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");

describe("CPE-1130 — headless GUI smoke: cost-History renders from a seeded journal", () => {
  let tmpDir = "";

  before(() => {
    // Written by wdio.conf.ts#onPrepare in the main process before this session started.
    ({ tmpDir } = JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir: string });
  });

  it("opens the Agent Watch drawer's History tab and renders .hd-bar / .hd-* elements", async () => {
    // Wait for the initial `--open=<tmpDir>` navigation (also asserted in open-dir.smoke.ts) so
    // `currentPath` is settled before seeding a session anchored to it.
    const crumb = await $('[aria-current="page"]');
    await crumb.waitForExist({ timeout: 30_000 });

    // Seed the synthetic "started" session announcement — same wire shape a real sidecar emits over
    // the `ai-console://session` Tauri event (agentSessions.ts#ingestSessionState decodes it).
    await browser.execute((cwd: string) => {
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
          sessionId: "gui-smoke-cpe-1130",
          agentId: "claude",
          agentName: "Claude Code",
          provider: "openrouter",
          model: "sonnet",
          cwd,
        })}`,
      );
    }, tmpDir);

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

    // Switch to the History tab. Text-matched over `.tl-tabbar .tab` rather than a `$('=text')`
    // exact-text locator — the latter relies on script-injected matching that doesn't reliably
    // resolve against wry's webview under the classic WebDriver protocol this harness forces (see
    // the same reasoning in open-dir.smoke.ts's marker-visibility test).
    const tabs = $$(".tl-tabbar .tab");
    let historyTab: WebdriverIO.Element | undefined;
    for await (const tab of tabs) {
      const html = await tab.getHTML({ includeSelectorTag: false });
      if (html.includes("History")) {
        historyTab = tab;
        break;
      }
    }
    expect(historyTab, 'expected a tab in .tl-tabbar labelled "History"').to.not.equal(undefined);
    // CPE-1130 (bug found + fixed via this exact assertion — see AgentTimeline.svelte's
    // `.tl-tabbar`/`.tl-tabbar .tab` CSS): at the app's own default 1000x700 launch size, the 5-tab
    // strip's base `.tab` class (120px min-width, sized for the wide main-window tabbar) never fit
    // the drawer's 340px width — the last ("History") tab silently overflowed past the document edge
    // and was permanently unclickable. `waitForClickable` (rather than a bare `.click()`) is kept
    // here as defence-in-depth for a genuine render/paint beat, now that the underlying overflow bug
    // is fixed.
    await historyTab!.waitForClickable({ timeout: 10_000 });
    await historyTab!.click();

    // Core assertion (CPE-1130): the rollup's over-time bars actually rendered — this is the
    // FALSIFIABLE check the ticket asks for. If `metricsHistory()` ever returned `[]` (e.g. the
    // fixture wasn't seeded, or the file/schema drifted), the tab renders its empty state instead
    // and NO `.hd-bar` exists, so this fails loudly rather than silently passing on an empty view.
    // (`$$(selector).length` is awaited directly — WebdriverIO's `ChainablePromiseArray.length` is
    // itself a lazily-resolved `Promise<number>` getter, the intended way to check a count without
    // first materializing the whole element array.)
    await browser.waitUntil(async () => (await $$(".hd-bar").length) > 0, {
      timeout: 15_000,
      timeoutMsg: "expected at least one .hd-bar to render from the seeded history.jsonl fixture",
    });
    const bars = await $$(".hd-bar");
    expect(bars.length, "expected >=1 .hd-bar rect from the 3-row seeded fixture").to.be.greaterThan(0);

    // The wider `.hd-*` rollup surface, not just the chart — the totals strip and its stat tiles.
    const totals = await $(".hd-totals");
    expect(await totals.isExisting(), "expected .hd-totals to render").to.equal(true);
    const stats = await $$(".hd-stat");
    expect(stats.length, "expected >=1 .hd-stat tile in .hd-totals").to.be.greaterThan(0);

    // And the per-model breakdown table (the fixture spans two models) — a second, independent
    // `.hd-*` surface fed by the same rollup.
    const modelRows = await $$(".hd-table tbody tr");
    expect(modelRows.length, "expected >=1 row in a .hd-table (by-model or by-agent)").to.be.greaterThan(0);
  });
});
