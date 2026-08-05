// CPE-1221 — headless GUI smoke for the Find-similar-documents dialog (NearDuplicatesDialog.svelte,
// CPE-1204, epic CPE-997): drives the real built app, opens the dialog via its real opener — the
// Command Palette (Ctrl+Shift+P → "Find similar documents…", the same `tool.findSimilarDocuments`
// command the Tools ▸ menu item is wired to, see App.svelte's `paletteCommands`) — scans the seeded
// tmpDir, and asserts the two near-identical seeded .md files render as ONE group containing BOTH.
//
// `NearDuplicatesDialog` is a near-clone of the already-pinned `SimilarImagesDialog`
// (similar-images.smoke.ts, CPE-1203): same dialog chrome, same grouped-results list. CPE-1204's
// original scope was read-only, but CPE-1324 added the same keeper-guarded cleanup flow
// `SimilarImagesDialog` already has (a per-item checkbox + a "Move N to Recycle Bin" button, disabled
// until a selection exists that still keeps at least one copy per group — see `keepsOnePerGroup` in
// ../duplicates). CPE-1331 closes the render-coverage gap that follow-up left: this spec now also
// checks ONE item's box and asserts the Move button flips from disabled to ENABLED, `snap()`-ing that
// state — but it never clicks Move-to-Bin, so the seeded fixtures on disk are never mutated.
//
// The seeded fixture (wdio.conf.ts#seedNearDupDocsFixture) follows the same shape as `simhash.rs`'s
// own `near_identical_text_is_closer_than_unrelated_text` unit test (a base paragraph, a lightly-edited
// near-copy, and an unrelated paragraph). Its actual measured SimHash Hamming distances are near = 3
// and far = 15 — provably inside `document_similarity::DEFAULT_MAX_DISTANCE` (8) for the near pair and
// well outside it for the unrelated doc — so the near pair groups as a real near-duplicate while the
// third doc stays a singleton. (Not byte-identical to the Rust test's strings; the distances above were
// measured directly against these fixture strings.)
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

    // CPE-1331: nothing is selected by default (CPE-1324's safety rail — see NearDuplicatesDialog's
    // own header comment), so the Move button starts disabled.
    const moveBtn = await $('[data-testid="nd-move-btn"]');
    await moveBtn.waitForExist({ timeout: 5_000, timeoutMsg: "expected the Move-to-Bin button to render" });
    expect(await moveBtn.isEnabled(), "expected Move-to-Bin to start disabled with nothing selected").to.equal(false);

    // Check ONE item's box in the two-item group — `keepsOnePerGroup` still passes (one copy remains),
    // so this is exactly the selection a real user makes before a safe cleanup.
    const checkbox = await group.$('[data-testid="nd-checkbox"]');
    await checkbox.waitForClickable({ timeout: 5_000, timeoutMsg: "expected the item's checkbox to render" });
    await checkbox.click();

    // Core assertion (CPE-1331): the Move button flips to ENABLED — the FALSIFIABLE check tied to this
    // click, tying `canClean`'s reactive guard to a real DOM state change rather than just unit-tested
    // logic. If `toggle`/`canClean` regressed, this would still read disabled.
    await browser.waitUntil(async () => moveBtn.isEnabled(), {
      timeout: 5_000,
      timeoutMsg: "expected Move-to-Bin to become enabled after checking one item's box",
    });
    const moveBtnText = await moveBtn.getText();
    expect(moveBtnText, 'expected the enabled button to read "Move 1 to Recycle Bin"').to.include("1");

    // Visual Critic frame: the ENABLED "Move N to Recycle Bin" state — the render-coverage gap this
    // ticket closes (previously only ever captured the disabled/nothing-selected state above).
    await snap("near-duplicates-move-enabled");

    // Never click Move-to-Bin — this dialog's cleanup mutates real files on disk (`deleteToTrash`) and
    // this spec's job is only to prove the button renders/enables correctly, not to exercise the
    // deletion path. Dismiss via the close button instead, leaving both seeded fixtures untouched.
    const closeBtn = await $('[data-testid="nd-close-btn"]');
    await closeBtn.click();
  });
});
