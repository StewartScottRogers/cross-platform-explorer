// CPE-1233 — headless GUI smoke for the Saved Searches flow (CPE-1229, epic CPE-978): drives the real
// built app, opens the SAME `SelectByDialog.svelte` used by "Select by…" but straight into its
// "Save search…" name field via the Command Palette (Ctrl+Shift+P → "Save search…", the
// `tool.saveSearch` command wired in App.svelte's `paletteCommands` — `selectByOpen = true;
// selectByAutoSave = true`), builds an Extension=png criterion, names it, and saves it
// (`saveCurrentSearch` -> `addSavedSearch`). Asserts the Sidebar's "Saved Searches" section
// (Sidebar.svelte, gated on `savedSearches.length > 0`) renders the new entry, opens it (dispatches
// `openSavedSearch` -> `openStructuredSearch`, which recursively `scanTree`s the seeded tmpDir and
// filters via `evaluateSavedSearch`), and asserts a seeded `.png` row shows while a non-matching file
// does not.
//
// This is a QA burndown pin (no new product code) — CPE-1229 already shipped and merged the whole
// flow; this spec is the first headless coverage + Visual Critic screenshot for it, mirroring the
// near-duplicates/similar-images dialog-driven captures (Ctrl+Shift+P open, `.cp-row` HTML scan,
// waits, `snap`, `snapFailure` afterEach).
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, $$, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");

// Duplicated literals rather than importing from wdio.conf.ts — matches this harness's existing
// convention (see similar-images.smoke.ts / near-duplicates.smoke.ts's identical notes). Both are
// seeded directly at the tmpDir root (not nested), so a shallow `.row` scan after the recursive
// search evaluates is unambiguous either way.
const MATCHING_PNG_NAME = "CPE-1203-scene-a.png"; // seeded by wdio.conf.ts#seedSimilarImagesFixture
const NON_MATCHING_ZIP_NAME = "CPE-1143-archive.zip"; // seeded by wdio.conf.ts#seedOrganizeFixture

const SEARCH_NAME = "CPE-1233 PNG search";

