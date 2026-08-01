// CPE-1237 — headless GUI smoke: the Gallery view's streaming thumbnail client renders REAL tiles for
// a mixed-format folder (PNG + SVG) through the backend's `thumb_queue`/`thumb_cache` pipeline, and
// gracefully falls back to the type icon for a format that fails to decode. Drives the REAL built app.
//
// Reachability: navigates into the dedicated `CPE-1237-thumbnail-gallery` subfolder (wdio.conf.ts's
// `seedThumbnailGalleryFixture` — isolated from the other fixtures the same way CPE-1207's link folder
// is), switches to Gallery view via the Command Palette ("View: Gallery" — `view.gallery` in
// App.svelte), and waits for the streamed `.thumb-img` tiles to appear. This is also how a human/the
// Visual Critic reaches the surface manually: open any folder with images/SVGs, Ctrl+Shift+P → type
// "gallery" → Enter (or the View menu's Gallery item).
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, $$, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";
import { doubleClick, type Point } from "../lib/mouse.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");
// Seeded literals duplicated here rather than imported across the runner/worker boundary — matches this
// suite's existing convention (see context-menu.smoke.ts's identical note on EMPTY_DIR_NAME).
const THUMB_GALLERY_DIR_NAME = "CPE-1237-thumbnail-gallery";
const THUMB_GALLERY_PNG_NAME = "CPE-1237-photo.png";
const THUMB_GALLERY_SVG_NAME = "CPE-1237-icon.svg";
const THUMB_GALLERY_BADFONT_NAME = "CPE-1237-bad.ttf";

/** Viewport-space centre of the FIRST `.row` whose rendered HTML contains `name` — the same getHTML-scan
 *  primitive every other spec in this suite uses (script-injected text locators don't reliably resolve
 *  against wry's classic-WebDriver webview). */
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

describe("CPE-1237 — headless GUI smoke: streaming thumbnail client renders a mixed-format gallery", () => {
  before(() => {
    JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir: string };
  });

  afterEach(async function () {
    await snapFailure(this.currentTest, "thumbnail-gallery");
  });

  it("navigates into the seeded gallery folder and switches to Gallery view", async () => {
    const crumb = await $('[aria-current="page"]');
    await crumb.waitForExist({ timeout: 30_000 });
    await crumb.click(); // move focus off any input, same as column-picker.smoke.ts

    const folderPoint = await pointOfRowNamed(THUMB_GALLERY_DIR_NAME);
    expect(folderPoint, `expected a row for the seeded "${THUMB_GALLERY_DIR_NAME}"`).to.not.equal(null);
    await doubleClick(folderPoint!);

    // Confirm the navigation landed (the three seeded files list) before switching views.
    await browser.waitUntil(
      async () => {
        const html = await (await $(".filelist-pane")).getHTML({ includeSelectorTag: false });
        return html.includes(THUMB_GALLERY_PNG_NAME) && html.includes(THUMB_GALLERY_SVG_NAME);
      },
      { timeout: 15_000, timeoutMsg: "expected the seeded PNG + SVG rows to list after navigating in" },
    );

    // Ctrl+Shift+P → "gallery" → Enter switches to the Gallery view (`view.gallery` palette command),
    // the same opener column-picker.smoke.ts/new-link.smoke.ts use for their own commands.
    await browser.keys(["Control", "Shift", "P"]);
    const paletteInput = await $(".cp-input");
    await paletteInput.waitForExist({ timeout: 10_000, timeoutMsg: "expected .cp-input after Ctrl+Shift+P" });
    await paletteInput.addValue("gallery");

    let galleryRow: WebdriverIO.Element | undefined;
    await browser.waitUntil(
      async () => {
        const rows = $$(".cp-row");
        for await (const row of rows) {
          if ((await row.getHTML({ includeSelectorTag: false })).includes("Gallery")) {
            galleryRow = row;
            return true;
          }
        }
        return false;
      },
      { timeout: 10_000, timeoutMsg: 'expected a .cp-row labelled "View: Gallery"' },
    );
    expect(galleryRow, 'expected a .cp-row labelled "View: Gallery"').to.not.equal(undefined);
    await galleryRow!.waitForClickable({ timeout: 10_000 });
    await galleryRow!.click();

    // Gallery view renders a `.thumb` tile for every hasThumbnail()-eligible entry — all three seeded
    // files qualify (PNG is a photo, SVG + the bad-extension .ttf are CPE-1236's non-photo formats) —
    // so exactly 3 tiles mount, regardless of whether each one's decode ultimately succeeds.
    await browser.waitUntil(async () => (await $$(".thumb").length) === 3, {
      timeout: 15_000,
      timeoutMsg: "expected exactly 3 .thumb tiles (png + svg + bad-font) to mount in Gallery view",
    });
  });

  it("streams real tiles for the PNG + SVG through the priority queue, and falls back for the bad font", async () => {
    // The streaming client (CPE-1237) resolves asynchronously — poll until BOTH real formats have a
    // decoded `<img class="thumb-img">`, not just an icon. This is the end-to-end proof that
    // `thumbnails_stream` → `thumb_queue`/`thumb_cache` → the frontend cache actually delivered pixels,
    // not just that the tile slot exists.
    await browser.waitUntil(async () => (await $$(".thumb-img").length) >= 2, {
      timeout: 20_000,
      timeoutMsg: "expected at least 2 rendered .thumb-img tiles (the PNG + the SVG) to stream in",
    });

    // CPE-1148 Part A: capture the gallery now that real tiles have landed.
    await snap("thumbnail-gallery");

    // The bad-font tile never becomes a real image — it stays on its `Icon` fallback (graceful
    // fallback AC). Cross-check by tile count: exactly 3 `.thumb` tiles total, and image tiles never
    // exceed 2 (png + svg) even once everything has had time to settle.
    expect(await $$(".thumb").length, "tile count must stay at 3 (no extra/missing tiles)").to.equal(3);
    expect(
      await $$(".thumb-img").length,
      "the bad-font tile must never resolve to a real <img> — it has no decodable content",
    ).to.equal(2);
  });
});
