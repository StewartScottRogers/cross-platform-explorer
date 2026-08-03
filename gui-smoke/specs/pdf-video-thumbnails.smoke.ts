// CPE-1268 — headless GUI smoke: the thumbnail grid renders REAL tiles for a PDF and a video, the two
// formats CPE-1267 had silently dropped from the frontend `hasThumbnail` gate (src/lib/filetypes.ts).
// Drives the REAL built app through the same streaming thumbnail pipeline the PNG/SVG gallery pin
// (thumbnail-gallery.smoke.ts) exercises — this is its pdf/video sibling.
//
// Why this exists: CPE-1267 was a pure frontend-gate drift — the grid never even REQUESTED pdf/video
// thumbnails though the backend could render them. The always-run backstop for that is the two-sided
// parity guard (src/lib/filetypes.test.ts ↔ crates/server/src/thumb_source.rs). This spec is the
// end-to-end GUI proof.
//
// Native-dep reality (see wdio.conf.ts `seedThumbnailFormatsFixture`): the smoke binary is an unbundled
// `--no-bundle` build, so pdfium/ffmpeg resolve via the OS search path, not `bundle.resources`. ffmpeg
// is reliably on PATH in CI + dev, so the VIDEO tile renders a real `.thumb-img` (hard-asserted when the
// fixture was seeded). pdfium is often absent, so the PDF tile's real-render is best-effort (logged),
// while its tile MOUNT — the exact CPE-1267 regression surface (the grid asking for it at all) — is
// asserted unconditionally. gui-smoke is non-blocking regardless (CPE-1048).
//
// Reachability (same as thumbnail-gallery.smoke.ts): open the seeded subfolder, Ctrl+Shift+P → "gallery"
// → Enter (the `view.gallery` palette command / the View menu's Gallery item).
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
// suite's existing convention (see thumbnail-gallery.smoke.ts / context-menu.smoke.ts).
const THUMB_FORMATS_DIR_NAME = "CPE-1268-pdf-video-thumbnails";
const THUMB_FORMATS_PDF_NAME = "CPE-1268-doc.pdf";
const THUMB_FORMATS_VIDEO_NAME = "CPE-1268-clip.mp4";

type SmokeState = { tmpDir: string; thumbFormatsFixture?: { pdf: boolean; video: boolean } };

/** Viewport-space centre of the FIRST `.row` whose rendered HTML contains `name` — the same getHTML-scan
 *  primitive every other spec in this suite uses. */
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

describe("CPE-1268 — headless GUI smoke: pdf + video render real thumbnail tiles (guards CPE-1267)", () => {
  let fixture: { pdf: boolean; video: boolean } = { pdf: true, video: false };

  before(() => {
    const state = JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as SmokeState;
    if (state.thumbFormatsFixture) fixture = state.thumbFormatsFixture;
  });

  afterEach(async function () {
    await snapFailure(this.currentTest, "pdf-video-thumbnails");
  });

  it("navigates into the seeded pdf/video folder and switches to Gallery view", async () => {
    const crumb = await $('[aria-current="page"]');
    await crumb.waitForExist({ timeout: 30_000 });
    await crumb.click(); // move focus off any input

    const folderPoint = await pointOfRowNamed(THUMB_FORMATS_DIR_NAME);
    expect(folderPoint, `expected a row for the seeded "${THUMB_FORMATS_DIR_NAME}"`).to.not.equal(null);
    await doubleClick(folderPoint!);

    // Confirm the navigation landed (the seeded PDF lists) before switching views.
    await browser.waitUntil(
      async () => {
        const html = await (await $(".filelist-pane")).getHTML({ includeSelectorTag: false });
        return html.includes(THUMB_FORMATS_PDF_NAME);
      },
      { timeout: 15_000, timeoutMsg: "expected the seeded PDF row to list after navigating in" },
    );

    // Ctrl+Shift+P → "gallery" → Enter switches to the Gallery view (`view.gallery` palette command).
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

    // The grid mounts a `.thumb` tile for every hasThumbnail()-eligible entry. With CPE-1267 fixed both
    // the PDF and (when seeded) the video qualify, so the expected tile count is pdf + maybe-video.
    const expectedTiles = 1 + (fixture.video ? 1 : 0);
    await browser.waitUntil(async () => (await $$(".thumb").length) === expectedTiles, {
      timeout: 15_000,
      timeoutMsg: `expected exactly ${expectedTiles} .thumb tile(s) (pdf${fixture.video ? " + video" : ""}) to mount — a pre-CPE-1267 gate would mount 0`,
    });
  });

  it("renders the video tile as a real <img> (ffmpeg on PATH), and the pdf tile best-effort", async () => {
    // Video: ffmpeg is reliably resolvable, so its frame must stream in as a real `.thumb-img`.
    if (fixture.video) {
      await browser.waitUntil(async () => (await $$(".thumb-img").length) >= 1, {
        timeout: 25_000,
        timeoutMsg: "expected the video tile to stream a real <img class='thumb-img'> (ffmpeg frame)",
      });
    } else {
      // eslint-disable-next-line no-console
      console.warn("[gui-smoke] CPE-1268: video fixture not seeded (no ffmpeg at seed) — skipping video render assertion");
    }

    await snap("pdf-video-thumbnails");

    // PDF: real render needs pdfium, which the unbundled smoke binary may not resolve. Best-effort —
    // log the outcome, never fail (the CPE-1267 regression itself is covered by the tile-MOUNT assertion
    // above + the always-run parity unit tests). If pdfium IS present, we expect BOTH tiles as images.
    const imgs = await $$(".thumb-img").length;
    const expectedIfPdfium = 1 + (fixture.video ? 1 : 0);
    if (imgs >= expectedIfPdfium) {
      // eslint-disable-next-line no-console
      console.log("[gui-smoke] CPE-1268: pdf tile rendered a real thumbnail too (pdfium present)");
    } else {
      // eslint-disable-next-line no-console
      console.warn("[gui-smoke] CPE-1268: pdf tile stayed on its icon fallback (pdfium not resolvable in this unbundled smoke build) — expected, non-fatal");
    }
  });
});
