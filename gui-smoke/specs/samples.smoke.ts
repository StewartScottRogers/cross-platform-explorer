// CPE-1358 — headless GUI smoke: sample-navigation walk (epic CPE-1148, pairs with CPE-1357).
//
// Drives the real built app through EVERY file in the repo's `samples/` tree (a fresh copy seeded by
// `wdio.conf.ts#seedSamplesFixture` into `CPE-1358-samples/` inside the shared tmpDir — see that
// function's header comment for why a copy, not a second `--open`) and, for each one: navigates to its
// folder via the address bar, selects it, and asserts (a) the app/window is still alive and responding
// — the CRASH guard, the exact CPE-1357 regression class ("open a PDF and the app goes nuts and
// crashes") — and (b) the preview pane produced SOMETHING for its kind: a real content element (image/
// media/pdf/font/table/text/hex/data-grid) or an explicit graceful "can't preview this" note, never a
// stuck spinner forever.
//
// The list of files is discovered by walking the REAL `samples/` tree at spec-load time (not a
// hardcoded filename list), so a new sample fixture is automatically covered next run — the ticket's
// "drive it data-first off the sample tree" requirement. `src/lib/sampleCoverage.test.ts` is the
// companion headless ratchet asserting every supported preview KIND has a sample in the first place;
// this spec is the heavier end-to-end half, actually opening each one on a real build.
//
// `documents/malformed.pdf` — the ORIGINAL degenerate (0-page, no-xref) PDF that crashed the app
// (CPE-1357), preserved unchanged as its own fixture once `documents/doc.pdf` was replaced with a real,
// valid PDF (samples/README.md's "PDF fixtures" section) — is deliberately run LAST, in its own `it()`,
// rather than interleaved into the walk below. If it still crashes the shared app process today (before
// CPE-1357 lands), that failure must not prevent every OTHER sample in this walk from being exercised —
// interleaving it earlier would silently blind the rest of the coverage to whatever ordering the
// filesystem happens to return. Once CPE-1357 lands this assertion should pass like every other file.
//
// NOTE on blast radius: every spec in this suite shares ONE app session/process (see wdio.conf.ts's
// header comment). If `documents/malformed.pdf` genuinely still crashes the whole app today, spec files
// that sort alphabetically AFTER this one (saved-search, shred-dialog, similar-images, snapshot-*,
// spotlight, terminal-panel, thumbnail-gallery, transfer-panel, vault*) will cascade-fail too, for the
// same underlying reason — that is expected, not a separate regression, and self-resolves once CPE-1357
// fixes PDF-preview crash resilience. The whole `gui-smoke` job is non-blocking (`continue-on-error`,
// CPE-1048) on both CI legs, so this is a loud diagnostic, not a red `main`.
import { expect } from "chai";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { $, $$, browser } from "@wdio/globals";
import { snap, snapFailure } from "../lib/snap.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const STATE_FILE = path.resolve(__dirname, "..", ".smoke-state.json");

// Duplicated literal rather than importing from wdio.conf.ts — matches this harness's existing
// convention (see file-health.smoke.ts / declutter.smoke.ts's identical notes on seeded names).
const SAMPLES_DIR_NAME = "CPE-1358-samples";

// The REAL repo samples/ tree (not the seeded copy) — walked at spec-load time so the `it()` list is
// discovered from the actual fixture set, same tree `wdio.conf.ts#seedSamplesFixture` copies from.
const REAL_SAMPLES_DIR = path.resolve(__dirname, "..", "..", "samples");
const MALFORMED_PDF_REL = "documents/malformed.pdf";

/** Every regular file under `dir`, as `/`-joined paths relative to `REAL_SAMPLES_DIR`. Excludes
 *  `README.md` (documentation, not a fixture — matches `sampleCoverage.test.ts`'s exclusion) and
 *  `MALFORMED_PDF_REL` (run separately, last — see the header comment above). */
function listSampleFiles(dir: string): string[] {
  const out: string[] = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const abs = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      out.push(...listSampleFiles(abs));
    } else if (entry.isFile() && entry.name !== "README.md") {
      out.push(path.relative(REAL_SAMPLES_DIR, abs).split(path.sep).join("/"));
    }
  }
  return out;
}

const ALL_SAMPLES = listSampleFiles(REAL_SAMPLES_DIR);
const WALK_FILES = ALL_SAMPLES.filter((f) => f !== MALFORMED_PDF_REL);

