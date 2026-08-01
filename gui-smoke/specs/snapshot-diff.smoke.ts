// CPE-1197 (frontend half, epic CPE-735) — headless GUI smoke pin for the snapshot per-file diff
// affordance: drives the real built app, opens `CheckpointDialog.svelte` via the Command Palette
// (the same reachability path checkpoint-restore.smoke.ts/organize.smoke.ts document — Tools →
// "Checkpoint & rollback…" is wired to the identical `tool.checkpoint` command, see App.svelte's
// `paletteCommands`), creates a REAL checkpoint over a seeded file through the dialog's own
// `checkpoint_create` button, mutates that file on disk (staging drift), previews the revert so the
// changed-file row renders, then clicks that row's "Open diff" affordance and asserts the reused
// `DiffPeek.svelte` renders the before/after — capturing the panel as a screenshot for the Visual
// Critic (CPE-1148).
//
// == Why the dialog's OWN create button (not a hand-seeded store) ==
// Same rationale as checkpoint-restore.smoke.ts: `checkpoint_diff_file` reads a REAL manifest + blob
// from the store, so the checkpoint has to be a genuine one. Rather than poke the content-addressed
// store shape from Node, this spec drives the dialog's "Create checkpoint" button — the exact
// production path a user takes — and lets the running app write the real store.
//
// == rel_path is just the file's basename ==
// The seeded diff file lives directly under the opened root, so `snapshot_capture`'s
// `relative_slash_path` (crates/server/src/snapshot_capture.rs) yields the bare filename as its
// rel_path — no subfolder segment to account for when matching the `diff-btn-<relPath>` /
// `diff-panel-<relPath>` testids `CheckpointDialog.svelte` renders per drift row.
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, $$, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");

// Duplicated literal rather than importing from wdio.conf.ts — matches this harness's convention of
// not reaching across the runner/worker process boundary (see open-dir.smoke.ts's FIXTURE_NAME
// comment).
const DIFF_FILE_NAME = "CPE-1197-diff.txt";
const BEFORE_TEXT = "original line — captured by the checkpoint\n";
const AFTER_TEXT = "changed line — written after the checkpoint\n";

