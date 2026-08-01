// CPE-1189 — render pin for the macro library dialog (`MacrosDialog.svelte`, epic CPE-739). Drives
// the REAL built app, opens the library via the Command Palette (`app.macros` in App.svelte's
// `paletteCommands` — the same keyboard-first opener a real user has), creates a one-step macro, and
// asserts it renders in the saved-macro list (name + step count). Mirrors column-picker.smoke.ts's and
// organize.smoke.ts's "open via palette, scan rows by rendered HTML" pattern — the text-based `$('=text')`
// exact-text locator is not reliable against wry's classic-WebDriver webview (see those specs' notes).
//
// Non-destructive tidy end: the macro created here is deleted again via its row's Delete button before
// the spec finishes, so repeated CI runs never accumulate stray entries in the persisted macro catalog.
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, $$, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");

const MACRO_NAME = "CPE-1189 Smoke Macro";

/** Open the Command Palette (Ctrl+Shift+P) and click the first `.cp-row` whose rendered HTML includes
 *  `labelSubstring`, after typing `query` into the always-autofocused palette input. */
async function openViaPalette(query: string, labelSubstring: string): Promise<void> {
  await browser.keys(["Control", "Shift", "P"]);
  const paletteInput = await $(".cp-input");
  await paletteInput.waitForExist({
    timeout: 10_000,
    timeoutMsg: "expected .cp-input (Command Palette) to render after Ctrl+Shift+P",
  });
  await paletteInput.addValue(query);

  let row: WebdriverIO.Element | undefined;
  await browser.waitUntil(
    async () => {
      const rows = $$(".cp-row");
      for await (const r of rows) {
        const html = await r.getHTML({ includeSelectorTag: false });
        if (html.includes(labelSubstring)) {
          row = r;
          return true;
        }
      }
      return false;
    },
    { timeout: 10_000, timeoutMsg: `expected a .cp-row labelled "${labelSubstring}" to appear` },
  );
  expect(row, `expected a .cp-row labelled "${labelSubstring}"`).to.not.equal(undefined);
  await row!.waitForClickable({ timeout: 10_000 });
  await row!.click();
}

describe("CPE-1189 — headless GUI smoke: the macro library renders a created macro", () => {
  before(() => {
    JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir: string };
  });

  afterEach(async function () {
    await snapFailure(this.currentTest, "macros-library");
  });

  it("opens via the command palette, creates a one-step macro, and lists it", async () => {
    // Settle the initial --open=<tmpDir> navigation first (this dialog isn't folder-scoped, but every
    // other spec in this suite waits on it before reaching for the palette — stay consistent).
    const crumb = await $('[aria-current="page"]');
    await crumb.waitForExist({ timeout: 30_000 });
    await crumb.click();

    await openViaPalette("macros", "Manage macros");

    const dialog = await $('.dialog[aria-label="Macros"]');
    await dialog.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected MacrosDialog's .dialog[aria-label='Macros'] to render",
    });

    await (await $('[data-testid="new-macro-btn"]')).click();
    await (await $('[data-testid="add-step-btn"]')).click(); // default kind: rename

    const nameField = await $("#macro-name");
    await nameField.addValue(MACRO_NAME);
    const stepField = await $('[data-testid="step-field-0"]');
    await stepField.addValue("{stem}_smoke.{ext}");

    const saveBtn = await $('[data-testid="save-macro-btn"]');
    await saveBtn.waitForEnabled({ timeout: 5_000 });
    await saveBtn.click();

    const macroRow = await $(`[data-testid="macro-${MACRO_NAME}"]`);
    await macroRow.waitForExist({
      timeout: 10_000,
      timeoutMsg: `expected a macro-${MACRO_NAME} row to render after saving`,
    });
    expect(await macroRow.getHTML({ includeSelectorTag: false })).to.include("1 step");

    // CPE-1148 Part A: capture the library with the created macro visible, before cleanup below.
    await snap("macros-library");

    // Non-destructive tidy end: delete the macro this spec created.
    await (await $(`[data-testid="delete-btn-${MACRO_NAME}"]`)).click();
    await macroRow.waitForExist({ timeout: 10_000, reverse: true, timeoutMsg: "expected the smoke macro row to disappear after Delete" });

    await (await $('.dialog[aria-label="Macros"] .actions .btn.primary')).click(); // Close
  });
});