// A definite "the preview rendered real content" indicator, one selector per PreviewKind's success
// markup (PreviewPane.svelte): image/decoded-image/raw-image/heic/dicom -> `.preview-img`; audio/video
// -> `.preview-media`; pdf -> `.preview-pdf`; font -> `.preview-font`; archive/csv/tsv ->
// `.preview-table-wrap`; markdown -> `.preview-markdown`; text/code -> `.code-view`; json AND the `info`
// kind's own success view -> a bare `pre.preview-text` (NEITHER wraps in `.preview-note` on success — no
// loading/error sibling to fall back to, so this selector is load-bearing, not decorative: without it,
// `waitForPreviewToSettle` would spin to its timeout and misreport a "stuck spinner" for every JSON/info
// sample even though the preview genuinely rendered); an active text editor -> `.preview-editor`; hex ->
// `HexView.svelte`'s `[data-testid="hexview"]`; data-grid -> `DataBrowser.svelte`'s `.data-browser`. All
// of these mount synchronously except for the async-loading kinds (decoded-image/raw-image/heic/dicom/
// archive/info/data-grid/json/csv/tsv/markdown/text), which show a transient `.preview-note` "Loading
// preview…" first — see `waitForPreviewToSettle` below.
const CONTENT_SELECTOR = [
  ".preview-img",
  ".preview-media",
  ".preview-pdf",
  ".preview-font",
  ".preview-table-wrap",
  ".preview-markdown",
  ".code-view",
  "pre.preview-text",
  ".preview-editor",
  '[data-testid="hexview"]',
  ".data-browser",
].join(", ");

// The exact English `pv.loading` string (PreviewPane.svelte / src/lib/i18n.ts) — the default locale in
// this harness (see file-health.smoke.ts's identical note). Used only to tell the TRANSIENT loading
// `.preview-note` apart from a TERMINAL one (e.g. "Can't preview this file").
const LOADING_TEXT = "Loading preview…";

/** Poll until the preview pane shows SOMETHING for the currently-selected file: either a definite
 *  content element ({@link CONTENT_SELECTOR}) or a terminal `.preview-note` (any note whose text isn't
 *  the transient loading string) — assertion (b), "rendered or gracefully degraded, never a stuck
 *  spinner forever". Throws (via `waitUntil`'s `timeoutMsg`) if neither ever appears within `timeoutMs`
 *  — the stuck-spinner failure mode this spec exists to catch. */
async function waitForPreviewToSettle(relPath: string, timeoutMs = 20_000): Promise<void> {
  await browser.waitUntil(
    async () => {
      if ((await $$(CONTENT_SELECTOR).length) > 0) return true;
      const notes = $$(".preview-note");
      for await (const note of notes) {
        const text = await note.getText();
        if (text && text !== LOADING_TEXT) return true;
      }
      return false;
    },
    {
      timeout: timeoutMs,
      timeoutMsg: `preview pane never settled for ${relPath} — still "${LOADING_TEXT}" (or nothing rendered) after ${timeoutMs}ms`,
    },
  );
}

/** Navigates the address bar directly to `absDir` (Ctrl+L -> type the absolute path -> Enter — the same
 *  "Type a path (Ctrl+L)" flow a user driving the address bar uses, NavToolbar.svelte), then waits for
 *  the breadcrumb to confirm the folder actually changed. Used instead of double-clicking through nested
 *  folders so every sample's own subdirectory (audio/, images/, documents/, …) is reachable in one step
 *  regardless of nesting depth. */
async function navigateTo(absDir: string): Promise<void> {
  await browser.keys(["Control", "l"]);
  const input = await $(".pathedit");
  await input.waitForExist({
    timeout: 10_000,
    timeoutMsg: "expected the address-bar path input (.pathedit) to appear after Ctrl+L",
  });
  await input.setValue(absDir);
  await browser.keys(["Enter"]);

  const expectedName = path.basename(absDir);
  await browser.waitUntil(
    async () => {
      const crumb = await $('[aria-current="page"]');
      if (!(await crumb.isExisting())) return false;
      return (await crumb.getText()) === expectedName;
    },
    {
      timeout: 15_000,
      timeoutMsg: `expected the breadcrumb to show "${expectedName}" after navigating to ${absDir}`,
    },
  );
}

/** The FIRST `.row` whose rendered HTML contains `name` — the same getHTML-scan idiom every other spec
 *  in this suite uses (declutter.smoke.ts / file-health.smoke.ts) rather than an exact-text locator
 *  (unreliable against wry under classic WebDriver — see open-dir.smoke.ts's note). */
