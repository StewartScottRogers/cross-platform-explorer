// CPE-1513 (epic CPE-1498) — headless GUI smoke SCAFFOLD for the Network sidebar section, the visible
// entry point for the SFTP/WebDAV backend (CPE-1510 keychain secrets + CPE-1511 remote `list_dir`
// routing). This is a structural scaffold only, per the ticket's explicit instruction: it proves the
// entry point renders and the add-connection popover opens/closes with its expected fields — it does
// NOT drive a real remote connect (no live SFTP/WebDAV server in this harness), does not exercise the
// row context menu, and is NOT the visual/interaction sign-off. That still needs the Visual Critic
// (screenshots of the real built app) or the user's attended eyes — see CPE-1513's Work Log.
//
// Guaranteed starting state, mirroring instant-search.smoke.ts's reasoning: this harness launches a
// brand-new app process per spec file with no saved `connections.json` and (headless CI) no OS-enumerated
// network shares, so the Network SECTION itself (which needs a saved connection or a discovered share
// to show — hidden/empty per the ticket) never renders here. The one FALSIFIABLE, non-flaky affordance
// available on a fresh process is the always-visible "Network…" entry-point row under Explore.
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");

describe("CPE-1513 — headless GUI smoke: Network sidebar entry point + add-connection popover", () => {
  before(() => {
    JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir: string };
  });

  afterEach(async function () {
    await snapFailure(this.currentTest, "network");
  });

  it("the always-visible 'Network…' entry point renders under Explore on a fresh app process", async () => {
    // Wait for the initial navigation to settle (parity with instant-search.smoke.ts) before looking
    // for sidebar content.
    const crumb = await $('[aria-current="page"]');
    await crumb.waitForExist({ timeout: 30_000 });

    const entryPoint = await $('button[title="Add a saved SFTP/WebDAV connection"]');
    await entryPoint.waitForExist({
      timeout: 15_000,
      timeoutMsg: "expected the 'Network…' entry-point row (Explore section) to render",
    });
    expect(await entryPoint.isDisplayed(), "expected the Network entry point to be visible").to.equal(true);
    expect(await entryPoint.getText()).to.include("Network");

    await snap("network-entry-point");
  });

  it("clicking the entry point opens the add-connection popover with its expected fields, and Escape closes it", async () => {
    const entryPoint = await $('button[title="Add a saved SFTP/WebDAV connection"]');
    await entryPoint.waitForExist({ timeout: 15_000 });
    await entryPoint.click();

    const form = await $('[aria-label="Add a connection"]');
    await form.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected the add-connection popover (NetworkConnectionForm) to render after clicking 'Network…'",
    });
    expect(await form.isDisplayed(), "expected the add-connection popover to be visible").to.equal(true);

    // Falsifiable field checks, not just "a container rendered" — the protocol dropdown defaults to
    // sftp/webdav (network.ts's SUPPORTED_SCHEMES) and the auth choice defaults to Password.
    const html = await form.getHTML({ includeSelectorTag: false });
    expect(html).to.include("sftp");
    expect(html).to.include("webdav");
    expect(html).to.include("Password");
    expect(html).to.include("Key file");

    await snap("network-add-form");

    // Escape closes it (the shared popover convention — MENUS.md / dialogs-need-visible-border).
    await browser.keys("Escape");
    await form.waitForExist({
      timeout: 5_000,
      reverse: true,
      timeoutMsg: "expected the add-connection popover to close on Escape",
    });
  });
});
