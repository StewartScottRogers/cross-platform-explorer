// CPE-1045 — the manual `--open <dir>` verification (CPE-1043/1044) automated: launches the real
// built app headlessly via tauri-driver, and asserts the folder actually opened.
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, browser } from "@wdio/globals";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");
const MARKER_NAME = "CPE-1045-marker.txt";

describe("CPE-1045 — headless GUI smoke: --open <dir> navigates", () => {
  let tmpBasename = "";

  before(() => {
    // Written by wdio.conf.ts#onPrepare in the main process before this session started.
    const { tmpDir } = JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir: string };
    tmpBasename = path.basename(tmpDir);
  });

  // Health check (burndown #2 — build -> deploy -> run smoke): the window launched and is
  // actually responding, independent of whether navigation itself worked.
  it("the app window launched and <body> rendered non-empty content", async () => {
    const body = await $("body");
    expect(await body.isExisting()).to.equal(true);

    const html = await body.getHTML({ includeSelectorTag: false });
    expect(html.trim().length).to.be.greaterThan(0);
  });

  // Core assertion (burndown #1 — GUI end-to-end): the same manual check hand-done for
  // CPE-1043/1044 — does the current-folder breadcrumb (`[aria-current="page"]`, the selector
  // App.features.test.ts already uses) show the folder we launched with `--open`?
  it("--open <tmpdir> navigated: the breadcrumb shows the folder name", async () => {
    const crumb = await $('[aria-current="page"]');
    await crumb.waitForExist({ timeout: 30_000 });

    const text = await crumb.getText();
    expect(text).to.equal(tmpBasename);
  });

  // The folder's contents actually rendered — not just that the breadcrumb string updated.
  //
  // Note: this deliberately checks the rendered <body> HTML for the marker filename rather than
  // locating it with WebdriverIO's `$('=text')` exact-text selector. That locator strategy relies
  // on script injection that doesn't reliably resolve against wry's webview under the classic
  // WebDriver protocol this harness forces (see the `wdio:enforceWebDriverClassic` comment above) —
  // it timed out here even once navigation had genuinely succeeded. `body.getHTML()` is the same
  // primitive the health check above already uses successfully, so polling it for the marker
  // filename is a more reliable proxy for "the listing rendered this entry".
  it("the seeded marker file is visible in the listing", async () => {
    const body = await $("body");
    await browser.waitUntil(
      async () => {
        const html = await body.getHTML({ includeSelectorTag: false });
        return html.includes(MARKER_NAME);
      },
      {
        timeout: 15_000,
        timeoutMsg: `expected <body> to contain the marker filename "${MARKER_NAME}"`,
      },
    );
  });
});