async function findRowContaining(name: string): Promise<WebdriverIO.Element> {
  let found: WebdriverIO.Element | undefined;
  await browser.waitUntil(
    async () => {
      const rows = $$(".row");
      for await (const row of rows) {
        const html = await row.getHTML({ includeSelectorTag: false });
        if (html.includes(name)) {
          found = row;
          return true;
        }
      }
      return false;
    },
    { timeout: 15_000, timeoutMsg: `expected a .row containing "${name}"` },
  );
  return found!;
}

/** The crash guard (assertion a): the window/session must still be responding. If the renderer process
 *  died, this WebDriver call itself throws (session/window gone) rather than returning — that failure
 *  IS the CPE-1357 regression signal, not a bug in this helper. */
async function assertAppStillAlive(context: string): Promise<void> {
  const body = await $("body");
  expect(await body.isExisting(), `app/window did not respond after ${context}`).to.equal(true);
  const html = await body.getHTML({ includeSelectorTag: false });
  expect(html.trim().length, `app/window rendered empty content after ${context}`).to.be.greaterThan(0);
}

/** Navigates to `relPath`'s folder, selects the file, waits for its preview to settle (or gracefully
 *  degrade), then re-confirms the app is alive — the full per-file assertion this spec makes for every
 *  sample. `samplesRootAbs` is `CPE-1358-samples`'s absolute path inside the seeded tmpDir. `crashPauseMs`
 *  is the grace period given to an ASYNC crash (e.g. WebView2's embedded PDF plugin, CPE-1357) to
 *  manifest before declaring the app alive — most kinds mount synchronously and don't need much of a
 *  beat, but the `documents/malformed.pdf` regression call passes a longer one (this is the one file in
 *  the whole walk where "nothing threw yet" is the least trustworthy signal that nothing crashed). */
async function openAndVerify(samplesRootAbs: string, relPath: string, crashPauseMs = 600): Promise<void> {
  const dirAbs = path.join(samplesRootAbs, ...relPath.split("/").slice(0, -1));
  const fileName = relPath.split("/").at(-1)!;

  await navigateTo(dirAbs || samplesRootAbs);

  const row = await findRowContaining(fileName);
  await row.click(); // single click selects, feeding PreviewPane's `entry` prop (same as every other spec)

  await waitForPreviewToSettle(relPath);
  // Matches the `browser.pause` idiom already used elsewhere in this suite (archive-password.smoke.ts /
  // drive-menu.smoke.ts / home-item-menu.smoke.ts).
  await browser.pause(crashPauseMs);

  await assertAppStillAlive(`opening samples/${relPath}`);
}

describe("CPE-1358 — headless GUI smoke: every samples/ file opens without crashing the app", () => {
  let samplesRootAbs = "";

  before(() => {
    const state = JSON.parse(fs.readFileSync(STATE_FILE, "utf-8")) as { tmpDir?: string };
    if (!state.tmpDir) throw new Error("expected STATE_FILE to carry the seeded tmpDir");
    samplesRootAbs = path.join(state.tmpDir, SAMPLES_DIR_NAME);
  });

  afterEach(async function () {
    await snapFailure(this.currentTest, "samples-walk");
  });

  it("the seeded samples copy is non-empty (coverage sanity)", () => {
    expect(WALK_FILES.length, "expected at least one non-malformed sample file to walk").to.be.greaterThan(0);
  });

  for (const relPath of WALK_FILES) {
    it(`opens samples/${relPath}: no crash + preview renders or gracefully degrades`, async function () {
      await openAndVerify(samplesRootAbs, relPath);

      // A couple of representative frames for the Visual Critic gallery (CPE-1148) — not one per file
      // (25+ near-identical screenshots would be noise); the newly-added coverage fixtures + the fixed
      // PDF are the surfaces actually worth a human/Critic glance here.
      if (relPath === "documents/doc.pdf") await snap("samples-pdf-valid");
      if (relPath === "fonts/mini.ttf") await snap("samples-font");
      if (relPath === "database/mini.sqlite") await snap("samples-data-grid");
    });
  }

  // CPE-1357 regression repro — deliberately LAST, see the file header comment for why. A longer
  // crash-detection pause than the default: this is the one file in the walk where "nothing threw yet"
  // is the least trustworthy signal, so give WebView2's embedded PDF plugin more room to actually crash
  // before this spec declares victory.
  it(`opens samples/${MALFORMED_PDF_REL}: the CPE-1357 crash-regression guard`, async function () {
    await openAndVerify(samplesRootAbs, MALFORMED_PDF_REL, 2_000);
    await snap("samples-pdf-malformed");
  });
});
