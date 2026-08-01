// CPE-1173 — gui-smoke render pin for the Agent Watch drawer's Cost ledger tab (CPE-1098): asserts
// the `.cl-card`/`.cl-row`/`.cl-*` per-session usage snapshot actually renders once a synthetic
// `ai-console://agent-cost` payload is folded into the live `agentCost` store.
//
// Opening the drawer needs a "watched" agent session whose `cwd` matches the current folder
// (App.svelte: `activeWatchCwd = watchTargetFor($agentSessions, currentPath)` — the drawer's own
// open button, `.agent-log-btn` in ExplorerPane.svelte, only renders once that's truthy). This
// harness never launches a real agent/sidecar to announce one, so this suite seeds a SYNTHETIC
// session announcement through the test-mode-only hook `window.__CPE_TEST_INGEST_SESSION__`
// (App.svelte, CPE-1130), same as cost-history.smoke.ts. It then seeds a synthetic per-session usage
// snapshot through the sibling test-mode-only hook `window.__CPE_TEST_INGEST_COST__` (App.svelte,
// CPE-1173), which folds a `{sessionId, inputTokens, outputTokens, costUsd}` payload into the live
// `agentCost` store the exact same way a real `ai-console://agent-cost` event would
// (`agentCost.ts#ingestCost`). Using the SAME sessionId for both seeds also exercises the "watched
// session" chip (`.cl-chip`/`.cl-current`) on the rendered card.
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, $$, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");

const SESSION_ID = "gui-smoke-cpe-1173";

describe("CPE-1173 — headless GUI smoke: cost-ledger tab renders from a seeded usage snapshot", () => {
  let tmpDir = "";

  before(() => {
    // Written by wdio.conf.ts#onPrepare in the main process before this session started.
    ({ tmpDir } = JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir: string });
  });

  // CPE-1149: on a failing run, leave a shot of the state it failed in (`cost-ledger-fail.png`) —
  // the inline `snap("cost-ledger")` below is only reached on a pass. Non-arrow fn so Mocha binds
  // `this`; `snapFailure` is a no-op on a pass and swallows its own errors.
  afterEach(async function () {
    await snapFailure(this.currentTest, "cost-ledger");
  });

  it("opens the Agent Watch drawer's Cost tab and renders .cl-card / .cl-row elements", async () => {
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

    // Switch to the Cost tab. Text-matched over `.tl-tabbar .tab` rather than a `$('=text')`
    // exact-text locator — the latter relies on script-injected matching that doesn't reliably
    // resolve against wry's webview under the classic WebDriver protocol this harness forces (see
    // the same reasoning in open-dir.smoke.ts's marker-visibility test).
    const tabs = $$(".tl-tabbar .tab");
    let costTab: WebdriverIO.Element | undefined;
    for await (const tab of tabs) {
      const html = await tab.getHTML({ includeSelectorTag: false });
      if (html.includes("Cost")) {
        costTab = tab;
        break;
      }
    }
    expect(costTab, 'expected a tab in .tl-tabbar labelled "Cost"').to.not.equal(undefined);
    await costTab!.waitForClickable({ timeout: 10_000 });
    await costTab!.click();

    // Seed a synthetic per-session usage snapshot — same wire shape a real sidecar emits over the
    // `ai-console://agent-cost` Tauri event (agentCost.ts#ingestCost decodes it). Same sessionId as
    // the seeded session above, so the "watched" chip (`.cl-chip`/`.cl-current`) also renders.
    await browser.execute(
      (sessionId: string) => {
        const hook = (window as unknown as { __CPE_TEST_INGEST_COST__?: (payload: string) => void })
          .__CPE_TEST_INGEST_COST__;
        if (!hook) {
          throw new Error(
            "window.__CPE_TEST_INGEST_COST__ is missing — is the app running with --test-mode?",
          );
        }
        hook(
          JSON.stringify({
            sessionId,
            inputTokens: 12345,
            outputTokens: 6789,
            costUsd: 0.4321,
          }),
        );
      },
      SESSION_ID,
    );

    // Core assertion (CPE-1173): the ledger card actually rendered from the seeded snapshot — this is
    // the FALSIFIABLE check the ticket asks for. If the store were never populated (e.g. the hook
    // wasn't wired, or the payload shape drifted), the tab renders its empty `.cl-note` state instead
    // and NO `.cl-card` exists, so this fails loudly rather than silently passing on an empty view.
    await browser.waitUntil(async () => (await $$(".cl-card").length) > 0, {
      timeout: 15_000,
      timeoutMsg: "expected at least one .cl-card to render from the seeded usage snapshot",
    });
    const cards = await $$(".cl-card");
    expect(cards.length, "expected >=1 .cl-card from the seeded session").to.be.greaterThan(0);

    const rows = await $$(".cl-row");
    expect(rows.length, "expected >=1 .cl-row inside the ledger card").to.be.greaterThan(0);
    const labels = await $$(".cl-label");
    expect(labels.length, "expected >=1 .cl-label inside the ledger card").to.be.greaterThan(0);
    const values = await $$(".cl-value");
    expect(values.length, "expected >=1 .cl-value inside the ledger card").to.be.greaterThan(0);

    // The watched-session chip: same sessionId was seeded for both the session announcement and the
    // usage snapshot, so the card should flag itself as the currently-watched session.
    const chip = await $(".cl-chip");
    expect(await chip.isExisting(), "expected .cl-chip (\"watched\") to render for the matching session").to.equal(
      true,
    );

    // CPE-1148 Part A: capture the cost-ledger tab, after the assertions above. On a FAILING run this
    // line is never reached — the `afterEach` hook above captures `cost-ledger-fail.png` of the
    // failure state instead (CPE-1149).
    await snap("cost-ledger");
  });
});
