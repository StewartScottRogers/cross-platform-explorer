// CPE-1241 — render pin + Visual Critic screenshot for the ShredConfirmDialog (CPE-1240, epic
// CPE-738): "Securely delete…" is the one destructive action in this app with NO trash fallback
// (`shred_paths` overwrites a file's bytes then unlinks it — no undo, nothing to restore). Reviewer
// + UAT already verified its honesty copy + safeguard at code level; this is the first headless
// coverage of the real rendered dialog, burning down the manual-visual-verification debt for it.
//
// CRITICAL SAFETY: this spec must NEVER actually confirm the shred — clicking `[data-testid=
// "shred-confirm"]` would irreversibly destroy the seeded fixture file. It opens the dialog, asserts
// the permanence/caveat copy + danger button + scheme picker render, snaps it, then dismisses via
// Cancel — and asserts the seeded file is STILL ON DISK afterward, proving nothing was shredded.
//
// Drives the REAL built app: navigates into the dedicated seeded subfolder
// (`CPE-1241-shred-folder`, wdio.conf.ts#seedShredFixture — its own subfolder so this spec's blast
// radius, if anything ever went wrong, is contained to one throwaway file), real-right-clicks the
// seeded file row (CDP `rightClick`, non-grabbing — see gui-smoke/lib/mouse.ts's CPE-1155 header),
// clicks "Securely delete…" in the app's own `.ctx` menu (ContextMenu.svelte's `shreddable`-gated
// row, `ctx.shred` = "Securely delete…" in the default English locale), and asserts against the real
// selectors shipped in ShredConfirmDialog.svelte: `[data-testid="shred-permanence"]` (the "permanent
// and non-recoverable" copy), `[data-testid="shred-caveat"]` (the honest best-effort/SSD/
// copy-on-write platform caveat), `[data-testid="shred-scheme"]` (the overwrite-scheme picker), and
// `[data-testid="shred-confirm"]` (the danger button, labelled "Shred permanently" — asserted present
// but never clicked).
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, $$, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";
import { rightClick, type Point } from "../lib/mouse.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");
// Duplicated literals rather than importing across the runner/worker boundary — matches this
// harness's existing convention (see new-link.smoke.ts / context-menu.smoke.ts's identical notes).
const SHRED_DIR_NAME = "CPE-1241-shred-folder";
const SHRED_FILE_NAME = "CPE-1241-shred-me.txt";

/** Viewport-space centre of the FIRST `.row` whose rendered HTML contains `name`, or `null` if none
 *  matched — the getHTML-scan primitive every other spec in this suite uses (script-injected text
 *  locators don't reliably resolve against wry's classic-WebDriver webview). */
async function pointOfRowNamed(name: string): Promise<Point | null> {
  const rows = $$(".row");
  for await (const row of rows) {
    if ((await row.getHTML({ includeSelectorTag: false })).includes(name)) {
      const loc = await row.getLocation();
      const size = await row.getSize();
      return { x: Math.round(loc.x + Math.min(60, size.width / 2)), y: Math.round(loc.y + size.height / 2) };
    }
  }
  return null;
}