describe("CPE-1233 — headless GUI smoke: Saved Searches (save via palette, open from sidebar)", () => {
  before(() => {
    JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir: string };
  });

  // CPE-1149: on a failing run, leave a shot of the state it failed in (`saved-search-fail.png`) —
  // the inline `snap("saved-search")` below is only reached on a pass. Non-arrow fn so Mocha binds
  // `this`; `snapFailure` is a no-op on a pass.
  afterEach(async function () {
    await snapFailure(this.currentTest, "saved-search");
  });

  it("saves a search from the palette, shows it in the sidebar, and opens the filtered view", async () => {
    // Wait for the initial `--open=<tmpDir>` navigation so `currentPath` is the seeded tmpDir before
    // reaching for a folder-scoped command — `tool.saveSearch` is `enabled: inFolder`.
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
    await paletteInput.addValue("save search");

    let saveSearchRow: WebdriverIO.Element | undefined;
    await browser.waitUntil(
      async () => {
        const rows = $$(".cp-row");
        for await (const row of rows) {
          const html = await row.getHTML({ includeSelectorTag: false });
          if (html.includes("Save search")) {
            saveSearchRow = row;
            return true;
          }
        }
        return false;
      },
      { timeout: 10_000, timeoutMsg: 'expected a .cp-row labelled "Save search…" to appear' },
    );
    expect(saveSearchRow, 'expected a .cp-row labelled "Save search…"').to.not.equal(undefined);
    await saveSearchRow!.waitForClickable({ timeout: 10_000 });
    await saveSearchRow!.click();

    // The dialog mounted (SelectByDialog.svelte's aria-label — shared with "Select by…", but only
    // one can be open at a time, and `autoReveal` is what makes the name field visible up front).
    const dialog = await $('[aria-label="Select by criteria"]');
    await dialog.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected the Select-by/Save-search dialog to render",
    });

    // Default criterion kind is "ext" (Extension), so its input is already visible — type the
    // extension that matches the seeded PNGs.
    const extInput = await $('[aria-label="Extensions"]');
    await extInput.waitForExist({ timeout: 5_000, timeoutMsg: "expected the Extensions input to render" });
    await extInput.addValue("png");

    // `autoReveal` (the palette's "Save search…" entry) means the name field is already shown — fill
    // it in and click the real Save button (`data-testid="save-search-confirm"`).
    const nameField = await $('[aria-label="Search name"]');
    await nameField.waitForExist({ timeout: 5_000, timeoutMsg: "expected the Search name input to render" });
    await nameField.addValue(SEARCH_NAME);

    const saveBtn = await $('[data-testid="save-search-confirm"]');
    await saveBtn.waitForClickable({ timeout: 5_000 });
    await saveBtn.click();

    // The dialog closes on save (`saveCurrentSearch` sets `selectByOpen = false`).
    await dialog.waitForExist({ timeout: 5_000, reverse: true, timeoutMsg: "expected the dialog to close after saving" });

    // CPE-1229: the Sidebar's "Saved Searches" section is gated on `savedSearches.length > 0` — assert
    // it now renders its header...
    let sectionHeader: WebdriverIO.Element | undefined;
    await browser.waitUntil(
      async () => {
        const headers = $$(".fav-title");
        for await (const h of headers) {
          if ((await h.getText()) === "Saved Searches") {
            sectionHeader = h;
            return true;
          }
        }
        return false;
      },
      { timeout: 10_000, timeoutMsg: 'expected the Sidebar "Saved Searches" section header to render' },
    );
    expect(sectionHeader).to.not.equal(undefined);

    // ...and the new entry itself (Sidebar.svelte's `.nav-item.fav-item` for each saved search).
    let entryBtn: WebdriverIO.Element | undefined;
    await browser.waitUntil(
      async () => {
        const items = $$(".nav-item.fav-item");
        for await (const item of items) {
          const html = await item.getHTML({ includeSelectorTag: false });
          if (html.includes(SEARCH_NAME)) {
            entryBtn = item;
            return true;
          }
        }
        return false;
      },
      { timeout: 10_000, timeoutMsg: `expected a sidebar entry for the saved search "${SEARCH_NAME}"` },
    );
    expect(entryBtn, `expected a sidebar entry for "${SEARCH_NAME}"`).to.not.equal(undefined);

    // Open it — dispatches `openSavedSearch` -> `openStructuredSearch`, which recursively `scanTree`s
    // the seeded tmpDir and filters through `evaluateSavedSearch` (CPE-1229/CPE-986).
    await entryBtn!.waitForClickable({ timeout: 10_000 });
    await entryBtn!.click();

    // Core assertion (CPE-1233): the filtered recursive view shows the seeded matching .png and does
    // NOT show a seeded non-matching file — the FALSIFIABLE check tied to this spec's own fixture. If
    // the open-evaluator were broken (bad root, broken scan, wrong condition) neither row would ever
    // appear, so this fails loudly rather than passing on a stale/unfiltered view.
    await browser.waitUntil(
      async () => {
        const rows = $$(".row");
        for await (const row of rows) {
          const html = await row.getHTML({ includeSelectorTag: false });
          if (html.includes(MATCHING_PNG_NAME)) return true;
        }
        return false;
      },
      { timeout: 20_000, timeoutMsg: `expected a row for the seeded matching ${MATCHING_PNG_NAME}` },
    );

    let nonMatchFound = false;
    for (const row of await $$(".row")) {
      const html = await row.getHTML({ includeSelectorTag: false });
      if (html.includes(NON_MATCHING_ZIP_NAME)) {
        nonMatchFound = true;
        break;
      }
    }
    expect(nonMatchFound, `expected the non-matching ${NON_MATCHING_ZIP_NAME} to be filtered out`).to.equal(false);

    // CPE-1148/1149: capture the passing state (sidebar section + filtered result) for the Visual Critic.
    await snap("saved-search");
  });
});
