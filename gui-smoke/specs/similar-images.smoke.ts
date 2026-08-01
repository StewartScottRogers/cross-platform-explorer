// CPE-1203 — headless GUI smoke for the Find-similar-images dialog (SimilarImagesDialog.svelte,
// CPE-1202, epic CPE-997): drives the real built app, opens the dialog via its real opener — the
// Command Palette (Ctrl+Shift+P → "Find similar images…", the same `tool.findSimilarImages` command
// the Tools ▸ menu item is wired to, see App.svelte's `paletteCommands`) — scans the seeded tmpDir,
// and asserts the two near-duplicate structured (multi-band) PNGs render as ONE group with BOTH images
// (thumbnails present). Also asserts the SAFETY guard: "Move to Bin" is disabled when a selection would
// delete every copy of the group.
//
// This spec DOES NOT delete anything: it never clicks "Move to Bin" (and proves it's disabled once a
// whole-group selection is armed). The dialog is dismissed via its close button, so nothing outside
// the throwaway tmpDir is ever touched. It relies on `find_similar_images_stream` being read-only
// (image_similarity.rs only reads + hashes) rather than re-verifying that here.
//
// The two seeded fixtures (wdio.conf.ts#seedSimilarImagesFixture) are the SAME structured multi-band
// pattern at two sizes — a genuine perceptual near-duplicate pair — and are structurally far from the
// flat batch-media PNGs also in this tmpDir. Crucially they survive the CPE-1205 featureless guard
// (which now excludes solids/gradients that hash all-zero), so they form their own group of exactly two.
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, $$, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");

// Duplicated literals rather than importing from wdio.conf.ts — matches this harness's existing
// convention (see organize.smoke.ts's identical note). Seeded by wdio.conf.ts#seedSimilarImagesFixture.
const SIMILAR_PNG_A_NAME = "CPE-1203-scene-a.png";
const SIMILAR_PNG_B_NAME = "CPE-1203-scene-b.png";

/** The `[data-testid="sim-group"]` whose rendered HTML contains `name` — scans HTML rather than an
 *  exact-text locator, same reasoning as organize.smoke.ts / batch-media.smoke.ts. */
async function findGroupContaining(name: string): Promise<WebdriverIO.Element> {
  let found: WebdriverIO.Element | undefined;
  await browser.waitUntil(
    async () => {
      const groups = $$('[data-testid="sim-group"]');
      for await (const g of groups) {
        const html = await g.getHTML({ includeSelectorTag: false });
        if (html.includes(name)) {
          found = g;
          return true;
        }
      }
      return false;
    },
    { timeout: 20_000, timeoutMsg: `expected a sim-group containing the seeded ${name}` },
  );
  return found!;
}

describe("CPE-1203 — headless GUI smoke: Find-similar-images dialog groups two near-duplicates", () => {
  before(() => {
    JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir: string };
  });

  // CPE-1149: on a failing run, leave a shot of the state it failed in
  // (`similar-images-dialog-fail.png`) — the inline `snap("similar-images-dialog")` below is only
  // reached on a pass. Non-arrow fn so Mocha binds `this`; `snapFailure` is a no-op on a pass.
  afterEach(async function () {
    await snapFailure(this.currentTest, "similar-images-dialog");
  });

  it("opens via the command palette, scans, renders one group with both images + thumbnails, and guards deletion", async () => {
    // Wait for the initial `--open=<tmpDir>` navigation so `currentPath` is the seeded tmpDir before
    // reaching for a folder-scoped command — `tool.findSimilarImages` is `enabled: inFolder`.
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
    await paletteInput.addValue("similar images");

    let simRow: WebdriverIO.Element | undefined;
    await browser.waitUntil(
      async () => {
        const rows = $$(".cp-row");
        for await (const row of rows) {
          const html = await row.getHTML({ includeSelectorTag: false });
          if (html.includes("Find similar images")) {
            simRow = row;
            return true;
          }
        }
        return false;
      },
      { timeout: 10_000, timeoutMsg: 'expected a .cp-row labelled "Find similar images…" to appear' },
    );
    expect(simRow, 'expected a .cp-row labelled "Find similar images…"').to.not.equal(undefined);
    await simRow!.waitForClickable({ timeout: 10_000 });
    await simRow!.click();

    // The dialog mounted (aria-label is the localized title; default locale is English).
    const dialog = await $('[aria-label="Find similar images"]');
    await dialog.waitForExist({ timeout: 10_000, timeoutMsg: "expected the Similar-images dialog to render" });

    // Kick the scan via its real button.
    const scanBtn = await $('[data-testid="sim-scan-btn"]');
    await scanBtn.waitForClickable({ timeout: 10_000 });
    await scanBtn.click();

    // Core assertion (CPE-1203): the two seeded near-duplicate multi-band images cluster into ONE group with
    // BOTH images — the FALSIFIABLE check tied to this spec's own fixture. If the scan returned no
    // groups (broken invoke, bad path, missing fixture) `[data-testid="sim-none"]` would render and
    // no sim-group would exist, so this fails loudly rather than passing on an empty view.
    const group = await findGroupContaining(SIMILAR_PNG_A_NAME);
    const groupHtml = await group.getHTML({ includeSelectorTag: false });
    expect(groupHtml, `expected the group to also contain ${SIMILAR_PNG_B_NAME}`).to.include(SIMILAR_PNG_B_NAME);

    // Exactly two images in that group (the near-duplicate pair — no stray members).
    const cards = await group.$$('[data-testid="sim-image"]');
    expect(cards.length, "expected exactly the two seeded near-duplicates in the group").to.equal(2);

    // Thumbnails present: the real `thumbnail` command decoded both images, so each card renders an
    // <img> (ThumbnailImage only emits <img> once a thumbnail loads; a decode failure falls back to an
    // Icon and no <img>). Wait for both to load.
    await browser.waitUntil(async () => (await group.$$("img").length) >= 2, {
      timeout: 20_000,
      timeoutMsg: "expected both group cards to render a thumbnail <img>",
    });

    // SAFETY guard (CPE-1202): "Move to Bin" must never let a whole group be wiped.
    const moveBtn = await $('[data-testid="sim-move-btn"]');
    // Nothing selected by default → disabled.
    expect(await moveBtn.isEnabled(), "Move to Bin must be disabled with nothing selected").to.equal(false);

    const checkboxes = await group.$$('input[type="checkbox"]');
    // Selecting ONE copy leaves the other → enabled.
    await checkboxes[0].click();
    await browser.waitUntil(async () => await moveBtn.isEnabled(), {
      timeout: 10_000,
      timeoutMsg: "Move to Bin should enable once one (but not all) copies are selected",
    });
    // Selecting the SECOND copy would delete every copy → the keeper guard disables it again.
    await checkboxes[1].click();
    await browser.waitUntil(async () => !(await moveBtn.isEnabled()), {
      timeout: 10_000,
      timeoutMsg: "Move to Bin must disable when the selection would delete all copies of a group",
    });

    // CPE-1148/1149: capture the passing state (both thumbnails + the armed-but-guarded selection).
    await snap("similar-images-dialog");

    // Non-destructive: dismiss via the close button — never "Move to Bin".
    const closeBtn = await $('[data-testid="sim-close-btn"]');
    await closeBtn.click();
  });
});
