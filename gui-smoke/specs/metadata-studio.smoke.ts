// CPE-1331 — headless GUI smoke for the Metadata Studio dialog (MetadataStudioDialog.svelte, CPE-1041,
// epic CPE-725): drives the real built app, selects the seeded ID3-tagged "mp3" (a plain click, exactly
// like batch-media.smoke.ts's `rowA.click()`), opens the dialog via its real opener — the Command
// Palette (Ctrl+Shift+P → "Metadata Studio…", the same `file.metadataStudio` command the right-click
// context menu item is wired to, see App.svelte's `paletteCommands` / `openMetadataStudio`) — and
// asserts the two seeded ID3 text frames render as EDITABLE fields (real `input.v` elements bound to
// real values, not just static text) over a real `metadata_read` / `metadata_writable` IPC round trip.
//
// This is the sprint QA end-to-end proof this dialog never had: MetadataStudioDialog.test.ts already
// covers the checkpoint-before-save / batch-op / revert logic in isolation with a mocked `invoke`, but
// never against the real backend codec (crates/server/src/media_meta.rs's `read_id3v2`/`is_writable`).
//
// Fixture (wdio.conf.ts#seedMetadataStudioFixture): a hand-built, byte-accurate ID3v2.3 tag with two
// text frames (TIT2 "title", TPE1 "artist") — no real MPEG audio payload is needed, because
// `read_id3v2` parses purely off the leading `ID3` header + frames (see that fixture's own header
// comment for the exact byte layout). `is_writable("mp3")` is unconditional on extension, so this
// fixture is genuinely writable, unlike a read-only format.
//
// Non-destructive: this spec never clicks Save — it only asserts the fields render, then dismisses via
// the Close button, so the seeded file on disk is never touched.
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, $$, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";
import { click, doubleClick, type Point } from "../lib/mouse.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");

// Duplicated literals rather than importing from wdio.conf.ts — matches this harness's existing
// convention (see near-duplicates.smoke.ts / batch-media.smoke.ts's identical notes).
const METADATA_STUDIO_DIR_NAME = "metadata-studio-gui-verify";
const METADATA_STUDIO_MP3_NAME = "CPE-1331-song.mp3";
const METADATA_STUDIO_TITLE = "CPE-1331 Song Title";
const METADATA_STUDIO_ARTIST = "CPE-1331 Artist";

/** Viewport-space centre of the FIRST `.row` whose rendered HTML contains `name` — the same
 *  getHTML-scan + CDP-click primitive thumbnail-gallery.smoke.ts / pdf-video-thumbnails.smoke.ts use.
 *  vault.smoke.ts's header notes WebdriverIO's own `.doubleClick()` "intermittently registers" against
 *  this stack (tauri-driver → msedgedriver → wry/WebView2) — this real-hit-testing CDP double-click
 *  (lib/mouse.ts) is the harness's proven-reliable replacement. */
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

