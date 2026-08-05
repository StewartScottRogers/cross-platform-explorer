// CPE-1329 — headless GUI smoke for the Declutter dialog (DeclutterDialog.svelte, epic CPE-979): drives
// the real built app, opens the dialog via its real opener — the Command Palette (Ctrl+Shift+P →
// "Declutter…", the same `tool.findClutter` command the Tools ▸ menu item is wired to, see App.svelte's
// `paletteCommands`) — scans the seeded fixture folder, and asserts each of the four seeded findings
// (one per ClutterReason) renders under its human-labelled reason group. This is the workshift QA proof
// that `organize_clutter` actually delivers findings over a REAL IPC round trip against the real built
// binary — DeclutterDialog.test.ts already covers the grouping/selection/checkpoint-then-trash logic in
// isolation with a mocked invoke, but never against a real backend process.
//
// Fixture (wdio.conf.ts#seedDeclutterFixture): a zero-byte file, a fake installer (`.exe`), a partial
// download (`.part`), and a backup leftover (`.bak`) — one per `ClutterReason` variant in
// `crates/server/src/organize.rs`.
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, $$, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");

// Duplicated literals rather than importing from wdio.conf.ts — matches this harness's existing
// convention (see file-health.smoke.ts / near-duplicates.smoke.ts's identical notes).
const DECLUTTER_ZERO_BYTE_NAME = "declutter-empty.log";
const DECLUTTER_INSTALLER_NAME = "declutter-setup.exe";
const DECLUTTER_TEMP_NAME = "declutter-movie.mp4.part";
const DECLUTTER_BACKUP_NAME = "declutter-notes.txt.bak";

/** The `[data-testid="dc-row"]` whose rendered HTML contains `name` — scans HTML rather than an exact-
 *  text locator, same reasoning as file-health.smoke.ts's `findRowContaining`. */
async function findRowContaining(name: string): Promise<WebdriverIO.Element> {
  let found: WebdriverIO.Element | undefined;
  await browser.waitUntil(
    async () => {
      const rows = $$('[data-testid="dc-row"]');
      for await (const row of rows) {
        const html = await row.getHTML({ includeSelectorTag: false });
        if (html.includes(name)) {
          found = row;
          return true;
        }
      }
      return false;
    },
    { timeout: 20_000, timeoutMsg: `expected a dc-row containing the seeded ${name}` },
  );
  return found!;
}

describe("CPE-1329 — headless GUI smoke: Declutter dialog surfaces real organize_clutter findings", () => {
  before(() => {
    // Confirms the shared tmpDir was actually seeded before this spec runs (fails loudly rather than
    // silently scanning an empty/missing folder if onPrepare's seeding ever regresses).
    const state = JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir?: string };
    if (!state.tmpDir) throw new Error("expected STATE_FILE to carry the seeded tmpDir");
  });

  afterEach(async function () {
    await snapFailure(this.currentTest, "declutter");
  });

  it("opens via the command palette, navigates into the fixture folder, and scans real findings", async function () {
    // Wait for the initial `--open=<tmpDir>` navigation so `currentPath` is the seeded tmpDir before
    // reaching for a folder-scoped command — `tool.findClutter` is `enabled: inFolder`.
    const crumb = await $('[aria-current="page"]');
    await crumb.waitForExist({ timeout: 30_000 });
    await crumb.click(); // plain, non-interactive <span> — just moves focus off any input

    // Navigate into the seeded declutter-gui-verify subfolder (Declutter scans the CURRENT folder, not
    // recursively) by double-clicking its row in the file list.
    const folderRow = await $("//*[contains(text(),'declutter-gui-verify')]");
    await folderRow.waitForExist({ timeout: 15_000, timeoutMsg: "expected the seeded declutter fixture folder to list" });
    await folderRow.doubleClick();

    // Ctrl+Shift+P opens the Command Palette (App.svelte's `handleKeydown`).
    await browser.keys(["Control", "Shift", "P"]);
    const paletteInput = await $(".cp-input");
    await paletteInput.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected .cp-input (Command Palette) to render after Ctrl+Shift+P",
    });
    await paletteInput.addValue("declutter");

    let row: WebdriverIO.Element | undefined;
    await browser.waitUntil(
      async () => {
        const rows = $$(".cp-row");
        for await (const r of rows) {
          const html = await r.getHTML({ includeSelectorTag: false });
          if (html.includes("Declutter")) {
            row = r;
            return true;
          }
        }
        return false;
      },
      { timeout: 10_000, timeoutMsg: 'expected a .cp-row labelled "Declutter…" to appear' },
    );
    expect(row, 'expected a .cp-row labelled "Declutter…"').to.not.equal(undefined);
    await row!.waitForClickable({ timeout: 10_000 });
    await row!.click();

    // The dialog mounted (aria-label is the localized title; default locale is English) in its
    // pre-scan intro state.
    const dialog = await $('[aria-label="Declutter"]');
    await dialog.waitForExist({ timeout: 10_000, timeoutMsg: "expected the Declutter dialog to render" });

    const scanBtn = await $('[data-testid="dc-scan-btn"]');
    await scanBtn.waitForExist({ timeout: 5_000, timeoutMsg: "expected the intro Scan button to render" });
    await snap("declutter-intro");
    await scanBtn.waitForClickable({ timeout: 10_000 });
    await scanBtn.click();

    // Core assertion: all four seeded findings — one per ClutterReason — stream back. If the scan
    // returned nothing (broken invoke, bad `dir` wiring, or a rules-engine regression) `[data-testid=
    // "dc-none"]` would render and no dc-row would exist, so this fails loudly rather than passing on
    // an empty view.
    const zeroByteRow = await findRowContaining(DECLUTTER_ZERO_BYTE_NAME);
    expect(await zeroByteRow.getHTML({ includeSelectorTag: false })).to.include("Empty file");

    const installerRow = await findRowContaining(DECLUTTER_INSTALLER_NAME);
    expect(await installerRow.getHTML({ includeSelectorTag: false })).to.include("Installer");

    const tempRow = await findRowContaining(DECLUTTER_TEMP_NAME);
    expect(await tempRow.getHTML({ includeSelectorTag: false })).to.include("Temporary / partial download");

    const backupRow = await findRowContaining(DECLUTTER_BACKUP_NAME);
    expect(await backupRow.getHTML({ includeSelectorTag: false })).to.include("Backup / leftover");

    await snap("declutter-results");

    // Nothing is pre-selected (safety) — Move to Bin starts disabled. Dismiss via the close button
    // without mutating the fixture, mirroring near-duplicates.smoke.ts's read-only-pass convention (the
    // move-to-bin/checkpoint mechanics are already covered against a mocked invoke in
    // DeclutterDialog.test.ts; this spec's job is proving the real IPC round trip renders findings).
    const moveBtn = await $('[data-testid="dc-move-btn"]');
    expect(await moveBtn.isEnabled()).to.equal(false);

    const closeBtn = await $('[data-testid="dc-close-btn"]');
    await closeBtn.click();
  });
});
