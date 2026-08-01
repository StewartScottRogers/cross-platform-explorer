// CPE-1178 — gui-smoke pin for the native-metadata bridge (epic CPE-717), retiring its manual-verify
// debt now that the real UI has landed: CPE-1177's `nativeBridgeEnabled` Settings opt-in toggle
// (`data-testid="native-bridge-toggle"`, SettingsDialog.svelte) and CPE-1176's read-only "Native
// metadata" section in PropertiesDialog.svelte (`data-testid="native-metadata-section"`, gated on
// `nativeBridgeEnabled && single`). Drives the REAL built app end to end: opens Settings via the
// Command Palette (the same reachability path column-picker.smoke.ts uses for its dialog), flips the
// toggle on, closes Settings, selects a seeded file, opens Properties via the `Alt+Enter` shortcut
// (App.svelte's global keydown handler — `openProperties()`, the same primitive `Alt+Enter` already
// wires for a single selection), and asserts the native-metadata section actually renders.
//
// Deliberately resilient on CONTENT: this harness's seeded fixture file carries no real OS-native
// tags/label (it's a plain marker file, not one round-tripped through a native Finder-tag/NTFS-ADS/
// xattr write), so the section legitimately shows its "No tags" empty state on a real machine. The
// assertion is therefore on the SECTION + its chips container rendering — the wiring the ticket asks
// to pin — not on specific tag values, which would be a false negative on an untagged seed file.
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, $$, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");
// Seeded by wdio.conf.ts#onPrepare into the opened tmpDir (open-dir.smoke.ts's own constant). Kept as
// a literal here rather than imported across the runner/worker boundary, matching this harness's
// existing convention (see open-dir.smoke.ts / context-menu.smoke.ts's identical notes).
const MARKER_NAME = "CPE-1045-marker.txt";

describe("CPE-1178 — headless GUI smoke: the native-metadata bridge toggle + Properties section", () => {
  before(() => {
    JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir: string };
  });

  // CPE-1149: on a failing run, leave a shot of the state it failed in (`native-tags-fail.png`) — the
  // inline `snap("native-tags")` below is only reached on a pass. Non-arrow fn so Mocha binds `this`;
  // `snapFailure` is a no-op on a pass and swallows its own errors.
  afterEach(async function () {
    await snapFailure(this.currentTest, "native-tags");
  });

  it("enables the native-bridge toggle in Settings, then opens Properties for a seeded file and sees the Native metadata section render", async () => {
    // Wait for the initial `--open=<tmpDir>` navigation (also asserted in open-dir.smoke.ts) so the
    // listing (and this suite's row-selection step below) has real content to work with.
    const crumb = await $('[aria-current="page"]');
    await crumb.waitForExist({ timeout: 30_000 });

    // --- Step 1: open Settings via the Command Palette (Ctrl+Shift+P), same opener organize.smoke.ts
    // and column-picker.smoke.ts use for their own dialogs. -----------------------------------------
    await browser.keys(["Control", "Shift", "P"]);

    const paletteInput = await $(".cp-input");
    await paletteInput.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected .cp-input (Command Palette) to render after Ctrl+Shift+P",
    });
    await paletteInput.addValue("Settings");

    // Locate the "Settings…" row by scanning rendered HTML for its label rather than a `$('=text')`
    // exact-text locator — the latter relies on script-injected text matching that doesn't reliably
    // resolve against wry's webview under the classic WebDriver protocol this harness forces (see
    // column-picker.smoke.ts's identical note).
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

    // The Settings dialog mounted (SettingsDialog.svelte's `.dialog[role="dialog"][aria-label="Settings"]`).
    const settingsDialog = await $('.dialog[role="dialog"][aria-label="Settings"]');
    await settingsDialog.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected the Settings dialog to render after choosing the palette command",
    });

    // --- Step 2: the native-bridge toggle — off by default, flip it on. ------------------------------
    const toggle = await $('[data-testid="native-bridge-toggle"]');
    await toggle.waitForExist({
      timeout: 10_000,
      timeoutMsg: 'expected [data-testid="native-bridge-toggle"] to render in Settings',
    });
    expect(
      await toggle.getProperty("checked"),
      "expected the native-bridge toggle to be OFF by default (CPE-1177 opt-in)",
    ).to.equal(false);

    await toggle.click();
    await browser.waitUntil(async () => (await toggle.getProperty("checked")) === true, {
      timeout: 5_000,
      timeoutMsg: "expected the native-bridge toggle to report checked=true after clicking it",
    });

    // Close Settings (Escape — SettingsDialog.svelte's `svelte:window on:keydown` dispatches `close`).
    await browser.keys(["Escape"]);
    await settingsDialog.waitForExist({
      timeout: 10_000,
      reverse: true,
      timeoutMsg: "expected the Settings dialog to close after Escape",
    });

    // --- Step 3: select the seeded marker file and open Properties via Alt+Enter. --------------------
    // Located by scanning row HTML for the seeded filename, rather than a text-based `$('=text')`
    // locator — the same reliable primitive open-dir.smoke.ts/context-menu.smoke.ts already use against
    // wry's webview under the classic WebDriver protocol this harness forces.
    const rows = $$(".row");
    let fileRow: WebdriverIO.Element | undefined;
    for await (const row of rows) {
      const html = await row.getHTML({ includeSelectorTag: false });
      if (html.includes(MARKER_NAME)) {
        fileRow = row;
        break;
      }
    }
    expect(fileRow, `expected a file row containing "${MARKER_NAME}"`).to.not.equal(undefined);

    // A plain click (no ctrl/shift) single-selects the row, arming App.svelte's `Alt+Enter` global
    // shortcut (`openProperties()` — only acts once `selectedEntries.length > 0`).
    await fileRow!.click();
    await browser.keys(["Alt", "Enter"]);

    // --- Step 4: the Native metadata section renders (CPE-1176), gated on the toggle we just enabled
    // plus a single selection — both true at this point. ----------------------------------------------
    const nativeSection = await $('[data-testid="native-metadata-section"]');
    await nativeSection.waitForExist({
      timeout: 10_000,
      timeoutMsg: 'expected [data-testid="native-metadata-section"] to render in Properties once the native bridge is enabled',
    });

    // The chips container is present regardless of whether the seeded file carries real OS-native
    // tags (it legitimately doesn't) — asserting the SECTION's wiring, not specific tag values, per
    // this spec's own coverage note above.
    const chips = await nativeSection.$(".chips");
    expect(await chips.isExisting(), "expected a .chips container inside the native-metadata-section").to.equal(true);

    // CPE-1148 Part A: capture the Properties dialog with the Native metadata section open (after the
    // assertions above). On a FAILING run this line is never reached — the `afterEach` hook above
    // captures `native-tags-fail.png` of the failure state instead (CPE-1149).
    await snap("native-tags");

    // Non-destructive tidy end state: close Properties via Escape (PropertiesDialog.svelte's own
    // `svelte:window on:keydown` dispatches `close`); nothing was written/persisted by this spec.
    await browser.keys(["Escape"]);
  });
});
