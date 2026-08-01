// CPE-1199 (epic CPE-735) — render pin for the "Scheduled snapshots" Settings section (the opt-in
// configuration surface for the background snapshot scheduler). Drives the REAL built app, opens
// Settings via the Command Palette (`app.settings` in App.svelte's `paletteCommands` — the same
// keyboard-first opener a real user has), and asserts the Scheduled snapshots section renders with its
// empty-state + the add-a-folder builder (path field, Browse picker, interval, Add). Captures the
// section as a screenshot for the Visual Critic (CPE-1148).
//
// Non-destructive: the spec only opens Settings and reads the section — it never adds a rule, so no
// schedule is written and the background timer stays off (off means off).
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, $$, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");

describe("CPE-1199 — headless GUI smoke: the Scheduled snapshots Settings section renders", () => {
  before(() => {
    JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir: string };
  });

  // CPE-1149: on a failing run leave a shot of the failure state; the inline snap below is only reached
  // on a pass. Non-arrow fn so Mocha binds `this`.
  afterEach(async function () {
    await snapFailure(this.currentTest, "snapshot-schedule-settings");
  });

  it("opens Settings via the command palette and shows the Scheduled snapshots section", async () => {
    // Wait for the initial `--open=<tmpDir>` navigation (also asserted in open-dir.smoke.ts) so the app
    // is fully settled before reaching for the palette.
    const crumb = await $('[aria-current="page"]');
    await crumb.waitForExist({ timeout: 30_000 });
    await crumb.click(); // plain <span> — just moves focus off any input

    // Ctrl+Shift+P opens the Command Palette (App.svelte's `handleKeydown`).
    await browser.keys(["Control", "Shift", "P"]);
    const paletteInput = await $(".cp-input");
    await paletteInput.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected .cp-input (Command Palette) to render after Ctrl+Shift+P",
    });

    // "settings" filters to the `app.settings` command ("Settings…").
    await paletteInput.addValue("settings");

    // Locate the Settings row by scanning rendered HTML for its label (the reliable locator under wry's
    // webview — see column-picker.smoke.ts's identical note).
    let settingsRow: WebdriverIO.Element | undefined;
    await browser.waitUntil(
      async () => {
        const rows = $$(".cp-row");
        for await (const row of rows) {
          const html = await row.getHTML({ includeSelectorTag: false });
          if (html.includes("Settings")) {
            settingsRow = row;
            return true;
          }
        }
        return false;
      },
      { timeout: 10_000, timeoutMsg: 'expected a .cp-row labelled "Settings…" to appear' },
    );
    expect(settingsRow, 'expected a .cp-row labelled "Settings…"').to.not.equal(undefined);
    await settingsRow!.waitForClickable({ timeout: 10_000 });
    await settingsRow!.click();

    // The Settings dialog mounted.
    const dialog = await $('.dialog[role="dialog"]');
    await dialog.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected the Settings dialog to render after choosing the palette command",
    });

    // The Scheduled snapshots section rendered. With no rules configured (the seeded app carries none)
    // the empty-state + the add-a-folder builder must both be present — proving the section's
    // `snapshot_schedule_list` invoke resolved (an empty list, off-means-off) rather than erroring.
    const section = await $('[data-testid="scheduled-snapshots"]');
    await section.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected the [data-testid='scheduled-snapshots'] section to render in Settings",
    });
    expect(await section.isExisting(), "expected the Scheduled snapshots section to render").to.equal(true);

    const empty = await $('[data-testid="schedule-empty"]');
    await empty.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected the empty-state (no scheduled folders) to render — off means off",
    });
    expect(await empty.isExisting(), "expected the no-folders empty state").to.equal(true);

    // The add-a-folder builder (path field + Browse picker + Add) is the opt-in surface.
    expect(await (await $('[data-testid="schedule-path"]')).isExisting(), "expected the path field").to.equal(true);
    expect(await (await $('[data-testid="schedule-browse"]')).isExisting(), "expected the Browse picker").to.equal(true);
    expect(await (await $('[data-testid="schedule-add-btn"]')).isExisting(), "expected the Add button").to.equal(true);

    // Capture the section for the Visual Critic (after the assertions above).
    await snap("snapshot-schedule-settings");
  });
});