describe("CPE-1331 — headless GUI smoke: Metadata Studio dialog renders real editable ID3 fields", () => {
  before(() => {
    JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir: string };
  });

  // CPE-1149: on a failing run, leave a shot of the state it failed in (`metadata-studio-fail.png`) —
  // the inline `snap("metadata-studio")` below is only reached on a pass. Non-arrow fn so Mocha binds
  // `this`; `snapFailure` is a no-op on a pass and swallows its own errors.
  afterEach(async function () {
    await snapFailure(this.currentTest, "metadata-studio");
  });

  it("selects the seeded mp3, opens via the command palette, and renders its editable ID3 fields", async () => {
    // Wait for the initial `--open=<tmpDir>` navigation so the file list is settled before we start
    // clicking rows (also asserted in open-dir.smoke.ts).
    const crumb = await $('[aria-current="page"]');
    await crumb.waitForExist({ timeout: 30_000 });
    await crumb.click(); // plain, non-interactive <span> — just moves focus off any input

    // The fixture is seeded into its own dedicated subfolder (wdio.conf.ts#seedMetadataStudioFixture,
    // same isolation reasoning as the CPE-1241/1226/1249 fixtures — see thumbnail-gallery.smoke.ts's
    // identical navigate-in pattern), so navigate in before looking for the mp3's own row.
    const folderPoint = await pointOfRowNamed(METADATA_STUDIO_DIR_NAME);
    expect(folderPoint, `expected a row for the seeded "${METADATA_STUDIO_DIR_NAME}"`).to.not.equal(null);
    await doubleClick(folderPoint!);

    // Confirm the navigation actually landed (the seeded mp3 lists) before selecting it — mirrors
    // thumbnail-gallery.smoke.ts's post-navigate wait.
    await browser.waitUntil(
      async () => {
        const html = await (await $(".filelist-pane")).getHTML({ includeSelectorTag: false });
        return html.includes(METADATA_STUDIO_MP3_NAME);
      },
      { timeout: 15_000, timeoutMsg: "expected the seeded mp3 row to list after navigating in" },
    );

    // `file.metadataStudio` is `enabled: hasSelection` (App.svelte) — select the seeded mp3 with a
    // plain click FIRST, or the command palette row renders greyed (`.cp-row.disabled`, CommandPalette
    // .svelte) and a click on it is a deliberate no-op (`if (!isEnabled(...)) return`, commandPalette.ts).
    const filePoint = await pointOfRowNamed(METADATA_STUDIO_MP3_NAME);
    expect(filePoint, `expected a row for the seeded "${METADATA_STUDIO_MP3_NAME}"`).to.not.equal(null);
    await click(filePoint!);
    await browser.waitUntil(async () => (await $$(".row.selected").length) === 1, {
      timeout: 10_000,
      timeoutMsg: "expected the seeded mp3's row to be selected before opening the command palette",
    });

    // Ctrl+Shift+P opens the Command Palette (App.svelte's `handleKeydown`).
    await browser.keys(["Control", "Shift", "P"]);
    const paletteInput = await $(".cp-input");
    await paletteInput.waitForExist({
      timeout: 10_000,
      timeoutMsg: "expected .cp-input (Command Palette) to render after Ctrl+Shift+P",
    });
    await paletteInput.addValue("metadata studio");

    let studioRow: WebdriverIO.Element | undefined;
    await browser.waitUntil(
      async () => {
        const rows = $$(".cp-row");
        for await (const r of rows) {
          const html = await r.getHTML({ includeSelectorTag: false });
          if (html.includes("Metadata Studio")) {
            studioRow = r;
            return true;
          }
        }
        return false;
      },
      { timeout: 10_000, timeoutMsg: 'expected a .cp-row labelled "Metadata Studio…" to appear' },
    );
    expect(studioRow, 'expected a .cp-row labelled "Metadata Studio…"').to.not.equal(undefined);
    // Enabled (not `.cp-row.disabled`) precisely because the mp3 row was selected above.
    const studioRowClass = await studioRow!.getAttribute("class");
    expect(studioRowClass, "expected the Metadata Studio row to be enabled, not greyed").to.not.include("disabled");
    await studioRow!.waitForClickable({ timeout: 10_000 });
    await studioRow!.click();

    // The dialog mounted (aria-label is the localized title; default locale is English) scoped to the
    // seeded file.
    const dialog = await $('[aria-label="Metadata Studio"]');
    await dialog.waitForExist({ timeout: 10_000, timeoutMsg: "expected the Metadata Studio dialog to render" });
    const dialogHtml = await dialog.getHTML({ includeSelectorTag: false });
    expect(dialogHtml, "expected the dialog to report the seeded mp3's filename").to.include(METADATA_STUDIO_MP3_NAME);
    // Not the read-only badge — `is_writable("mp3")` is unconditional (media_meta.rs), so this fixture
    // must be reported writable.
    expect(dialogHtml, "expected the mp3 fixture to NOT be reported view-only").to.not.include("View only");

    // Core assertion (CPE-1331): a real `metadata_read` IPC round trip parsed the seeded ID3v2 tag into
    // editable `MetaField`s — at least one `[data-testid="ms-field-input"]` renders — the FALSIFIABLE
    // check tied to this spec's own fixture. If the read returned nothing (broken invoke, bad path, or
    // a codec regression) `studio.noMeta` would render instead and no ms-field-input would exist, so
    // this fails loudly rather than passing on an empty view.
    await browser.waitUntil(async () => (await $$('[data-testid="ms-field-input"]').length) > 0, {
      timeout: 10_000,
      timeoutMsg: "expected at least one [data-testid='ms-field-input'] to render",
    });
    // The row LABELS come from `friendly_key` (media_meta_read.rs: TIT2 -> "Title", TPE1 -> "Artist"),
    // rendered as static markup — but each field's VALUE is bound via Svelte's `value={...}` property
    // binding, a live DOM property the browser never reflects back into `outerHTML`/`getHTML()`, so the
    // seeded strings themselves have to be asserted via `getValue()` below, not a markup scan.
    const fieldsEl = await $('[data-testid="ms-fields"]');
    const fieldsHtml = await fieldsEl.getHTML({ includeSelectorTag: false });
    expect(fieldsHtml, "expected a Title field row to render").to.include(">Title<");
    expect(fieldsHtml, "expected an Artist field row to render").to.include(">Artist<");

    const inputs = $$('[data-testid="ms-field-input"]');
    const values: string[] = [];
    for await (const input of inputs) values.push(await input.getValue());
    expect(values, "expected the Title's input value to be the seeded, editable ID3 value").to.include(
      METADATA_STUDIO_TITLE,
    );
    expect(values, "expected the Artist's input value to be the seeded, editable ID3 value").to.include(
      METADATA_STUDIO_ARTIST,
    );

    // CPE-1148/1149: capture the rendered editable-fields state for the Visual Critic.
    await snap("metadata-studio");

    // Non-destructive — never click Save — dismiss via the close button.
    const closeBtn = await $('[data-testid="ms-close-btn"]');
    await closeBtn.click();
  });
});
