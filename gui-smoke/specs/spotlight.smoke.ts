// CPE-1216 (epic CPE-704) — headless GUI smoke pin for the Spotlight overlay (Spotlight.svelte):
// drives the real built app, opens the overlay via its in-app trigger (the Command Palette's
// "Spotlight (search everywhere)…" entry — the same reachability path organize.smoke.ts/
// column-picker.smoke.ts use for their own dialogs, and exactly what the ticket asks for: verifiable
// without the OS-level global hotkey CPE-1215 owns), types a query, and asserts the overlay renders
// ranked + matched-position-highlighted, SECTIONED rows fed by the real `spotlight_search` command
// (CPE-1214) over the real (streamed) file-name walk of the seeded tmpDir (CPE-666's engine, per
// [[prefer-streaming-liveness]]) — not a mocked/stubbed backend.
//
// Non-destructive: Enter reveals the matched file (selects it in the listing) rather than mutating
// anything on disk, so no fixture cleanup is needed.
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, $$, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");
// Seeded by wdio.conf.ts#onPrepare alongside the tmpDir this harness opens with `--open` (duplicated
// literal, not imported — matches this suite's established convention, see open-dir.smoke.ts's
// FIXTURE_NAME comment).
const MARKER_NAME = "CPE-1045-marker.txt";

describe("CPE-1216 — headless GUI smoke: the Spotlight overlay renders ranked, sectioned, highlighted rows", () => {
  before(() => {
    JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir: string };
  });

  // CPE-1149: on a failing run, leave a shot of the state it failed in (`spotlight-fail.png`) — the
  // inline `snap("spotlight")` below is only reached on a pass. Non-arrow fn so Mocha binds `this`;
  // `snapFailure` is a no-op on a pass and swallows its own errors.
  afterEach(async function () {
    await snapFailure(this.currentTest, "spotlight");
  });

  it("opens via the Command Palette's in-app trigger, ranks a typed query, and Enter reveals the hit", async () => {
    // Wait for the initial `--open=<tmpDir>` navigation (also asserted in open-dir.smoke.ts) so the
    // window has real, settled DOM content before reaching for a keyboard shortcut.
    const crumb = await $('[aria-current="page"]');
    await crumb.waitForExist({ timeout: 30_000 });
    await crumb.click(); // plain, non-interactive <span> — just moves focus off any input

    // Ctrl+Shift+P opens the Command Palette (App.svelte's `handleKeydown`) — the same reachability
    // path organize.smoke.ts/column-picker.smoke.ts use.
    await browser.keys(["Control", "Shift", "P"]);
    const paletteInput = await $(".cp-input");
    await paletteInput.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected .cp-input (Command Palette) to render after Ctrl+Shift+P",
    });
    await paletteInput.addValue("spotlight");

    // Locate the "Spotlight (search everywhere)…" row by scanning rendered HTML for its label rather
    // than a `$('=text')` exact-text locator — the latter relies on script-injected text matching that
    // doesn't reliably resolve against wry's webview under the classic WebDriver protocol this harness
    // forces (see organize.smoke.ts's identical note).
    let spotlightRow: WebdriverIO.Element | undefined;
    await browser.waitUntil(
      async () => {
        const rows = $$(".cp-row");
        for await (const row of rows) {
          const html = await row.getHTML({ includeSelectorTag: false });
          if (html.includes("Spotlight")) {
            spotlightRow = row;
            return true;
          }
        }
        return false;
      },
      { timeout: 10_000, timeoutMsg: 'expected a .cp-row labelled "Spotlight (search everywhere)…" to appear' },
    );
    expect(spotlightRow, 'expected a .cp-row labelled "Spotlight (search everywhere)…"').to.not.equal(undefined);
    await spotlightRow!.waitForClickable({ timeout: 10_000 });
    await spotlightRow!.click();

    // The overlay mounted (Spotlight.svelte's `.sp-panel[role="dialog"]`).
    const panel = await $('.sp-panel[role="dialog"]');
    await panel.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected .sp-panel (the Spotlight overlay) to render after choosing the palette command",
    });
    expect(await panel.isDisplayed(), "expected the Spotlight overlay to be visible").to.equal(true);

    // Type a query matching the seeded marker file's name — this exercises the real, streamed
    // `find_files_by_name_stream` walk (CPE-666) feeding the "file" source into the real
    // `spotlight_search` command (CPE-1214), not a stub.
    const spotlightInput = await $(".sp-input");
    await spotlightInput.addValue("marker");

    // Core assertion (CPE-1216): a sectioned, ranked row for the seeded marker file renders — the
    // FALSIFIABLE check the ticket asks for. If `spotlight_search` ever returned nothing (a broken
    // invoke, a bad root, or the walk failing), NO `.sp-row` would exist and this fails loudly rather
    // than silently passing on an empty overlay. Debounced (150ms) + a real (if tiny) directory walk,
    // so this polls rather than asserting immediately.
    await browser.waitUntil(
      async () => {
        const rows = $$(".sp-row");
        for await (const row of rows) {
          const title = await row.getAttribute("title");
          if (title && title.includes(MARKER_NAME)) return true;
        }
        return false;
      },
      { timeout: 15_000, timeoutMsg: `expected a .sp-row titled with the seeded "${MARKER_NAME}"` },
    );

    // Sectioned (CPE-1216: "renders sectioned results"): the row lives under a "Files" section header.
    const sectionLabels = $$(".sp-section-label");
    const labelTexts: string[] = [];
    for await (const el of sectionLabels) labelTexts.push(await el.getText());
    expect(labelTexts, "expected a 'Files' section header to render").to.include("Files");

    // Matched-position highlighting (CPE-1216: from `SpotResult.positions`): the typed "marker"
    // substring is wrapped in a <mark class="sp-hl"> inside the matched row, not just plain text.
    const rows = $$(".sp-row");
    let markerRow: WebdriverIO.Element | undefined;
    for await (const row of rows) {
      const title = await row.getAttribute("title");
      if (title && title.includes(MARKER_NAME)) {
        markerRow = row;
        break;
      }
    }
    expect(markerRow, "expected to re-locate the marker row for the highlight assertion").to.not.equal(undefined);
    const markEl = await markerRow!.$("mark.sp-hl");
    expect(await markEl.isExisting(), "expected a <mark class=\"sp-hl\"> inside the matched row").to.equal(true);
    const markText = (await markEl.getText()).toLowerCase();
    expect(markText, "expected the highlighted run to be part of the typed query").to.include("marker");

    // CPE-1148 Part A: capture the overlay's ranked, highlighted state (after the assertions above,
    // before Enter dismisses it below). On a FAILING run this line is never reached — the `afterEach`
    // hook above captures `spotlight-fail.png` of the failure state instead (CPE-1149).
    await snap("spotlight");

    // Enter activates the top row and closes the overlay (non-destructive — a "file" activation only
    // reveals/selects it, never mutates anything on disk; an "action" activation just runs a palette
    // command). Either way the overlay must close.
    await browser.keys(["Enter"]);
    await panel.waitForExist({
      reverse: true,
      timeout: 10_000,
      timeoutMsg: "expected the Spotlight overlay to close after Enter activates a row",
    });
  });
});