describe("CPE-1197 — headless GUI smoke: snapshot per-file diff opens from a checkpoint preview", () => {
  let tmpDir = "";

  before(() => {
    ({ tmpDir } = JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir: string });
  });

  // CPE-1149 discipline: on a failing run leave a shot of the failure state; the inline snap below is
  // only reached on a pass. Non-arrow fn so Mocha binds `this`.
  afterEach(async function () {
    await snapFailure(this.currentTest, "snapshot-diff");
  });

  it("creates a checkpoint, stages drift, and opens the per-file diff from the preview's changed-file row", async () => {
    // 1) Wait for the initial `--open=<tmpDir>` navigation so the folder is settled before the dialog's
    //    `initialPath` (bound to `currentPath` in App.svelte) is prefilled with it.
    const crumb = await $('[aria-current="page"]');
    await crumb.waitForExist({ timeout: 30_000 });
    await crumb.click(); // plain, non-interactive <span> — just moves focus off any input

    // 2) Seed the diff file's ORIGINAL content BEFORE the checkpoint so the checkpoint captures it.
    const diffPath = path.join(tmpDir, DIFF_FILE_NAME);
    fs.writeFileSync(diffPath, BEFORE_TEXT, "utf-8");

    // 3) Ctrl+Shift+P opens the Command Palette (App.svelte's `handleKeydown`) — the same
    //    reachability path organize.smoke.ts documents.
    await browser.keys(["Control", "Shift", "P"]);

    const paletteInput = await $(".cp-input");
    await paletteInput.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected .cp-input (Command Palette) to render after Ctrl+Shift+P",
    });
    await paletteInput.addValue("checkpoint");

    // Locate the "Checkpoint & rollback…" row by scanning rendered HTML for its label rather than a
    // `$('=text')` exact-text locator (see organize.smoke.ts's identical note on wry/WebDriver).
    let checkpointRow: WebdriverIO.Element | undefined;
    await browser.waitUntil(
      async () => {
        const rows = $$(".cp-row");
        for await (const row of rows) {
          const html = await row.getHTML({ includeSelectorTag: false });
          if (html.includes("Checkpoint")) {
            checkpointRow = row;
            return true;
          }
        }
        return false;
      },
      { timeout: 10_000, timeoutMsg: 'expected a .cp-row labelled "Checkpoint & rollback…" to appear' },
    );
    expect(checkpointRow, 'expected a .cp-row labelled "Checkpoint & rollback…"').to.not.equal(undefined);
    await checkpointRow!.waitForClickable({ timeout: 10_000 });
    await checkpointRow!.click();

    // 4) The dialog mounted with `path` prefilled — wait for its checkpoint list to settle.
    const pathInput = await $('[data-testid="checkpoint-path"]');
    await pathInput.waitForExist({ timeout: 10_000, timeoutMsg: "expected the Checkpoint dialog to render" });
    await $('[data-testid="checkpoint-list"]').waitForExist({ timeout: 10_000 });

    // 5) Create a REAL checkpoint over the seeded file's original content via the dialog's own button
    //    — the exact production path a user takes (not a hand-seeded store).
    const createBtn = await $('[data-testid="create-btn"]');
    await createBtn.waitForClickable({ timeout: 10_000 });
    await createBtn.click();

    const note = await $('[data-testid="note"]');
    await note.waitForExist({
      timeout: 15_000,
      timeoutMsg: "expected the dialog's note to render once checkpoint_create resolved",
    });

    // 6) Stage drift: overwrite the captured file AFTER the checkpoint, so the preview's fresh scan
    //    reports it as a changed file (an overwrite the revert would apply).
    fs.writeFileSync(diffPath, AFTER_TEXT, "utf-8");

    // 7) Preview the just-created checkpoint — `checkpoint_list` is newest-first
    //    (checkpoint_store::checkpoint_list), so the first row's Preview button is the one to click.
    const previewBtns = await $$('[data-testid^="preview-btn-"]');
    expect(previewBtns.length, "expected at least one checkpoint row's Preview button").to.be.greaterThan(0);
    const previewBtn = previewBtns[0];
    await previewBtn.waitForClickable({ timeout: 10_000 });
    await previewBtn.click();

    const driftList = await $('[data-testid="drift-list"]');
    await driftList.waitForExist({
      timeout: 15_000,
      timeoutMsg: "expected the drift list to render once checkpoint_preview_revert resolved with drift",
    });

    // 8) Click the seeded file's "Open diff" affordance.
    const diffBtn = await $(`[data-testid="diff-btn-${DIFF_FILE_NAME}"]`);
    await diffBtn.waitForExist({
      timeout: 10_000,
      timeoutMsg: `expected a diff-btn row for the seeded ${DIFF_FILE_NAME}`,
    });
    await diffBtn.waitForClickable({ timeout: 10_000 });
    await diffBtn.click();

    // Core assertion (CPE-1197 frontend half): `checkpoint_diff_file` resolved and the reused
    // `DiffPeek.svelte` rendered the real before/after — not a binary/oversize/unknown-path notice.
    const diffPanel = await $(`[data-testid="diff-panel-${DIFF_FILE_NAME}"]`);
    await diffPanel.waitForExist({
      timeout: 15_000,
      timeoutMsg: "expected the diff panel to render once checkpoint_diff_file resolved",
    });
    expect(await $('[data-testid="diff-error"]').isExisting(), "expected no diff-error notice for a plain text file")
      .to.equal(false);

    const panelHtml = await diffPanel.getHTML({ includeSelectorTag: false });
    expect(panelHtml, "expected the diff panel to render the checkpoint's captured line").to.include("original line");
    expect(panelHtml, "expected the diff panel to render the live file's changed line").to.include("changed line");

    // 9) Capture the opened diff panel for the Visual Critic.
    await snap("snapshot-diff");
  });
});
