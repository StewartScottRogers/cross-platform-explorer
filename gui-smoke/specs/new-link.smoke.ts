// CPE-1207 — headless GUI smoke: the New Link… dialog renders (render pin) and a real click-through
// creates a HARDLINK on disk. Drives the REAL built app.
//
// Hardlink, not symlink, by deliberate choice (matches the ticket's "unprivileged-safe" AC): creating a
// symlink on Windows needs Developer Mode or elevation (`create_symlink`'s own doc comment + the
// Windows-error-surfacing half of CPE-1207 is covered by NewLinkDialog.test.ts's mocked-rejection unit
// test instead), whereas `std::fs::hard_link` needs no special privilege on any of the three CI OSes —
// so this is the one link kind this headless click-through can create for real without touching platform
// permissions.
//
// The link target is the seeded marker file (`CPE-1045-marker.txt`, open-dir.smoke.ts) at the tmpDir
// root; the link itself is created inside a DEDICATED empty subfolder (`CPE-1207-new-link-folder`,
// seeded by wdio.conf.ts's `seedNewLinkFixture`) so this spec's write can never perturb another spec's
// fixture — same isolation reasoning as CPE-1154's own empty-folder fixture.
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, $$, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";
import { rightClick, doubleClick, click, type Point } from "../lib/mouse.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");
// Seeded literals duplicated here rather than imported across the runner/worker boundary — matches
// this suite's existing convention (see context-menu.smoke.ts's identical note on EMPTY_DIR_NAME).
const MARKER_NAME = "CPE-1045-marker.txt";
const NEW_LINK_DIR_NAME = "CPE-1207-new-link-folder";
const HARDLINK_NAME = "CPE-1207-hardlink.txt";

/** Viewport-space centre of the FIRST `.row`/`.rows .row` whose rendered HTML contains `name`, or
 *  `null` if none matched — the same getHTML-scan primitive every other spec in this suite uses
 *  (script-injected text locators don't reliably resolve against wry's classic-WebDriver webview). */
async function pointOfRowNamed(name: string): Promise<Point | null> {
  const rows = $$(".row");
  for await (const row of rows) {
    if ((await row.getHTML({ includeSelectorTag: false })).includes(name)) {
      const loc = await row.getLocation();
      const size = await row.getSize();
      return { x: Math.round(loc.x + Math.min(60, size.width / 2)), y: Math.round(loc.y + size.height / 2) };
    }
  }
  return null;
}

/** Viewport-space centre of the FIRST element matching `selector` whose HTML contains `text`. */
async function pointByText(selector: string, text: string): Promise<Point | null> {
  const els = $$(selector);
  for await (const el of els) {
    if ((await el.getHTML({ includeSelectorTag: false })).includes(text)) {
      const loc = await el.getLocation();
      const size = await el.getSize();
      return { x: Math.round(loc.x + size.width / 2), y: Math.round(loc.y + size.height / 2) };
    }
  }
  return null;
}

