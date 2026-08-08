// CPE-1154 — the native-WebView2-context-menu leak, verified with a REAL pointer right-click.
//
// The bug: right-clicking an EMPTY folder (or any pane pixel not covered by a per-element handler)
// showed the native WebView2/Edge menu ("Back / Refresh / Save as / Print / …") instead of the app's
// own menu, because there was no global suppressor and `.empty-state` is a centred box that doesn't
// fill the pane. The prior driver-level check used only a SYNTHETIC event fired at the handled
// element, so it missed this coverage gap entirely — hence this spec drives a genuine `button:2`
// pointer down/up on the blank pane area and asserts the app's `.ctx` menu appears there.
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, $$, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";
import { rightClick } from "../lib/mouse.js"; // CPE-1155: non-grabbing CDP right-click

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");
// Seeded by wdio.conf.ts#onPrepare into the opened tmpDir (kept as a literal here to match this
// suite's convention of duplicating the seeded name rather than importing across the runner boundary).
const EMPTY_DIR_NAME = "CPE-1154-empty-folder";

describe("CPE-1154 — headless GUI smoke: real right-click never leaks the native menu", () => {
  before(() => {
    // Read (not required beyond confirming the harness wrote it) — the folder was opened via --open.
    JSON.parse(fs.readFileSync(STATE_FILE, "utf-8"));
  });

  afterEach(async function () {
    await snapFailure(this.currentTest, "context-menu");
  });

  // Navigate into the seeded EMPTY subfolder by double-clicking its row (located by scanning row
  // HTML for the folder name — the reliable primitive under wry's classic-WebDriver webview, per the
  // note in open-dir.smoke.ts; the text-based `$('=name')` locator is not reliable here).
  it("navigates into the empty folder and renders the empty state", async () => {
    // CPE-1481: wait for the initial `--open=<tmpDir>` navigation to have actually rendered before
    // scanning for the seeded folder's row — archive-browse.smoke.ts/archive-password.smoke.ts/
    // link-badge.smoke.ts/macro-in-menu.smoke.ts all gate their first assertion on this same
    // `[aria-current="page"]` crumb; this spec was missing it, so a still-loading listing (this
    // spec's own `before` hook does nothing but confirm the state file exists) could read as "no row"
    // rather than "not rendered yet".
    const crumb = await $('[aria-current="page"]');
    await crumb.waitForExist({ timeout: 30_000 });

    // CPE-1481: poll rather than a single scan — the crumb existing only proves navigation STARTED,
    // not that the (potentially large, ~27-entry) root listing has finished painting every row yet.
    let folderRow: WebdriverIO.Element | undefined;
    await browser.waitUntil(
      async () => {
        for (const row of await $$(".row")) {
          const html = await row.getHTML({ includeSelectorTag: false });
          if (html.includes(EMPTY_DIR_NAME)) {
            folderRow = row;
            return true;
          }
        }
        return false;
      },
      { timeout: 15_000, timeoutMsg: `expected a row for the seeded empty folder "${EMPTY_DIR_NAME}"` },
    );
    expect(folderRow, `expected a row for the seeded empty folder "${EMPTY_DIR_NAME}"`).to.not.equal(undefined);

    await folderRow!.doubleClick();

    // The empty folder shows FileList's centred `.empty-state` box (entries.length === 0).
    const emptyState = await $(".empty-state");
    await emptyState.waitForExist({
      timeout: 15_000,
      timeoutMsg: "expected the .empty-state box to render after entering the empty folder",
    });
  });

  it("a REAL right-click on the BLANK pane area opens the app's empty-area menu (.ctx), not the native one", async () => {
    // The menu must not already be showing.
    expect(await $(".ctx").isExisting()).to.equal(false);

    // Aim at a genuinely BLANK pane pixel: near the TOP of the scroll pane, where the vertically
    // centred `.empty-state` icon/text is NOT — this is exactly the region that used to leak the
    // native menu. Absolute viewport coords (getLocation is viewport-relative), so the click lands on
    // the same spot a user's cursor would.
    //
    // CPE-1155: driven via the non-grabbing CDP `rightClick` helper (was `browser.action('pointer')`,
    // which grabs input). CDP injects a faithful right-click — real hit-testing + native contextmenu —
    // without moving the OS cursor, so this runs in the background without hijacking the machine.
    const pane = await $(".filelist-pane");
    const loc = await pane.getLocation();
    const size = await pane.getSize();
    const x = Math.round(loc.x + size.width / 2);
    const y = Math.round(loc.y + 12);

    await rightClick({ x, y });

    // The app's custom context menu appears at the click.
    const ctx = await $(".ctx");
    await ctx.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected the app's .ctx menu to open on a real right-click in the empty folder's blank pane",
    });

    // It's the EMPTY-AREA variant: it carries Paste (Ctrl+V) and has NO on-item quick-action row.
    const ctxHtml = await ctx.getHTML({ includeSelectorTag: false });
    expect(ctxHtml, "expected the empty-area menu (Paste/Ctrl+V present)").to.include("Ctrl+V");
    expect(await $(".ctx .quickrow").isExisting(), "empty-area menu must not show the on-item quick-action row").to.equal(false);

    await snap("context-menu");
  });
});
