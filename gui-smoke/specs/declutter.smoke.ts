// CPE-1329 — headless GUI smoke for the Declutter dialog (DeclutterDialog.svelte, epic CPE-979): drives
// the real built app, opens the dialog via its real opener — the Command Palette (Ctrl+Shift+P →
// "Declutter…", the same `tool.findClutter` command the Tools ▸ menu item is wired to, see App.svelte's
// `paletteCommands`) — scans the seeded fixture folder, and asserts each of the four seeded findings
// (one per ClutterReason) renders under its human-labelled reason group. This is the sprint QA proof
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

/** The `[data-testid="dc-group"]` whose rendered HTML contains `text` — the group header renders
 *  `{reasonLabel(g.reason)} ({g.rows.length})` as a SIBLING of the `dc-row`s, not inside any one row
 *  (see DeclutterDialog.svelte), so the group — not the row — is what carries the reason label. Scans
 *  HTML rather than an exact-text locator, same reasoning as file-health.smoke.ts's `findRowContaining`. */
async function findGroupContaining(text: string): Promise<WebdriverIO.Element> {
  let found: WebdriverIO.Element | undefined;
  await browser.waitUntil(
    async () => {
      const groups = $$('[data-testid="dc-group"]');
      for await (const group of groups) {
        const html = await group.getHTML({ includeSelectorTag: false });
        if (html.includes(text)) {
          found = group;
          return true;
        }
      }
      return false;
    },
    { timeout: 20_000, timeoutMsg: `expected a dc-group containing "${text}"` },
  );
  return found!;
}

/** The `[data-testid="dc-row"]` (scoped to `group`) whose rendered HTML contains the seeded `name` —
 *  each row holds only a checkbox + filename button, so this confirms the finding actually rendered as
 *  a row under the group located by `findGroupContaining`. */
async function findRowContaining(group: WebdriverIO.Element, name: string): Promise<WebdriverIO.Element> {
  let found: WebdriverIO.Element | undefined;
  await browser.waitUntil(
    async () => {
      const rows = group.$$('[data-testid="dc-row"]');
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
    // CPE-1866: `waitForExist` only proves the row is IN THE DOM, not that it is actually clickable yet
    // (a listing can still be settling/streaming in — see CLAUDE.md's "Streaming liveness" convention).
    // Under session-per-shard this spec's session has already run several prior spec files' worth of
    // real interaction by the time it starts, so a momentary not-yet-interactive row reads identically
    // to a real regression without this — matches the same `waitForClickable`-before-click pattern
    // already used elsewhere in this suite (e.g. cost-history.smoke.ts's `.agent-log-btn`).
    await folderRow.waitForClickable({ timeout: 10_000 });
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

    // Core assertion: all four seeded findings — one per ClutterReason — stream back, each under its
    // correctly-labelled reason group. If the scan returned nothing (broken invoke, bad `dir` wiring, or
    // a rules-engine regression) `[data-testid="dc-none"]` would render and no dc-group would exist, so
    // this fails loudly rather than passing on an empty view. The reason label + count ("Empty file (1)")
    // lives on the `dc-group` header (a sibling of the rows), NOT inside any one `dc-row` — see
    // DeclutterDialog.svelte — so each finding is checked in two steps: locate its group by the labelled
    // header, then confirm the seeded filename rendered as a `dc-row` under that same group.
    const zeroByteGroup = await findGroupContaining("Empty file (1)");
    await findRowContaining(zeroByteGroup, DECLUTTER_ZERO_BYTE_NAME);

    const installerGroup = await findGroupContaining("Installer (1)");
    await findRowContaining(installerGroup, DECLUTTER_INSTALLER_NAME);

    const tempGroup = await findGroupContaining("Temporary / partial download (1)");
    await findRowContaining(tempGroup, DECLUTTER_TEMP_NAME);

    const backupGroup = await findGroupContaining("Backup / leftover (1)");
    await findRowContaining(backupGroup, DECLUTTER_BACKUP_NAME);

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