describe("CPE-1241 — headless GUI smoke: ShredConfirmDialog renders, then Cancels without shredding", () => {
  let tmpDir = "";
  let seededFilePath = "";

  before(() => {
    ({ tmpDir } = JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir: string });
    seededFilePath = path.join(tmpDir, SHRED_DIR_NAME, SHRED_FILE_NAME);
  });

  // CPE-1149: on a failing run, leave a shot of the state it failed in (`shred-dialog-fail.png`) —
  // the inline `snap("shred-dialog")` below is only reached on a pass. Non-arrow fn so Mocha binds
  // `this`; `snapFailure` is a no-op on a pass.
  afterEach(async function () {
    await snapFailure(this.currentTest, "shred-dialog");
  });

  it("navigates into the dedicated shred fixture folder", async () => {
    const crumb = await $('[aria-current="page"]');
    await crumb.waitForExist({ timeout: 30_000 });
    await crumb.click(); // plain, non-interactive <span> — just moves focus off any input

    let folderRow: WebdriverIO.Element | undefined;
    for await (const row of await $$(".row")) {
      if ((await row.getHTML({ includeSelectorTag: false })).includes(SHRED_DIR_NAME)) {
        folderRow = row;
        break;
      }
    }
    expect(folderRow, `expected a row for the seeded "${SHRED_DIR_NAME}"`).to.not.equal(undefined);
    await folderRow!.doubleClick();

    await browser.waitUntil(
      async () => (await pointOfRowNamed(SHRED_FILE_NAME)) !== null,
      { timeout: 15_000, timeoutMsg: `expected a row for the seeded "${SHRED_FILE_NAME}" after navigating in` },
    );
  });

  it('right-clicks the seeded file and opens "Securely delete…" — asserts the real dialog copy/controls, snaps, then Cancels WITHOUT shredding', async () => {
    // Sanity: the fixture is genuinely on disk before we touch anything.
    expect(fs.existsSync(seededFilePath), "expected the seeded shred fixture to exist before the test").to.equal(true);

    const filePoint = await pointOfRowNamed(SHRED_FILE_NAME);
    expect(filePoint, `expected a row for the seeded "${SHRED_FILE_NAME}"`).to.not.equal(null);

    // CPE-1155: non-grabbing CDP right-click — real hit-testing + native contextmenu order, without
    // moving the OS cursor (see mouse.ts's header).
    await rightClick(filePoint!);
    const ctx = await $(".ctx");
    await ctx.waitForExist({ timeout: 10_000, timeoutMsg: "expected the item context menu (.ctx) to open" });

    // "Securely delete…" — ContextMenu.svelte's `shreddable`-gated row (CPE-1240). The seeded target
    // is a single plain file on a real writable location, so `shreddable` is true and the row renders.
    // getHTML-scan locator (same reasoning as `pointOfRowNamed` above — script-injected text locators
    // don't reliably resolve against wry's classic-WebDriver webview).
    let shredBtn: WebdriverIO.Element | undefined;
    for await (const btn of await $$(".ctx button.row")) {
      if ((await btn.getHTML({ includeSelectorTag: false })).includes("Securely delete")) {
        shredBtn = btn;
        break;
      }
    }
    expect(shredBtn, 'expected a ".ctx button.row" labelled "Securely delete…"').to.not.equal(undefined);
    await shredBtn!.waitForClickable({ timeout: 10_000 });
    await shredBtn!.click();

    // The dialog mounted — its real, static aria-label (ShredConfirmDialog.svelte's `<div role=
    // "dialog" aria-modal="true" aria-label="Securely delete?">`, not templated with the filename).
    const dialog = await $('[aria-label="Securely delete?"]');
    await dialog.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected the ShredConfirmDialog to render after clicking Securely delete…",
    });

    // Permanence copy (CPE-1240's "no trash fallback, no undo" honesty requirement).
    const permanence = await $('[data-testid="shred-permanence"]');
    await permanence.waitForExist({ timeout: 5_000, timeoutMsg: "expected the permanence copy to render" });
    const permanenceText = await permanence.getText();
    expect(permanenceText, "expected the permanence copy to state it is permanent").to.match(/permanent/i);
    expect(permanenceText, "expected the permanence copy to state it is non-recoverable").to.match(/non-recoverable/i);

    // Platform caveat copy (best-effort overwrite — SSD wear-levelling / copy-on-write caveat).
    const caveat = await $('[data-testid="shred-caveat"]');
    await caveat.waitForExist({ timeout: 5_000, timeoutMsg: "expected the platform caveat copy to render" });
    const caveatText = await caveat.getText();
    expect(caveatText, "expected the caveat copy to state overwriting is best-effort").to.match(/best-effort/i);

    // Scheme picker.
    const schemeSelect = await $('[data-testid="shred-scheme"]');
    expect(await schemeSelect.isExisting(), "expected the overwrite-scheme picker to render").to.equal(true);

    // The danger "Shred permanently" button renders and is present — asserted, but NEVER clicked.
    const confirmBtn = await $('[data-testid="shred-confirm"]');
    expect(await confirmBtn.isExisting(), 'expected the "Shred permanently" danger button to render').to.equal(true);
    const confirmText = await confirmBtn.getText();
    expect(confirmText, 'expected the danger button labelled "Shred permanently"').to.match(/shred permanently/i);

    // The Cancel button also renders (the safe dismiss path this spec takes below).
    const cancelBtn = await $('[data-testid="shred-cancel"]');
    expect(await cancelBtn.isExisting(), "expected the Cancel button to render").to.equal(true);

    // CPE-1148/1149: capture the passing state (permanence copy + caveat + scheme picker + danger
    // button, all rendered) for the Visual Critic BEFORE dismissing.
    await snap("shred-dialog");

    // SAFETY: dismiss via Cancel — never click "Shred permanently". `ShredConfirmDialog` dispatches
    // `close` on Cancel, which App.svelte's `on:close` handler clears `shredConfirmFor` on, unmounting
    // the dialog (`{#if shredConfirmFor}`).
    await cancelBtn.click();
    await dialog.waitForExist({
      timeout: 5_000,
      reverse: true,
      timeoutMsg: "expected the dialog to close after clicking Cancel",
    });

    // Falsifiable proof nothing was shredded: the seeded fixture file still exists on disk, byte for
    // byte, after the whole flow.
    expect(fs.existsSync(seededFilePath), "expected the seeded shred fixture to STILL exist after Cancel — nothing must ever be shredded").to.equal(true);
  });
});