describe("CPE-1207 — headless GUI smoke: New Link… dialog renders + a hardlink click-through lists", () => {
  let tmpDir = "";

  before(() => {
    ({ tmpDir } = JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir: string });
  });

  afterEach(async function () {
    await snapFailure(this.currentTest, "new-link");
  });

  it("navigates into the dedicated empty folder", async () => {
    const crumb = await $('[aria-current="page"]');
    await crumb.waitForExist({ timeout: 30_000 });
    await crumb.click(); // move focus off any input, same as column-picker.smoke.ts

    const folderPoint = await pointOfRowNamed(NEW_LINK_DIR_NAME);
    expect(folderPoint, `expected a row for the seeded "${NEW_LINK_DIR_NAME}"`).to.not.equal(null);
    await doubleClick(folderPoint!);

    const emptyState = await $(".empty-state");
    await emptyState.waitForExist({
      timeout: 15_000,
      timeoutMsg: "expected the .empty-state box to render after entering the seeded empty folder",
    });
  });

  it("New ▸ New Link… renders the dialog (render pin)", async () => {
    const pane = await $(".filelist-pane");
    const loc = await pane.getLocation();
    const size = await pane.getSize();
    await rightClick({ x: Math.round(loc.x + size.width / 2), y: Math.round(loc.y + 12) });

    const ctx = await $(".ctx");
    await ctx.waitForExist({ timeout: 10_000, timeoutMsg: "expected the empty-area menu (.ctx) to open" });

    const newPoint = await pointByText(".ctx .parent", "New");
    expect(newPoint, 'expected the "New ▸" submenu parent row').to.not.equal(null);
    await click(newPoint!);

    const flyout = await $(".ctx .flyout");
    await flyout.waitForExist({ timeout: 5_000, timeoutMsg: "expected the New ▸ flyout to open" });

    const newLinkPoint = await pointByText(".ctx .flyout .row", "New Link…");
    expect(newLinkPoint, 'expected a "New Link…" row in the New ▸ flyout').to.not.equal(null);
    await click(newLinkPoint!);

    const dialog = await $('.dialog[aria-label="New link"]');
    await dialog.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected the New Link dialog to render after choosing New ▸ New Link…",
    });

    // Its three fields are all present — the render pin proper.
    expect(await (await $('[data-testid="new-link-kind"]')).isExisting()).to.equal(true);
    expect(await (await $('[data-testid="new-link-target"]')).isExisting()).to.equal(true);
    expect(await (await $('[data-testid="new-link-name"]')).isExisting()).to.equal(true);

    // CPE-1148 Part A: capture the empty dialog before the click-through below fills it in.
    await snap("new-link");
  });

  it("filling Hardlink + target + name and clicking Create makes a real hardlink that then lists", async () => {
    const kindField = await $('[data-testid="new-link-kind"]');
    await kindField.selectByAttribute("value", "hardlink");

    const targetField = await $('[data-testid="new-link-target"]');
    await targetField.setValue(path.join(tmpDir, MARKER_NAME));

    const nameField = await $('[data-testid="new-link-name"]');
    await nameField.setValue(HARDLINK_NAME);

    await (await $('[data-testid="new-link-create"]')).click();

    // Success closes the dialog (App.svelte's `onNewLinkCreated`) and reloads the folder.
    const dialog = await $('.dialog[aria-label="New link"]');
    await dialog.waitForExist({
      timeout: 15_000,
      reverse: true,
      timeoutMsg: "expected the New Link dialog to close after a successful hardlink create",
    });

    // The fresh listing lands the new hardlink straight into inline-rename (mirrors createNewItem's
    // create-then-rename flow, `pendingRenamePath` in App.svelte) — assert the rename input carries the
    // created name, then press Enter to commit it (same name in, same name out) and settle on the
    // plain row so "that then lists" is unambiguous.
    const renameInput = await $("input.rename");
    await renameInput.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected the new hardlink to land in inline-rename mode after creation",
    });
    expect(await renameInput.getValue()).to.equal(HARDLINK_NAME);
    await browser.keys(["Enter"]);

    await browser.waitUntil(
      async () => (await pointOfRowNamed(HARDLINK_NAME)) !== null,
      { timeout: 10_000, timeoutMsg: `expected a listed row for the created hardlink "${HARDLINK_NAME}"` },
    );

    // Genuinely on disk, not just in the DOM: a real hardlink shares the marker's inode/data, so its
    // content matches even though nothing wrote to it directly.
    const linkContent = fs.readFileSync(path.join(tmpDir, NEW_LINK_DIR_NAME, HARDLINK_NAME), "utf-8");
    const markerContent = fs.readFileSync(path.join(tmpDir, MARKER_NAME), "utf-8");
    expect(linkContent).to.equal(markerContent);

    await snap("new-link-created");
  });
});
