// CPE-1315 — headless GUI smoke for the File Health panel's Dangling-links tab (FileHealthDialog.svelte,
// epic CPE-1002): drives the real built app, opens the dialog via its real opener — the Command Palette
// (Ctrl+Shift+P → "Find dangling links…", the same `tool.findDanglingLinks` command the Tools ▸ menu
// item is wired to, see App.svelte's `paletteCommands`) — scans the seeded tmpDir, and asserts the
// permanently-broken symlink (already seeded for link-badge.smoke.ts, CPE-1208) renders as a dangling
// link with the "Missing target" reason badge.
//
// This spec is a RENDER-SPEC SKETCH (per the ticket's own instruction — the Foreman verifies the real
// streamed-Channel path end to end against a live build; jsdom in FileHealthDialog.test.ts already covers
// the streaming/cancel/supersede logic in isolation with a mocked Channel). It's written to actually run
// (reuses a real, already-seeded fixture — see below), it just isn't executed as part of this ticket.
//
// Reuses `seedLinkBadgeFixture` (wdio.conf.ts) rather than adding a new fixture: that seed already lands
// one intact + one PERMANENTLY broken symlink (LINK_BROKEN_NAME -> a name that's never created) in the
// same tmpDir every other dialog spec shares. Symlink creation needs Developer Mode/elevation on
// Windows, so — exactly like link-badge.smoke.ts — this spec reads `linkBadgeFixture.supported` from
// STATE_FILE and skips its assertions on an unprivileged runner rather than failing the whole suite over
// a sandbox permission gap.
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, $$, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");

// Kept as a literal (not imported from wdio.conf.ts) to match this harness's existing convention of
// duplicating seeded filenames rather than reaching across the runner/worker boundary (see
// link-badge.smoke.ts's identical note on LINK_BROKEN_NAME).
const LINK_BROKEN_NAME = "CPE-1208-broken-link.txt";

/** The `[data-testid="fh-row"]` whose rendered HTML contains `name` — scans HTML rather than an exact-
 *  text locator, same reasoning as near-duplicates.smoke.ts / similar-images.smoke.ts. */
async function findRowContaining(name: string): Promise<WebdriverIO.Element> {
  let found: WebdriverIO.Element | undefined;
  await browser.waitUntil(
    async () => {
      const rows = $$('[data-testid="fh-row"]');
      for await (const row of rows) {
        const html = await row.getHTML({ includeSelectorTag: false });
        if (html.includes(name)) {
          found = row;
          return true;
        }
      }
      return false;
    },
    { timeout: 20_000, timeoutMsg: `expected an fh-row containing the seeded ${name}` },
  );
  return found!;
}

describe("CPE-1315 — headless GUI smoke: File Health dialog streams the seeded broken symlink", () => {
  let supported = false;

  before(() => {
    const state = JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as {
      linkBadgeFixture?: { supported: boolean };
    };
    supported = state.linkBadgeFixture?.supported ?? false;
    if (!supported) {
      // eslint-disable-next-line no-console
      console.warn(
        "[file-health.smoke] symlink creation was unprivileged on this runner — skipping CPE-1315 assertions",
      );
    }
  });

  afterEach(async function () {
    await snapFailure(this.currentTest, "file-health");
  });

  it("opens via the command palette, scans, and streams the seeded broken symlink as a dangling link", async function () {
    if (!supported) return this.skip();

    // Wait for the initial `--open=<tmpDir>` navigation so `currentPath` is the seeded tmpDir before
    // reaching for a folder-scoped command — `tool.findDanglingLinks` is `enabled: inFolder`.
    const crumb = await $('[aria-current="page"]');
    await crumb.waitForExist({ timeout: 30_000 });
    await crumb.click(); // plain, non-interactive <span> — just moves focus off any input

    // Ctrl+Shift+P opens the Command Palette (App.svelte's `handleKeydown`).
    await browser.keys(["Control", "Shift", "P"]);
    const paletteInput = await $(".cp-input");
    await paletteInput.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected .cp-input (Command Palette) to render after Ctrl+Shift+P",
    });
    await paletteInput.addValue("dangling links");

    let row: WebdriverIO.Element | undefined;
    await browser.waitUntil(
      async () => {
        const rows = $$(".cp-row");
        for await (const r of rows) {
          const html = await r.getHTML({ includeSelectorTag: false });
          if (html.includes("Find dangling links")) {
            row = r;
            return true;
          }
        }
        return false;
      },
      { timeout: 10_000, timeoutMsg: 'expected a .cp-row labelled "Find dangling links…" to appear' },
    );
    expect(row, 'expected a .cp-row labelled "Find dangling links…"').to.not.equal(undefined);
    await row!.waitForClickable({ timeout: 10_000 });
    await row!.click();

    // The dialog mounted (aria-label is the localized title; default locale is English) with its
    // Dangling-links tab already active.
    const dialog = await $('[aria-label="File health"]');
    await dialog.waitForExist({ timeout: 10_000, timeoutMsg: "expected the File Health dialog to render" });
    const tab = await $('[data-testid="fh-tab-dangling"]');
    await tab.waitForExist({ timeout: 5_000, timeoutMsg: "expected the Dangling links tab to render" });

    // Kick the scan via its real button — this is the REAL `find_dangling_links_stream` command over a
    // real Tauri ipc::Channel, the first GUI exercise of that streaming path (jsdom mocks it elsewhere).
    const scanBtn = await $('[data-testid="fh-scan-btn"]');
    await scanBtn.waitForClickable({ timeout: 10_000 });
    await scanBtn.click();

    // Core assertion (CPE-1315): the seeded permanently-broken symlink streams back as a dangling link
    // with the "Missing target" reason — the FALSIFIABLE check tied to this spec's own fixture. If the
    // scan returned nothing (broken invoke, bad path/excludes wiring, or a classifier regression)
    // `[data-testid="fh-none"]` would render and no fh-row would exist, so this fails loudly rather than
    // passing on an empty view.
    const brokenRow = await findRowContaining(LINK_BROKEN_NAME);
    const badge = await brokenRow.$('[data-testid="fh-reason"]');
    await badge.waitForExist({ timeout: 5_000, timeoutMsg: "expected the row to show a reason badge" });
    expect(await badge.getText()).to.equal("Missing target");

    // CPE-1148/1149: capture the passing state (the streamed dangling-link row) for the Visual Critic.
    await snap("file-health");

    // Read-only dialog (no delete/repair action here) — dismiss via the close button.
    const closeBtn = await $('[data-testid="fh-close-btn"]');
    await closeBtn.click();
  });
});
