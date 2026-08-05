// CPE-1315/1316/1317 — headless GUI smoke for the File Health panel's 4 tabs (FileHealthDialog.svelte,
// epic CPE-1002): drives the real built app, opens the dialog via its real opener — the Command Palette
// (Ctrl+Shift+P → "Find dangling links…", the same `tool.findDanglingLinks` command the Tools ▸ menu
// item is wired to, see App.svelte's `paletteCommands`) — then, for each of the 4 tabs, clicks Scan and
// asserts >=1 result row renders. This is the workshift QA end-to-end proof that the panel's 3 STREAMING
// tabs (dangling / mismatch / orphan, each over a real Tauri `ipc::Channel`) and its 1 plain-invoke tab
// (empty) all actually deliver rows over a REAL IPC round trip against the real built binary — jsdom in
// FileHealthDialog.test.ts / FileHealthDialogMismatch.test.ts / FileHealthDialogOrphan.test.ts /
// FileHealthDialogEmpty.test.ts already cover the streaming/cancel/supersede logic in isolation with a
// mocked Channel, but never against a real backend process.
//
// Fixtures:
//   - `dangling` reuses `seedLinkBadgeFixture`'s LINK_BROKEN_NAME (wdio.conf.ts) — a permanently-broken
//     symlink at the shared tmpDir root. Symlink creation needs Developer Mode/elevation on Windows, so
//     — exactly like link-badge.smoke.ts — this spec reads `linkBadgeFixture.supported` from STATE_FILE
//     and skips ONLY the dangling-tab assertion (not the whole suite) on an unprivileged runner.
//   - `mismatch` / `orphan` / `empty` reuse `seedFileHealthFixture`'s dedicated subfolder (wdio.conf.ts),
//     seeded unconditionally (no privilege requirement): a PE-header file misnamed `.jpg`, an orphaned
//     `.srt` subtitle with no video primary, and an empty nested subfolder.
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, $$, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");

// Kept as literals (not imported from wdio.conf.ts) to match this harness's existing convention of
// duplicating seeded filenames rather than reaching across the runner/worker boundary (see
// link-badge.smoke.ts's identical note on LINK_BROKEN_NAME).
const LINK_BROKEN_NAME = "CPE-1208-broken-link.txt";
const FILE_HEALTH_MISMATCH_NAME = "fh-disguised.jpg";
const FILE_HEALTH_ORPHAN_NAME = "fh-orphan.srt";
const FILE_HEALTH_EMPTY_SUBDIR_NAME = "fh-empty-nested";

/** The `[data-testid="fh-row"]` whose rendered HTML contains `name` — scans HTML rather than an exact-
 *  text locator, same reasoning as near-duplicates.smoke.ts / similar-images.smoke.ts. */
async function findRowContaining(name: string): Promise<WebdriverIO.Element> {
  let found: WebdriverIO.Element | undefined;
  await browser.waitUntil(
    async () => {
      const rows = $$('[data-testid="fh-row"]');
      for await (const row of rows) {
        const html = await row.getHTML({ includeSelectorTag: false });
        if (html.includes(name)) {
          found = row;
          return true;
        }
      }
      return false;
    },
    { timeout: 20_000, timeoutMsg: `expected an fh-row containing the seeded ${name}` },
  );
  return found!;
}

