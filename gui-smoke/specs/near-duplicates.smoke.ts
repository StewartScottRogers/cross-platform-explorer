// CPE-1221 — headless GUI smoke for the Find-similar-documents dialog (NearDuplicatesDialog.svelte,
// CPE-1204, epic CPE-997): drives the real built app, opens the dialog via its real opener — the
// Command Palette (Ctrl+Shift+P → "Find similar documents…", the same `tool.findSimilarDocuments`
// command the Tools ▸ menu item is wired to, see App.svelte's `paletteCommands`) — scans the seeded
// tmpDir, and asserts the two near-identical seeded .md files render as ONE group containing BOTH.
//
// `NearDuplicatesDialog` is a near-clone of the already-pinned `SimilarImagesDialog`
// (similar-images.smoke.ts, CPE-1203): same dialog chrome, same grouped-results list. Unlike that
// dialog, this one is deliberately **read-only** (no delete/Move-to-Bin action — CPE-1204's stated
// scope), so there is no selection/keeper-guard assertion to mirror here; the spec dismisses via the
// close button and never mutates the tmpDir.
//
// The two seeded fixtures (wdio.conf.ts#seedNearDupDocsFixture) reuse the EXACT paragraphs
// `simhash.rs`'s own unit test measures at Hamming distance <= 8 (near pair) vs. >= 12 (unrelated) —
// i.e. this fixture is provably inside `document_similarity::DEFAULT_MAX_DISTANCE` (8), not just
// plausible-looking prose — so they group as a real near-duplicate pair, structurally far from the
// unrelated third doc also seeded into this tmpDir.
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, $$, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");

// Duplicated literals rather than importing from wdio.conf.ts — matches this harness's existing
// convention (see similar-images.smoke.ts's identical note).
const NEAR_DUP_DOC_A_NAME = "CPE-1221-notes-a.md";
const NEAR_DUP_DOC_B_NAME = "CPE-1221-notes-b.md";

/** The `[data-testid="nd-group"]` whose rendered HTML contains `name` — scans HTML rather than an
 *  exact-text locator, same reasoning as similar-images.smoke.ts's `findGroupContaining`. */
async function findGroupContaining(name: string): Promise<WebdriverIO.Element> {
  let found: WebdriverIO.Element | undefined;
  await browser.waitUntil(
    async () => {
      const groups = $$('[data-testid="nd-group"]');
      for await (const g of groups) {
        const html = await g.getHTML({ includeSelectorTag: false });
        if (html.includes(name)) {
          found = g;
          return true;
        }
      }
      return false;
    },
    { timeout: 20_000, timeoutMsg: `expected an nd-group containing the seeded ${name}` },
  );
  return found!;
}

describe("CPE-1221 — headless GUI smoke: Find-similar-documents dialog groups two near-identical docs", () => {
  before(() => {
    JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir: string };
  });

  // CPE-1149: on a failing run, leave a shot of the state it failed in (`near-duplicates-fail.png`) —
  // the inline `snap("near-duplicates")` below is only reached on a pass. Non-arrow fn so Mocha binds
  // `this`; `snapFailure` is a no-op on a pass.
  afterEach(async function () {
    await snapFailure(this.currentTest, "near-duplicates");
  });

  it("opens via the command palette, scans, and renders one group with both near-identical docs", async () => {
    // Wait for the initial `--open=<tmpDir>` navigation so `currentPath` is the seeded tmpDir before
    // reaching for a folder-scoped command — `tool.findSimilarDocuments` is `enabled: inFolder`.
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
    await paletteInput.addValue("similar documents");

    let docsRow: WebdriverIO.Element | undefined;
    await browser.waitUntil(
      async () => {
        const rows = $$(".cp-row");
        for await (const row of rows) {
          const html = await row.getHTML({ includeSelectorTag: false });
          if (html.includes("Find similar documents")) {
            docsRow = row;
            return true;
          }
        }
        return false;
      },
      { timeout: 10_000, timeoutMsg: 'expected a .cp-row labelled "Find similar documents…" to appear' },
    );
    expect(docsRow, 'expected a .cp-row labelled "Find similar documents…"').to.not.equal(undefined);
    await docsRow!.waitForClickable({ timeout: 10_000 });
    await docsRow!.click();

    // The dialog mounted (aria-label is the localized title; default locale is English).
    const dialog = await $('[aria-label="Find similar documents"]');
    await dialog.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected the Near-duplicates dialog to render",
    });

    // Kick the scan via its real button.
    const scanBtn = await $('[data-testid="nd-scan-btn"]');
    await scanBtn.waitForClickable({ timeout: 10_000 });
    await scanBtn.click();

    // Core assertion (CPE-1221): the two seeded near-identical .md docs cluster into ONE group with
    // BOTH — the FALSIFIABLE check tied to this spec's own fixture. If the scan returned no groups
    // (broken invoke, bad path, missing fixture, or a threshold regression) `[data-testid="nd-none"]`
    // would render and no nd-group would exist, so this fails loudly rather than passing on an empty
    // view.
    const group = await findGroupContaining(NEAR_DUP_DOC_A_NAME);
    const groupHtml = await group.getHTML({ includeSelectorTag: false });
    expect(groupHtml, `expected the group to also contain ${NEAR_DUP_DOC_B_NAME}`).to.include(
      NEAR_DUP_DOC_B_NAME,
    );

    // Exactly two items in that group (the near-duplicate pair — no stray members, e.g. the seeded
    // unrelated doc must NOT have joined it).
    const items = await group.$$('[data-testid="nd-item"]');
    expect(items.length, "expected exactly the two seeded near-identical docs in the group").to.equal(2);

    // CPE-1148/1149: capture the passing state (the grouped-results view) for the Visual Critic.
    await snap("near-duplicates");

    // Read-only dialog (no delete action) — dismiss via the close button.
    const closeBtn = await $('[data-testid="nd-close-btn"]');
    await closeBtn.click();
  });
});