describe("CPE-1315/1316/1317 — headless GUI smoke: File Health dialog's 4 tabs stream real findings", () => {
  let linkBadgeSupported = false;

  before(() => {
    const state = JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as {
      linkBadgeFixture?: { supported: boolean };
    };
    linkBadgeSupported = state.linkBadgeFixture?.supported ?? false;
    if (!linkBadgeSupported) {
      // eslint-disable-next-line no-console
      console.warn(
        "[file-health.smoke] symlink creation was unprivileged on this runner — skipping the dangling-tab assertion only",
      );
    }
  });

  afterEach(async function () {
    await snapFailure(this.currentTest, "file-health");
  });

  it("opens via the command palette and streams/invokes >=1 finding on each of the 4 tabs", async function () {
    // Wait for the initial `--open=<tmpDir>` navigation so `currentPath` is the seeded tmpDir before
    // reaching for a folder-scoped command — `tool.findDanglingLinks` is `enabled: inFolder`.
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
    await paletteInput.addValue("dangling links");

    let row: WebdriverIO.Element | undefined;
    await browser.waitUntil(
      async () => {
        const rows = $$(".cp-row");
        for await (const r of rows) {
          const html = await r.getHTML({ includeSelectorTag: false });
          if (html.includes("Find dangling links")) {
            row = r;
            return true;
          }
        }
        return false;
      },
      { timeout: 10_000, timeoutMsg: 'expected a .cp-row labelled "Find dangling links…" to appear' },
    );
    expect(row, 'expected a .cp-row labelled "Find dangling links…"').to.not.equal(undefined);
    await row!.waitForClickable({ timeout: 10_000 });
    await row!.click();

    // The dialog mounted (aria-label is the localized title; default locale is English) with its
    // Dangling-links tab already active.
    const dialog = await $('[aria-label="File health"]');
    await dialog.waitForExist({ timeout: 10_000, timeoutMsg: "expected the File Health dialog to render" });

    // Pre-scan intro state, captured before any tab's Scan button is clicked — the Visual Critic's
    // baseline "what does this dialog look like before a scan runs" frame.
    const introScanBtn = await $('[data-testid="fh-scan-btn"]');
    await introScanBtn.waitForExist({ timeout: 5_000, timeoutMsg: "expected the intro Scan button to render" });
    await snap("file-health-intro");

    // CPE-1331: fill the shared exclude-pill row (CPE-1323) via its real quick-add chip — the intro
    // snap above only ever captured the EMPTY "no excludes yet" state (`.empty` placeholder text); this
    // is the render-coverage gap this ticket closes. Click rather than type-and-Enter so the seeded
    // pattern is deterministic and doesn't depend on synthetic key events landing correctly.
    const excludeSuggestChip = await $('[data-testid="fh-exclude-suggest"]');
    await excludeSuggestChip.waitForClickable({ timeout: 10_000, timeoutMsg: "expected an exclude quick-add chip to render" });
    const suggestedPattern = await excludeSuggestChip.getText(); // e.g. "+ node_modules"
    await excludeSuggestChip.click();

    // Core assertion (CPE-1331): the quick-add chip's pattern now renders as a real FILLED exclude pill
    // (`.chip`/`[data-testid="fh-exclude-remove"]`), not just the empty placeholder — the FALSIFIABLE
    // check tied to this click. If `addExclude` regressed, the excludes container would still show only
    // the `.empty` placeholder and no `fh-exclude-remove` chip-x would exist.
    await browser.waitUntil(async () => (await $$('[data-testid="fh-exclude-remove"]').length) > 0, {
      timeout: 10_000,
      timeoutMsg: "expected a filled exclude pill to render after clicking a quick-add chip",
    });
    const excludesEl = await $('[data-testid="fh-excludes"]');
    const excludesHtml = await excludesEl.getHTML({ includeSelectorTag: false });
    expect(excludesHtml, `expected the exclude pill row to contain "${suggestedPattern.replace(/^\+\s*/, "")}"`).to.include(
      suggestedPattern.replace(/^\+\s*/, ""),
    );

    // Visual Critic frame: the exclude row with a real, non-empty pill — the render-coverage gap this
    // ticket closes (previously only ever captured empty).
    await snap("file-health-excludes-filled");

    // --- Tab 1: dangling links (STREAMING find_dangling_links_stream, already active on open) ------
    if (!linkBadgeSupported) {
      // eslint-disable-next-line no-console
      console.warn("[file-health.smoke] dangling-tab assertion skipped: symlink fixture unsupported here");
    } else {
      const tab = await $('[data-testid="fh-tab-dangling"]');
      await tab.waitForExist({ timeout: 5_000, timeoutMsg: "expected the Dangling links tab to render" });
      const scanBtn = await $('[data-testid="fh-scan-btn"]');
      await scanBtn.waitForClickable({ timeout: 10_000 });
      await scanBtn.click();

      // Core assertion: the seeded permanently-broken symlink streams back as a dangling link with the
      // "Missing target" reason — the FALSIFIABLE check tied to this fixture. If the scan returned
      // nothing (broken invoke, bad path/excludes wiring, or a classifier regression) `[data-testid=
      // "fh-none"]` would render and no fh-row would exist, so this fails loudly rather than passing on
      // an empty view.
      const brokenRow = await findRowContaining(LINK_BROKEN_NAME);
      const badge = await brokenRow.$('[data-testid="fh-reason"]');
      await badge.waitForExist({ timeout: 5_000, timeoutMsg: "expected the row to show a reason badge" });
      expect(await badge.getText()).to.equal("Missing target");

      await snap("file-health-dangling");
    }

    // --- Tab 2: type mismatches (STREAMING find_type_mismatches_stream) ----------------------------
    const mismatchTab = await $('[data-testid="fh-tab-mismatch"]');
    await mismatchTab.waitForClickable({ timeout: 10_000, timeoutMsg: "expected the Type mismatch tab to be clickable" });
    await mismatchTab.click();
    const mismatchScanBtn = await $('[data-testid="fh-scan-btn"]');
    await mismatchScanBtn.waitForClickable({ timeout: 10_000, timeoutMsg: "expected the mismatch tab's Scan button" });
    await mismatchScanBtn.click();

    // The seeded PE-header file misnamed `.jpg` must stream back flagged — if the scan returned
    // nothing, `[data-testid="fh-none"]` would render instead and this loudly fails.
    const mismatchRow = await findRowContaining(FILE_HEALTH_MISMATCH_NAME);
    const mismatchReason = await mismatchRow.$('[data-testid="fh-reason"]');
    await mismatchReason.waitForExist({ timeout: 5_000, timeoutMsg: "expected the mismatch row to show a reason badge" });
    expect(await mismatchReason.getText()).to.include("Windows executable/library");

    await snap("file-health-mismatch");

    // --- Tab 3: orphan sidecars (STREAMING find_orphan_sidecars_stream) ----------------------------
    const orphanTab = await $('[data-testid="fh-tab-orphan"]');
    await orphanTab.waitForClickable({ timeout: 10_000, timeoutMsg: "expected the Orphan sidecars tab to be clickable" });
    await orphanTab.click();
    const orphanScanBtn = await $('[data-testid="fh-scan-btn"]');
    await orphanScanBtn.waitForClickable({ timeout: 10_000, timeoutMsg: "expected the orphan tab's Scan button" });
    await orphanScanBtn.click();

    // The seeded orphaned .srt (no matching video primary in the same folder) must stream back.
    await findRowContaining(FILE_HEALTH_ORPHAN_NAME);

    await snap("file-health-orphan");

    // --- Tab 4: empty folders (plain-invoke find_empty_dirs, no streaming Channel) ------------------
    const emptyTab = await $('[data-testid="fh-tab-empty"]');
    await emptyTab.waitForClickable({ timeout: 10_000, timeoutMsg: "expected the Empty folders tab to be clickable" });
    await emptyTab.click();
    const emptyScanBtn = await $('[data-testid="fh-scan-btn"]');
    await emptyScanBtn.waitForClickable({ timeout: 10_000, timeoutMsg: "expected the empty tab's Scan button" });
    await emptyScanBtn.click();

    // The seeded empty nested subfolder must come back as a cascade-empty directory.
    await findRowContaining(FILE_HEALTH_EMPTY_SUBDIR_NAME);

    await snap("file-health-empty");

    // Read-only dialog (no delete/repair action here) — dismiss via the close button.
    const closeBtn = await $('[data-testid="fh-close-btn"]');
    await closeBtn.click();
  });
});
